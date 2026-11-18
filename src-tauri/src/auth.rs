use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;
use crate::error::{Error, Result};
use crate::signin_box::SignInKey;

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
    pending: Option<Pending>,
}

/// A sign-in page opened in the browser and not answered yet.
struct Pending {
    url: String,
    state: String,
    key: SignInKey,
}

/// Firebase Auth over its REST API. Signing in happens in the browser, on the
/// site's /signin/ page (Google, Apple or email), which hands the session back
/// via lovearound://signin, sealed to a one-time key of ours (signin_box.rs).
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
            inner: Mutex::new(Inner { session, id_token: None, expires_at: Instant::now(), pending: None }),
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
        // Signed out, or into another account, while the call was out: this
        // token belongs to a session that is no longer here. Not SignedOut,
        // which would sign the new account out; the caller just tries again.
        let Some(session) = inner.session.as_mut().filter(|s| s.refresh_token == refresh_token) else {
            return Err(Error::Network);
        };
        let changed = session.refresh_token != new_refresh;
        if changed {
            session.refresh_token = new_refresh.to_string();
        }
        inner.id_token = Some(id.to_string());
        inner.expires_at = Instant::now() + Duration::from_secs(expires_in);
        if changed {
            self.save(&inner);
        }
        Ok(id.to_string())
    }

    // ---- sign-in, through the site -------------------------------------------

    /// The page that signs in (Google, Apple or email) and returns to
    /// lovearound://signin, with the session sealed to our one-time key.
    /// Opening it again gives the same page, so a tab already open still works.
    pub fn sign_in_url(&self) -> String {
        let mut inner = self.inner.lock().unwrap();
        if let Some(pending) = &inner.pending {
            return pending.url.clone();
        }
        let state = uuid::Uuid::new_v4().to_string().to_uppercase();
        let key = SignInKey::new();
        let url = format!("{}/signin?state={state}&key={}", config::SITE, key.public());
        inner.pending = Some(Pending { url: url.clone(), state, key });
        url
    }

    /// lovearound://signin?state=…&epk=…&box=…, the box holding uid, email and
    /// the refresh token. A link without the box is refused: the site has
    /// sealed it since 2026-11, and a session in the clear could have been
    /// read by anything that saw the link.
    pub async fn complete_sign_in(&self, url: &url::Url) -> Result<()> {
        let q: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
        let sealed = {
            let mut inner = self.inner.lock().unwrap();
            // The state proves the callback answers our own request, so a crafted
            // link cannot sign the app into someone else's account.
            let Some(pending) = inner.pending.as_ref().filter(|p| q.get("state") == Some(&p.state)) else {
                return Err(Error::Firebase("BAD_CALLBACK".into()));
            };
            let sealed = q.get("box").and_then(|sealed| pending.key.open(q.get("epk").map_or("", |s| s.as_str()), sealed, &pending.state));
            let Some(sealed) = sealed.filter(|s| !s.uid.is_empty() && !s.rt.is_empty()) else {
                return Err(Error::Firebase("BAD_CALLBACK".into()));
            };
            inner.pending = None;
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

    /// How this session signed in: "google.com", "apple.com" or "password",
    /// read from the ID token (a refreshed token keeps it). None before the
    /// first token.
    pub fn provider(&self) -> Option<String> {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let token = self.inner.lock().unwrap().id_token.clone()?;
        let payload = URL_SAFE_NO_PAD.decode(token.split('.').nth(1)?.trim_end_matches('=')).ok()?;
        let claims: Value = serde_json::from_slice(&payload).ok()?;
        claims["firebase"]["sign_in_provider"].as_str().map(str::to_string)
    }

    // ---- session ------------------------------------------------------------

    pub fn sign_out(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.session = None;
        inner.id_token = None;
        inner.pending = None;
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
