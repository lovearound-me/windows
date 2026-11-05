use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::config;
use crate::error::{Error, Result};

/// A contact from the server. Its color is not here: colors live on this PC.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Contact {
    /// The server's id for this entry; the person's own id never leaves it.
    pub id: String,
    pub label: String,
    pub position: i64,
    /// Optional: a server older than the field leaves it out.
    #[serde(default)]
    pub close: Option<bool>,
}

impl Contact {
    /// A close person: a circle on top of the menu, first on a board.
    pub fn is_close(&self) -> bool {
        self.close.unwrap_or(false)
    }
}

/// One of the user's own devices (a toy, a lamp, a board) with its own token.
#[derive(Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<String>,
    pub last_seen_at: Option<String>,
}

#[derive(Deserialize)]
pub struct Me {
    pub uid: String,
    pub invite_url: String,
}

/// needed-server REST API; see docs/schema.md.
pub struct Api {
    auth: Arc<Auth>,
    http: reqwest::Client,
    /// Sent as `X-Client` with every request, so the server logs which version sent what.
    pub client: String,
}

impl Api {
    pub fn new(auth: Arc<Auth>, http: reqwest::Client, client: String) -> Self {
        Api { auth, http, client }
    }

    pub async fn me(&self) -> Result<Me> {
        decode(self.request("GET", "v1/me", None).await?)
    }

    pub async fn contacts(&self) -> Result<Vec<Contact>> {
        #[derive(Deserialize)]
        struct List {
            contacts: Vec<Contact>,
        }
        Ok(decode::<List>(self.request("GET", "v1/contacts", None).await?)?.contacts)
    }

    /// Always answers the same whether or not the code belongs to anyone.
    pub async fn add_contact(&self, code: &str, label: &str, close: bool) -> Result<Contact> {
        decode(self.request("POST", "v1/contacts", Some(json!({ "code": code, "label": label, "close": close }))).await?)
    }

    pub async fn update_contact(&self, id: &str, label: Option<&str>, position: Option<i64>, close: Option<bool>) -> Result<()> {
        let mut body = serde_json::Map::new();
        if let Some(label) = label {
            body.insert("label".into(), json!(label));
        }
        if let Some(position) = position {
            body.insert("position".into(), json!(position));
        }
        if let Some(close) = close {
            body.insert("close".into(), json!(close));
        }
        self.request("PATCH", &format!("v1/contacts/{id}"), Some(Value::Object(body))).await?;
        Ok(())
    }

    pub async fn delete_contact(&self, id: &str) -> Result<()> {
        self.request("DELETE", &format!("v1/contacts/{id}"), None).await?;
        Ok(())
    }

    pub async fn signal(&self, contact_id: &str) -> Result<()> {
        self.request("POST", "v1/events", Some(json!({ "type": "signal", "to": { "contact": contact_id } }))).await?;
        Ok(())
    }

    /// 202 even with nobody online to take it; 429 if one already went this hour.
    pub async fn signal_anyone(&self) -> Result<()> {
        self.request("POST", "v1/events", Some(json!({ "type": "signal", "to": { "anyone": true } }))).await?;
        Ok(())
    }

    pub async fn devices(&self) -> Result<Vec<Device>> {
        #[derive(Deserialize)]
        struct List {
            devices: Vec<Device>,
        }
        Ok(decode::<List>(self.request("GET", "v1/devices", None).await?)?.devices)
    }

    /// The token comes back only here, once.
    pub async fn add_device(&self, name: &str, capabilities: &[String]) -> Result<(Device, String)> {
        #[derive(Deserialize)]
        struct Created {
            device: Device,
            token: String,
        }
        let body = json!({ "name": name, "kind": "custom", "capabilities": capabilities });
        let created: Created = decode(self.request("POST", "v1/devices", Some(body)).await?)?;
        Ok((created.device, created.token))
    }

    pub async fn delete_device(&self, id: &str) -> Result<()> {
        self.request("DELETE", &format!("v1/devices/{id}"), None).await?;
        Ok(())
    }

    pub async fn delete_me(&self) -> Result<()> {
        self.request("DELETE", "v1/me", None).await?;
        Ok(())
    }

    /// Retries once with a fresh token on 401.
    async fn request(&self, method: &str, path: &str, body: Option<Value>) -> Result<Vec<u8>> {
        let method = reqwest::Method::from_bytes(method.as_bytes()).expect("method");
        for attempt in 0..2 {
            let token = self.auth.token(attempt > 0).await?;
            let mut req = self
                .http
                .request(method.clone(), format!("{}/{path}", config::API_BASE))
                .bearer_auth(token)
                .header("X-Client", &self.client);
            if let Some(body) = &body {
                req = req.json(body);
            }
            let resp = req.send().await?;
            let status = resp.status().as_u16();
            if status == 401 && attempt == 0 {
                continue;
            }
            let data = resp.bytes().await?.to_vec();
            if !(200..300).contains(&status) {
                let json: Value = serde_json::from_slice(&data).unwrap_or(Value::Null);
                return Err(Error::Status(status, json["error"].as_str().unwrap_or("").to_string()));
            }
            return Ok(data);
        }
        Err(Error::Status(401, "unauthorized".into()))
    }
}

fn decode<T: serde::de::DeserializeOwned>(data: Vec<u8>) -> Result<T> {
    serde_json::from_slice(&data).map_err(|_| Error::Status(0, "bad response".into()))
}
