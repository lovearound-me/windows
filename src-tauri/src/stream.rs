use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::{interval_at, sleep, sleep_until, Instant, MissedTickBehavior};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::auth::Auth;
use crate::config;

pub enum Cmd {
    Start,
    Stop,
    /// After sleep or a network change the old socket is likely dead.
    Restart,
    /// Take signals to anyone too; reconnects with the new choice.
    Anyone(bool),
}

pub enum Event {
    /// True once the server said "ready" on the current socket, false when it is gone.
    Online(bool),
    /// The address book may have changed: on every (re)connect, since events
    /// could be missed while offline, and on a "sync" hint from the server.
    Sync,
    Signal,
    AuthLost,
}

/// How a socket's life ended.
enum End {
    Stopped,
    Restart,
    AuthLost,
    Failed,
}

/// WS /v1/stream: incoming events, reconnecting with backoff. Runs as one
/// task; commands go in, events come out.
pub fn spawn(auth: Arc<Auth>, client: String, anyone: bool, events: UnboundedSender<Event>) -> UnboundedSender<Cmd> {
    let (tx, rx) = unbounded_channel();
    tauri::async_runtime::spawn(run(auth, client, anyone, events, rx));
    tx
}

async fn run(auth: Arc<Auth>, client: String, mut anyone: bool, events: UnboundedSender<Event>, mut cmds: UnboundedReceiver<Cmd>) {
    let mut running = false;
    let mut backoff = 1.0f64;
    // The server turned the last handshake down: the cached token is stale.
    let mut refresh = false;
    loop {
        if !running {
            match cmds.recv().await {
                Some(Cmd::Start) => {
                    running = true;
                    backoff = 1.0;
                }
                Some(Cmd::Anyone(a)) => anyone = a,
                Some(_) => {}
                None => return,
            }
            continue;
        }
        let end = socket(&auth, &client, &mut anyone, &mut refresh, &mut backoff, &events, &mut cmds).await;
        let _ = events.send(Event::Online(false));
        match end {
            End::Stopped => running = false,
            End::Restart => backoff = 1.0,
            End::AuthLost => {
                let _ = events.send(Event::AuthLost);
                running = false;
            }
            End::Failed => {
                let delay = backoff;
                backoff = (backoff * 2.0).min(60.0);
                tokio::select! {
                    _ = sleep(Duration::from_secs_f64(delay)) => {}
                    cmd = cmds.recv() => match cmd {
                        Some(Cmd::Stop) | None => running = false,
                        Some(Cmd::Anyone(a)) => { anyone = a; backoff = 1.0 }
                        Some(_) => backoff = 1.0,
                    }
                }
            }
        }
    }
}

async fn socket(
    auth: &Auth,
    client: &str,
    anyone: &mut bool,
    refresh: &mut bool,
    backoff: &mut f64,
    events: &UnboundedSender<Event>,
    cmds: &mut UnboundedReceiver<Cmd>,
) -> End {
    let token = match auth.token(*refresh).await {
        Ok(token) => {
            *refresh = false;
            token
        }
        Err(e) if e.ends_session() => return End::AuthLost,
        Err(_) => return End::Failed,
    };
    let mut url = format!("{}/v1/stream", config::API_BASE.replacen("https://", "wss://", 1));
    if !*anyone {
        url.push_str("?anyone=false");
    }
    let Ok(mut req) = url.into_client_request() else { return End::Failed };
    let headers = req.headers_mut();
    if let Ok(v) = format!("Bearer {token}").parse() {
        headers.insert("Authorization", v);
    }
    if let Ok(v) = client.parse() {
        headers.insert("X-Client", v);
    }

    let connect = tokio::time::timeout(Duration::from_secs(20), tokio_tungstenite::connect_async(req));
    let mut ws = tokio::select! {
        result = connect => match result {
            Ok(Ok((ws, _))) => ws,
            Ok(Err(tungstenite::Error::Http(resp))) => {
                if resp.status().as_u16() == 401 {
                    *refresh = true;
                }
                return End::Failed;
            }
            _ => return End::Failed,
        },
        cmd = cmds.recv() => return command(cmd, anyone),
    };

    // A socket can die without an error (Wi-Fi gone, a router dropped it):
    // TCP would notice only minutes later, so no pong in 10 s means reconnect.
    let mut ping = interval_at(Instant::now() + Duration::from_secs(30), Duration::from_secs(30));
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut pong_due: Option<Instant> = None;
    loop {
        let deadline = pong_due.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    if handle(&text, events) {
                        *backoff = 1.0;
                    }
                }
                Some(Ok(Message::Pong(_))) => pong_due = None,
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return End::Failed,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if ws.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return End::Failed;
                }
                pong_due.get_or_insert(Instant::now() + Duration::from_secs(10));
            }
            _ = sleep_until(deadline), if pong_due.is_some() => return End::Failed,
            cmd = cmds.recv() => {
                let end = command(cmd, anyone);
                let _ = ws.close(None).await;
                return end;
            }
        }
    }
}

fn command(cmd: Option<Cmd>, anyone: &mut bool) -> End {
    match cmd {
        Some(Cmd::Stop) | None => End::Stopped,
        Some(Cmd::Anyone(a)) => {
            *anyone = a;
            End::Restart
        }
        Some(_) => End::Restart,
    }
}

/// True on "ready".
fn handle(text: &str, events: &UnboundedSender<Event>) -> bool {
    let Ok(msg) = serde_json::from_str::<Value>(text) else { return false };
    match msg["type"].as_str() {
        Some("ready") => {
            let _ = events.send(Event::Online(true));
            let _ = events.send(Event::Sync);
            true
        }
        Some("sync") => {
            let _ = events.send(Event::Sync);
            false
        }
        Some("event") => {
            // Unknown types and versions are ignored, so old apps survive new events.
            let event = &msg["event"];
            if event["type"] == "signal" && event["v"] == 1 {
                let _ = events.send(Event::Signal);
            }
            false
        }
        _ => false,
    }
}
