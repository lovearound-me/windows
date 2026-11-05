use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Local, TimeZone, Timelike};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::api::{Api, Contact};
use crate::auth::Auth;
use crate::error::{Error, Result};
use crate::settings::{self, Settings, FOREVER};
use crate::stream::{self, Cmd, Event};
use crate::{config, overlay, presenting, tray, ui};

/// The popover's screens. The book and the settings are windows of their own.
#[derive(Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(tag = "name", rename_all = "camelCase")]
pub enum Route {
    Main,
    Add { invite: Option<String> },
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PauseLength {
    Hour,
    UntilMorning,
    UntilResumed,
}

/// The twelve palette colors, by index, as in the UI's palette.js: the glow
/// takes them. Indices are what settings store.
const PALETTE: [u32; 12] = [
    0xF07A62, 0xE86A9A, 0x4E9BD8, 0x6FAE7C, 0x9B83DD, 0xE5A93B, 0x3FB3A8, 0xC94A5E, 0xC98B5E, 0x9DA24A, 0x6474D6, 0x7E8C99,
];

fn palette_at(index: i64) -> u32 {
    PALETTE[index.rem_euclid(PALETTE.len() as i64) as usize]
}

/// The sender of a signal to anyone sees stars in this color.
const STAR_COLOR: u32 = 0xF2C46B;

pub struct State {
    pub signed_in: bool,
    pub email: Option<String>,
    pub invite_url: Option<String>,
    pub contacts: Vec<Contact>,
    pub route: Route,
    /// The contact that was just sent a signal, for "Улетело ♥".
    pub sent: Option<String>,
    sent_gen: u64,
    pub problem: Option<String>,
    pub waiting_for_browser: bool,
    /// The stream is connected. A reconnect of a few seconds (after sleep, a
    /// new network, a settings change) does not count.
    pub online: bool,
    /// No connection for a while (or none since launch): the menu says signals
    /// won't arrive. Not just `!online`, which would flash on every launch.
    pub offline: bool,
    offline_gen: u64,
    /// The device just made in Settings → Устройства: its token is shown once,
    /// so it outlives the window closing.
    pub created_device: Option<(String, String)>,
    pub settings: Settings,
    pub anyone_sending: bool,
    last_shown: Option<Instant>,
    pending_add: Option<String>,
    reorder_gen: u64,
    pub launch_at_login: bool,
    /// The contact the book shows on the right.
    pub book_selection: Option<String>,
}

impl State {
    pub fn is_paused(&self) -> bool {
        self.settings.paused_until.is_some_and(|until| until > now_ms())
    }

    pub fn pause_text(&self) -> String {
        let Some(until) = self.settings.paused_until.filter(|_| self.is_paused()) else { return String::new() };
        if until == FOREVER {
            return tr!("Пауза, пока не снимешь");
        }
        let Some(at) = Local.timestamp_millis_opt(until).single() else { return String::new() };
        let time = at.format("%H:%M");
        if at.date_naive() == Local::now().date_naive() {
            tr!("Пауза до {time}", time = time)
        } else {
            tr!("Пауза до завтра, {time}", time = time)
        }
    }

    fn close_contacts(&self) -> Vec<Contact> {
        self.contacts.iter().filter(|c| c.is_close()).cloned().collect()
    }

    fn color_for(&self, contact: &Contact) -> u32 {
        match self.settings.contact_colors.get(&contact.id) {
            Some(index) => palette_at(*index),
            None => palette_at(self.contacts.iter().position(|c| c.id == contact.id).unwrap_or(0) as i64),
        }
    }

    /// Whole minutes until the next signal to anyone, or None if it can go now.
    fn anyone_wait(&self) -> Option<i64> {
        let left = self.settings.anyone_sent_at? + 3_600_000 - now_ms();
        (left > 0).then(|| (left + 59_999) / 60_000)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot<'a> {
    signed_in: bool,
    email: &'a Option<String>,
    invite_url: &'a Option<String>,
    contacts: Vec<SnapshotContact<'a>>,
    route: &'a Route,
    sent: &'a Option<String>,
    problem: &'a Option<String>,
    waiting_for_browser: bool,
    offline: bool,
    created_device: Option<serde_json::Value>,
    settings: &'a Settings,
    anyone_sending: bool,
    paused: bool,
    pause_text: String,
    launch_at_login: bool,
    book_selection: &'a Option<String>,
    version: String,
    lang: &'static str,
}

#[derive(Serialize)]
struct SnapshotContact<'a> {
    id: &'a str,
    label: &'a str,
    position: i64,
    close: bool,
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// App state behind the menu. Contact colors, the own signal color and all
/// settings stay on this PC: the server only knows contacts and labels.
pub struct Model {
    app: AppHandle,
    auth: Arc<Auth>,
    pub api: Api,
    stream: UnboundedSender<Cmd>,
    state: Mutex<State>,
    settings_file: PathBuf,
    /// Saves go to the server one after another, in the order made, and a
    /// reload waits for them: halfway through it would show a new order half-applied.
    saving: tokio::sync::Mutex<()>,
}

impl Model {
    pub fn new(app: AppHandle) -> Arc<Self> {
        let dir = app.path().app_data_dir().expect("app data dir");
        let _ = std::fs::create_dir_all(&dir);
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .expect("http client");
        let auth = Arc::new(Auth::new(dir.clone(), http.clone()));
        let client = format!("windows/{}", app.package_info().version);
        let settings_file = dir.join("settings.json");
        let settings = Settings::load(&settings_file);
        crate::i18n::set(&settings.language);
        let (events_tx, mut events) = unbounded_channel();
        let stream = stream::spawn(auth.clone(), client.clone(), settings.accept_anyone, events_tx);

        let model = Arc::new(Model {
            api: Api::new(auth.clone(), http, client),
            app: app.clone(),
            auth,
            stream,
            settings_file,
            saving: tokio::sync::Mutex::new(()),
            state: Mutex::new(State {
                signed_in: false,
                email: None,
                invite_url: None,
                contacts: Vec::new(),
                route: Route::Main,
                sent: None,
                sent_gen: 0,
                problem: None,
                waiting_for_browser: false,
                online: false,
                offline: false,
                offline_gen: 0,
                created_device: None,
                settings,
                anyone_sending: false,
                last_shown: None,
                pending_add: None,
                reorder_gen: 0,
                launch_at_login: false,
                book_selection: None,
            }),
        });

        let weak = Arc::downgrade(&model);
        tauri::async_runtime::spawn(async move {
            while let Some(event) = events.recv().await {
                let Some(model) = weak.upgrade() else { return };
                match event {
                    Event::Online(up) => model.connection_changed(up),
                    Event::Signal => model.receive(),
                    Event::AuthLost => model.sign_out(),
                    Event::Sync => {
                        let model = model.clone();
                        tauri::async_runtime::spawn(async move {
                            drop(model.saving.lock().await);
                            model.reload().await;
                        });
                    }
                }
            }
        });

        // A pause ends by itself.
        let weak = Arc::downgrade(&model);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let Some(model) = weak.upgrade() else { return };
                let ended = model.read(|s| s.settings.paused_until.is_some() && !s.is_paused());
                if ended {
                    model.update(|s| s.settings.paused_until = None);
                }
            }
        });

        // After sleep the old socket is likely dead: a timer that wakes up far
        // later than it asked to means the PC was asleep.
        let weak = Arc::downgrade(&model);
        tauri::async_runtime::spawn(async move {
            let mut last = chrono::Utc::now();
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let Some(model) = weak.upgrade() else { return };
                let now = chrono::Utc::now();
                if (now - last).num_seconds() > 30 {
                    model.restart_stream();
                }
                last = now;
            }
        });

        let first_launch = !model.read(|s| s.settings.login_item_set_up);
        if first_launch {
            let _ = app.autolaunch().enable();
            model.update(|s| s.settings.login_item_set_up = true);
        }
        let on = app.autolaunch().is_enabled().unwrap_or(false);
        model.update(|s| s.launch_at_login = on);
        model
    }

    // ---- state --------------------------------------------------------------

    pub fn read<R>(&self, f: impl FnOnce(&State) -> R) -> R {
        f(&self.state.lock().unwrap())
    }

    /// Changes the state, saves the settings and shows the change everywhere.
    pub fn update<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        let (result, snapshot) = {
            let mut state = self.state.lock().unwrap();
            let result = f(&mut state);
            state.settings.save(&self.settings_file);
            (result, (self.snapshot_of(&state), tray::Look::of(&state)))
        };
        // Outside the lock: the tray waits for the main thread, which may be waiting for the lock.
        tray::refresh(&self.app, snapshot.1);
        let _ = self.app.emit("state", snapshot.0);
        result
    }

    pub fn snapshot(&self) -> serde_json::Value {
        self.snapshot_of(&self.state.lock().unwrap())
    }

    fn snapshot_of(&self, s: &State) -> serde_json::Value {
        let snapshot = Snapshot {
            signed_in: s.signed_in,
            email: &s.email,
            invite_url: &s.invite_url,
            contacts: s
                .contacts
                .iter()
                .map(|c| SnapshotContact { id: &c.id, label: &c.label, position: c.position, close: c.is_close() })
                .collect(),
            route: &s.route,
            sent: &s.sent,
            problem: &s.problem,
            waiting_for_browser: s.waiting_for_browser,
            offline: s.offline,
            created_device: s.created_device.as_ref().map(|(name, token)| serde_json::json!({ "name": name, "token": token })),
            settings: &s.settings,
            anyone_sending: s.anyone_sending,
            paused: s.is_paused(),
            pause_text: s.pause_text(),
            launch_at_login: s.launch_at_login,
            book_selection: &s.book_selection,
            version: self.version(),
            lang: crate::i18n::lang(),
        };
        serde_json::to_value(snapshot).unwrap_or_default()
    }

    pub fn version(&self) -> String {
        self.app.package_info().version.to_string()
    }

    // ---- session ------------------------------------------------------------

    pub fn start(self: &Arc<Self>) {
        let Some(session) = self.auth.session() else { return };
        self.update(|s| {
            s.signed_in = true;
            s.email = session.email.clone();
        });
        // The stream reloads the contacts once it is connected.
        let _ = self.stream.send(Cmd::Start);
        let online = self.read(|s| s.online);
        self.connection_changed(online);
    }

    pub fn restart_stream(&self) {
        let _ = self.stream.send(Cmd::Restart);
    }

    pub async fn reload(&self) {
        let result: Result<_> = async { Ok((self.api.me().await?, self.api.contacts().await?)) }.await;
        match result {
            Ok((me, contacts)) => self.update(|s| {
                let _ = me.uid;
                s.invite_url = Some(me.invite_url);
                s.contacts = contacts;
                s.problem = None;
            }),
            Err(e) if e.ends_session() => self.sign_out(),
            Err(_) => self.update(|s| s.problem = Some(tr!("Нет связи с сервером. Попробуем ещё раз чуть позже."))),
        }
    }

    fn connection_changed(self: &Arc<Self>, up: bool) {
        let gen = self.update(|s| {
            s.offline_gen += 1;
            if up {
                s.online = true;
                s.offline = false;
            }
            s.offline_gen
        });
        if !up {
            let model = self.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                model.update(|s| {
                    if s.offline_gen == gen {
                        s.online = false;
                        s.offline = s.signed_in;
                    }
                });
            });
        }
    }

    // ---- signals ------------------------------------------------------------

    pub fn send(self: &Arc<Self>, contact_id: &str) {
        let Some((color, gen, count, calm)) = self.update(|s| {
            let contact = s.contacts.iter().find(|c| c.id == contact_id)?.clone();
            s.sent = Some(contact.id.clone());
            s.sent_gen += 1;
            s.settings.last_sent.insert(contact.id.clone(), now_ms());
            Some((s.color_for(&contact), s.sent_gen, s.settings.emoji_count, s.settings.calm))
        }) else {
            return;
        };
        let model = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            model.update(|s| {
                if s.sent_gen == gen {
                    s.sent = None;
                }
            });
        });
        // The sender sees the glow too, in the contact's color: silent, and even on pause.
        overlay::play(&self.app, color, overlay::Emoji::Warm, count, calm, false);
        let model = self.clone();
        let id = contact_id.to_string();
        tauri::async_runtime::spawn(async move {
            match model.api.signal(&id).await {
                Ok(()) => model.update(|s| s.problem = None),
                Err(e) if e.ends_session() => {
                    model.sign_out();
                    model.update(|s| s.problem = Some(tr!("Нужно войти заново")));
                }
                Err(_) => model.update(|s| {
                    s.sent = None;
                    s.problem = Some(tr!("Не улетело, проверь интернет"));
                }),
            }
        });
    }

    /// "Кому-нибудь на свете": one random person online who takes them. The
    /// server spends the hour on the try and accepts it even with nobody
    /// online, so "Улетело" is all the sender ever learns.
    pub fn send_to_anyone(self: &Arc<Self>) {
        let go = self.update(|s| {
            if s.anyone_wait().is_some() || s.anyone_sending {
                return false;
            }
            s.anyone_sending = true;
            true
        });
        if !go {
            return;
        }
        let model = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = model.api.signal_anyone().await;
            let calm = model.read(|s| s.settings.calm);
            model.update(|s| s.anyone_sending = false);
            match result {
                Ok(()) => {
                    model.update(|s| {
                        s.settings.anyone_sent_at = Some(now_ms());
                        s.problem = None;
                    });
                    overlay::play(&model.app, STAR_COLOR, overlay::Emoji::Stars, settings::DEFAULT_EMOJI_COUNT, calm, false);
                }
                Err(e) if e.ends_session() => {
                    model.sign_out();
                    model.update(|s| s.problem = Some(tr!("Нужно войти заново")));
                }
                // Sent from another device this hour, at a time not known
                // here: wait a whole hour from now, a little longer than needed.
                Err(e) if e.status() == 429 => model.update(|s| {
                    s.settings.anyone_sent_at = Some(now_ms() - 120_000);
                    s.problem = None;
                }),
                Err(_) => model.update(|s| s.problem = Some(tr!("Не улетело, проверь интернет"))),
            }
        });
    }

    fn receive(&self) {
        let show = self.update(|s| {
            if s.is_paused() {
                return false;
            }
            // Not shown, and not saved for later: a signal is about the moment.
            if s.settings.quiet_when_presenting && presenting::now() {
                return false;
            }
            // The server lets one signal per sender through each second; a rarer
            // glow is this PC's own choice, and the ones in between are not shown.
            if s.last_shown.is_some_and(|at| at.elapsed().as_secs_f64() < s.settings.min_gap) {
                return false;
            }
            s.last_shown = Some(Instant::now());
            true
        });
        if show {
            self.play_effect();
        }
    }

    /// The incoming-signal effect, also used by "Показать, как это выглядит".
    pub fn play_effect(&self) {
        let (color, count, calm, sound) =
            self.read(|s| (palette_at(s.settings.my_color), s.settings.emoji_count, s.settings.calm, s.settings.sound));
        overlay::play(&self.app, color, overlay::Emoji::Warm, count, calm, sound);
        tray::flash(&self.app, color);
    }

    // ---- pause --------------------------------------------------------------

    pub fn pause(&self, length: PauseLength) {
        let until = match length {
            PauseLength::Hour => now_ms() + 3_600_000,
            PauseLength::UntilMorning => {
                let now = Local::now();
                let today8 = now.with_hour(8).and_then(|t| t.with_minute(0)).and_then(|t| t.with_second(0)).unwrap_or(now);
                let at = if today8 > now { today8 } else { today8 + chrono::Duration::days(1) };
                at.timestamp_millis()
            }
            PauseLength::UntilResumed => FOREVER,
        };
        self.update(|s| s.settings.paused_until = Some(until));
    }

    pub fn resume(&self) {
        self.update(|s| s.settings.paused_until = None);
    }

    // ---- contacts -----------------------------------------------------------

    pub async fn add_contact(&self, code: &str, label: &str, color: i64, close: bool) -> Result<String> {
        let contact = self.api.add_contact(code, label, close).await?;
        let id = contact.id.clone();
        self.update(|s| {
            s.settings.contact_colors.insert(contact.id.clone(), color);
            s.contacts.push(contact);
        });
        Ok(id)
    }

    pub async fn update_contact(&self, id: &str, label: &str, color: i64) -> Result<()> {
        let current = self.read(|s| s.contacts.iter().find(|c| c.id == id).cloned());
        let Some(current) = current else { return Err(Error::Status(404, String::new())) };
        if label != current.label {
            let _guard = self.saving.lock().await;
            self.api.update_contact(id, Some(label), None, None).await?;
        }
        self.update(|s| {
            s.settings.contact_colors.insert(id.to_string(), color);
            if let Some(c) = s.contacts.iter_mut().find(|c| c.id == id) {
                c.label = label.to_string();
            }
        });
        Ok(())
    }

    /// Into "Рядом" or out of it: shown at once, saved behind.
    pub fn set_close(self: &Arc<Self>, id: &str, close: bool) {
        self.update(|s| {
            if let Some(c) = s.contacts.iter_mut().find(|c| c.id == id) {
                c.close = Some(close);
            }
        });
        let model = self.clone();
        let id = id.to_string();
        tauri::async_runtime::spawn(async move {
            let guard = model.saving.lock().await;
            if model.api.update_contact(&id, None, None, Some(close)).await.is_err() {
                drop(guard);
                model.update(|s| s.problem = Some(tr!("Не получилось сохранить")));
                model.reload().await;
            }
        });
    }

    pub async fn remove(&self, id: &str) -> Result<()> {
        self.api.delete_contact(id).await?;
        self.update(|s| {
            s.contacts.retain(|c| c.id != id);
            s.settings.contact_colors.remove(id);
            s.settings.last_sent.remove(id);
            if s.book_selection.as_deref() == Some(id) {
                s.book_selection = None;
            }
        });
        Ok(())
    }

    /// Puts a close contact in the target's circle, the others shift.
    pub fn move_circle(self: &Arc<Self>, id: &str, target: &str) {
        let mut close = self.read(|s| s.close_contacts());
        let (Some(from), Some(to)) = (close.iter().position(|c| c.id == id), close.iter().position(|c| c.id == target)) else {
            return;
        };
        if from == to {
            return;
        }
        let person = close.remove(from);
        close.insert(to, person);
        self.reorder_close(close);
    }

    /// A drag in the book: into "Рядом" at this place among the circles,
    /// or out of it with None.
    pub fn place(self: &Arc<Self>, id: &str, close_at: Option<usize>) {
        let Some(contact) = self.read(|s| s.contacts.iter().find(|c| c.id == id).cloned()) else { return };
        let Some(index) = close_at else {
            if contact.is_close() {
                self.set_close(id, false);
            }
            return;
        };
        if !contact.is_close() {
            self.set_close(id, true);
        }
        let (mut close, person) = self.read(|s| {
            (s.close_contacts().into_iter().filter(|c| c.id != id).collect::<Vec<_>>(), s.contacts.iter().find(|c| c.id == id).cloned())
        });
        let Some(person) = person else { return };
        let at = index.min(close.len());
        close.insert(at, person);
        self.reorder_close(close);
    }

    /// Shown at once; saved a moment after the last change, so a drag across
    /// several circles is one save.
    fn reorder_close(self: &Arc<Self>, close: Vec<Contact>) {
        let gen = self.update(|s| {
            // The close ones keep their slots in the whole list, in the new order.
            let slots: Vec<usize> = (0..s.contacts.len()).filter(|&i| s.contacts[i].is_close()).collect();
            for (slot, person) in slots.into_iter().zip(close) {
                s.contacts[slot] = person;
            }
            s.reorder_gen += 1;
            s.reorder_gen
        });
        let model = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(700)).await;
            if model.read(|s| s.reorder_gen) == gen {
                model.save_positions().await;
            }
        });
    }

    async fn save_positions(self: &Arc<Self>) {
        let guard = self.saving.lock().await;
        let updates = self.update(|s| {
            let mut updates = Vec::new();
            for (index, contact) in s.contacts.iter_mut().enumerate() {
                if contact.position != index as i64 {
                    contact.position = index as i64;
                    updates.push((contact.id.clone(), index as i64));
                }
            }
            updates
        });
        for (id, position) in updates {
            if self.api.update_contact(&id, None, Some(position), None).await.is_err() {
                drop(guard);
                self.update(|s| s.problem = Some(tr!("Не получилось сохранить порядок")));
                self.reload().await;
                return;
            }
        }
    }

    /// The code of an invite link on the clipboard, unless it is this user's own.
    pub fn pasted_invite(&self, clipboard: Option<String>) -> Option<String> {
        let code = invite_id(&clipboard?)?;
        let bare = |code: &str| code.to_uppercase().replace('-', "");
        let own = self.read(|s| s.invite_url.clone()).and_then(|url| invite_id(&url));
        if own.is_some_and(|own| bare(&own) == bare(&code)) {
            return None;
        }
        Some(code)
    }

    // ---- links --------------------------------------------------------------

    pub fn handle_url(self: &Arc<Self>, link: &str) {
        let Ok(url) = url::Url::parse(link) else { return };
        if !config::URL_SCHEMES.contains(&url.scheme()) {
            return;
        }
        match url.host_str() {
            Some("add") => {
                // The whole link, not the path: its leading "/" would leave an empty id.
                let Some(invite) = invite_id(link) else { return };
                self.update(|s| {
                    if s.signed_in {
                        s.route = Route::Add { invite: Some(invite) };
                    } else {
                        s.pending_add = Some(invite);
                    }
                });
                ui::show_popover(&self.app, None);
            }
            Some("signin") => {
                // The browser can hand over the same link twice (the page's own
                // redirect, then its "back" button, or a restored tab): the first
                // one signs in and uses up the state, so a late copy is no news.
                if self.read(|s| s.signed_in && !s.waiting_for_browser) {
                    return;
                }
                let model = self.clone();
                tauri::async_runtime::spawn(async move {
                    match model.auth.complete_google(&url).await {
                        Ok(()) => model.did_sign_in(),
                        // Unless it's a copy that lost the race to the first one.
                        Err(e) => model.update(|s| {
                            if !s.signed_in {
                                s.problem = Some(e.message())
                            }
                        }),
                    }
                    model.update(|s| s.waiting_for_browser = false);
                    ui::show_popover(&model.app, None);
                });
            }
            _ => {}
        }
    }

    // ---- account ------------------------------------------------------------

    pub async fn sign_in(self: &Arc<Self>, email: &str, password: &str) -> Result<()> {
        self.auth.sign_in(email, password).await?;
        self.did_sign_in();
        Ok(())
    }

    pub fn sign_in_with_google(&self) {
        self.update(|s| s.waiting_for_browser = true);
        let url = self.auth.google_sign_in_url();
        let _ = self.app.opener().open_url(url, None::<&str>);
    }

    pub async fn send_password_reset(&self, email: &str) -> Result<()> {
        self.auth.send_password_reset(email).await
    }

    fn did_sign_in(self: &Arc<Self>) {
        self.update(|s| s.problem = None);
        self.start();
        self.update(|s| {
            s.route = match s.pending_add.take() {
                Some(invite) => Route::Add { invite: Some(invite) },
                None => Route::Main,
            };
        });
    }

    pub fn sign_out(&self) {
        let _ = self.stream.send(Cmd::Stop);
        self.auth.sign_out();
        self.update(|s| {
            s.offline_gen += 1;
            s.offline = false;
            s.online = false;
            s.created_device = None;
            s.signed_in = false;
            s.email = None;
            s.invite_url = None;
            s.contacts.clear();
            s.route = Route::Main;
            s.book_selection = None;
        });
        ui::close_windows(&self.app);
    }

    pub async fn delete_account(&self) -> Result<()> {
        self.api.delete_me().await?;
        self.update(|s| {
            s.settings.contact_colors.clear();
            s.settings.last_sent.clear();
        });
        self.sign_out();
        Ok(())
    }

    pub fn set_launch_at_login(&self, on: bool) {
        let autolaunch = self.app.autolaunch();
        let result = if on { autolaunch.enable() } else { autolaunch.disable() };
        let now = autolaunch.is_enabled().unwrap_or(false);
        self.update(|s| {
            if result.is_err() {
                s.problem = Some(tr!("Windows не дал поменять автозапуск"));
            }
            s.launch_at_login = now;
        });
    }

    pub fn set_accept_anyone(&self, on: bool) {
        self.update(|s| s.settings.accept_anyone = on);
        let _ = self.stream.send(Cmd::Anyone(on));
    }

    /// Everything in the language just chosen: the tray at once, the pages
    /// reload when they see `lang` change.
    pub fn set_language(&self, choice: &str) {
        crate::i18n::set(choice);
        self.update(|s| s.settings.language = choice.to_string());
        crate::ui::retitle(&self.app);
    }

    pub fn set_route(&self, route: Route) {
        self.update(|s| s.route = route);
    }
}

/// The code from an invite link (or the bare code), as typed.
pub fn invite_id(text: &str) -> Option<String> {
    let mut text = text.trim().to_string();
    for marker in ["/i/", "lovearound://add/", "needed://add/"] {
        if let Some(at) = text.rfind(marker) {
            text = text[at + marker.len()..].to_string();
        }
    }
    let text = text.split(['/', '?', '#']).next().unwrap_or("").to_string();
    // 10 characters of the server's alphabet, dashes anywhere (`GSSV7-A33HN`).
    // The server never says whether a code is taken, so this is the only
    // check a typo gets: no 0, 1, I, L, O or U.
    const ALPHABET: &str = "23456789ABCDEFGHJKMNPQRSTVWXYZ";
    let code: String = text.to_uppercase().chars().filter(|&c| c != '-').collect();
    (code.chars().count() == 10 && code.chars().all(|c| ALPHABET.contains(c))).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::invite_id;

    #[test]
    fn invite_codes() {
        assert_eq!(invite_id("https://lovearound.me/i/GSSV7-A33HN").as_deref(), Some("GSSV7-A33HN"));
        assert_eq!(invite_id("lovearound://add/GSSV7-A33HN").as_deref(), Some("GSSV7-A33HN"));
        assert_eq!(invite_id("needed://add/gssv7a33hn").as_deref(), Some("gssv7a33hn"));
        assert_eq!(invite_id(" lovearound.me/i/GSSV7A33HN?x=1 ").as_deref(), Some("GSSV7A33HN"));
        assert_eq!(invite_id("K7Q0M-9XWPA"), None);
        assert_eq!(invite_id("hello"), None);
    }
}
