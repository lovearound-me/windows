use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config;
use crate::error::{Error, Result};
use crate::signin_box::{Sealed, SignInKey};

/// A signed-in Firebase account. Only the refresh token is kept; ID tokens
/// live in memory.
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub uid: String,
    pub email: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
}

struct Inner {
    session: Option<Session>,
    id_token: Option<String>,
    expires_at: Instant,
    google_state: Option<String>,
    google_key: Option<SignInKey>,
}

/// Firebase Auth over its REST API: email+password directly, Google through
/// the site's /signin/ page, which hands the session back via lovearound://signin,
/// sealed to a one-time key of ours (signin_box.rs).
pub struct Auth {
    inner: Mutex<Inner>,
    /// One refresh at a time; the others wait for it and take its token.
    refreshing: tokio::sync::Mutex<()>,
    file: PathBuf,
    http: reqwest::Client,
}

impl Auth {
    pub fn new(dir: PathBuf, http: reqwest::Client) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("session.json");
        let session = std::fs::read(&file).ok().and_then(|data| serde_json::from_slice(&data).ok());
        Auth {
            inner: Mutex::new(Inner { session, id_token: None, expires_at: Instant::now(), google_state: None, google_key: None }),
            refreshing: tokio::sync::Mutex::new(()),
            file,
            http,
        }
    }

    pub fn session(&self) -> Option<Session> {
        self.inner.lock().unwrap().session.clone()
    }

    fn cached(&self) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        match &inner.id_token {
            Some(token) if inner.expires_at > Instant::now() + Duration::from_secs(300) => Some(token.clone()),
            _ => None,
        }
    }

    /// A valid ID token, refreshed when it is about to expire.
    pub async fn token(&self, force_refresh: bool) -> Result<String> {
        if !force_refresh {
            if let Some(token) = self.cached() {
                return Ok(token);
            }
        }
        let _guard = self.refreshing.lock().await;
        if !force_refresh {
            // Refreshed by whoever held the lock before.
            if let Some(token) = self.cached() {
                return Ok(token);
            }
        }
        self.refresh().await
    }

    async fn refresh(&self) -> Result<String> {
        let refresh_token = self.session().ok_or(Error::SignedOut)?.refresh_token;
        let req = self
            .http
            .post(format!("https://securetoken.googleapis.com/v1/token?key={}", config::FIREBASE_API_KEY))
            .form(&[("grant_type", "refresh_token"), ("refresh_token", refresh_token.as_str())]);
        let json = call(req).await?;
        let (Some(id), Some(new_refresh)) = (json["id_token"].as_str(), json["refresh_token"].as_str()) else {
            return Err(Error::Firebase("BAD_RESPONSE".into()));
        };
        let expires_in = json["expires_in"].as_str().and_then(|s| s.parse().ok()).unwrap_or(3600);
        let mut inner = self.inner.lock().unwrap();
        inner.id_token = Some(id.to_string());
        inner.expires_at = Instant::now() + Duration::from_secs(expires_in);
        let changed = match inner.session.as_mut() {
            Some(session) if session.refresh_token != new_refresh => {
                session.refresh_token = new_refresh.to_string();
                true
            }
            _ => false,
        };
        if changed {
            self.save(&inner);
        }
        Ok(id.to_string())
    }

    // ---- email + password ---------------------------------------------------

    /// Signs in, or creates the account when the email is new.
    pub async fn sign_in(&self, email: &str, password: &str) -> Result<()> {
        match self.email_call("signInWithPassword", email, password).await {
            Err(Error::Firebase(code)) if code.starts_with("EMAIL_NOT_FOUND") || code.starts_with("INVALID_LOGIN_CREDENTIALS") => {
                // With email enumeration protection both cases look the same:
                // try to create the account, and an existing email means a wrong password.
                match self.email_call("signUp", email, password).await {
                    Err(Error::Firebase(code)) if code.starts_with("EMAIL_EXISTS") => Err(Error::Firebase("INVALID_PASSWORD".into())),
                    other => other,
                }
            }
            other => other,
        }
    }

    pub async fn send_password_reset(&self, email: &str) -> Result<()> {
        self.post("accounts:sendOobCode", json!({ "requestType": "PASSWORD_RESET", "email": email })).await?;
        Ok(())
    }

    async fn email_call(&self, method: &str, email: &str, password: &str) -> Result<()> {
        let json = self
            .post(&format!("accounts:{method}"), json!({ "email": email, "password": password, "returnSecureToken": true }))
            .await?;
        let (Some(uid), Some(id), Some(refresh_token)) =
            (json["localId"].as_str(), json["idToken"].as_str(), json["refreshToken"].as_str())
        else {
            return Err(Error::Firebase("BAD_RESPONSE".into()));
        };
        let email = json["email"].as_str().unwrap_or(email).to_string();
        self.adopt(Session { uid: uid.into(), email: Some(email), refresh_token: refresh_token.into() });
        let expires_in = json["expiresIn"].as_str().and_then(|s| s.parse().ok()).unwrap_or(3600);
        let mut inner = self.inner.lock().unwrap();
        inner.id_token = Some(id.into());
        inner.expires_at = Instant::now() + Duration::from_secs(expires_in);
        Ok(())
    }

    // ---- Google, through the site -------------------------------------------

    /// The page that signs in with Google and returns to lovearound://signin,
    /// with the session sealed to our one-time key.
    pub fn google_sign_in_url(&self) -> String {
        let state = uuid::Uuid::new_v4().to_string().to_uppercase();
        let key = SignInKey::new();
        let url = format!("{}/signin?state={state}&key={}", config::SITE, key.public());
        let mut inner = self.inner.lock().unwrap();
        inner.google_state = Some(state);
        inner.google_key = Some(key);
        url
    }

    /// lovearound://signin?state=…&epk=…&box=…, the box holding uid, email and
    /// the refresh token. The site before the box sent them in the clear
    /// (needed://signin?state=…&uid=…&email=…&rt=…); that is still accepted
    /// until the site with the box is published.
    pub async fn complete_google(&self, url: &url::Url) -> Result<()> {
        let q: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
        let sealed = {
            let mut inner = self.inner.lock().unwrap();
            // The state proves the callback answers our own request, so a crafted
            // link cannot sign the app into someone else's account.
            let Some(state) = inner.google_state.clone().filter(|s| q.get("state") == Some(s)) else {
                return Err(Error::Firebase("BAD_CALLBACK".into()));
            };
            let sealed = match q.get("box") {
                Some(sealed) => inner
                    .google_key
                    .as_ref()
                    .and_then(|key| key.open(q.get("epk").map_or("", |s| s.as_str()), sealed, &state)),
                None => Some(Sealed {
                    uid: q.get("uid").cloned().unwrap_or_default(),
                    email: q.get("email").cloned().unwrap_or_default(),
                    rt: q.get("rt").cloned().unwrap_or_default(),
                }),
            };
            let Some(sealed) = sealed.filter(|s| !s.uid.is_empty() && !s.rt.is_empty()) else {
                return Err(Error::Firebase("BAD_CALLBACK".into()));
            };
            inner.google_state = None;
            inner.google_key = None;
            sealed
        };
        self.adopt(Session {
            uid: sealed.uid,
            email: Some(sealed.email).filter(|s| !s.is_empty()),
            refresh_token: sealed.rt,
        });
        self.token(true).await?;
        Ok(())
    }

    // ---- session ------------------------------------------------------------

    pub fn sign_out(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.session = None;
        inner.id_token = None;
        let _ = std::fs::remove_file(&self.file);
    }

    fn adopt(&self, session: Session) {
        let mut inner = self.inner.lock().unwrap();
        inner.session = Some(session);
        inner.id_token = None;
        inner.expires_at = Instant::now();
        self.save(&inner);
    }

    /// %APPDATA% is the user's own: no one else can read the file.
    fn save(&self, inner: &Inner) {
        if let Some(data) = inner.session.as_ref().and_then(|s| serde_json::to_vec(s).ok()) {
            let tmp = self.file.with_extension("tmp");
            if std::fs::write(&tmp, data).is_ok() {
                let _ = std::fs::rename(&tmp, &self.file);
            }
        }
    }

    async fn post(&self, method: &str, body: Value) -> Result<Value> {
        let req = self
            .http
            .post(format!("https://identitytoolkit.googleapis.com/v1/{method}?key={}", config::FIREBASE_API_KEY))
            .json(&body);
        call(req).await
    }
}

async fn call(req: reqwest::RequestBuilder) -> Result<Value> {
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    let json: Value = resp.json().await.unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let code = json["error"]["message"].as_str().map(str::to_string).unwrap_or(format!("HTTP_{status}"));
        return Err(Error::Firebase(code));
    }
    Ok(json)
}
