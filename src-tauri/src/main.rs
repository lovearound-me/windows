// No console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod api;
mod auth;
mod config;
mod error;
mod model;
mod overlay;
mod presenting;
mod settings;
mod signin_box;
mod stream;
mod tray;
mod ui;
mod updates;

use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Manager, RunEvent, State, WebviewWindow};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_opener::OpenerExt;

use error::Error;
use model::{Model, PauseLength, Route};

type M<'a> = State<'a, Arc<Model>>;
type Answer<T = ()> = std::result::Result<T, String>;

/// Passed by the login item, so a launch at login stays quiet.
const AUTOSTART_FLAG: &str = "--autostart";

// The pages call these; every one is async, so none runs on the main thread
// while it may wait for the model's lock.

#[tauri::command]
async fn state(model: M<'_>) -> Answer<Value> {
    Ok(model.snapshot())
}

/// The dictionary for the pages (lib.js).
#[tauri::command]
async fn strings() -> Answer<Value> {
    Ok(i18n::strings())
}

#[tauri::command]
async fn send(model: M<'_>, id: String) -> Answer {
    model.send(&id);
    Ok(())
}

#[tauri::command]
async fn send_anyone(model: M<'_>) -> Answer {
    model.send_to_anyone();
    Ok(())
}

#[tauri::command]
async fn sign_in(model: M<'_>, email: String, password: String) -> Answer {
    model.sign_in(email.trim(), &password).await.map_err(|e| e.message())
}

#[tauri::command]
async fn google_sign_in(model: M<'_>) -> Answer {
    model.sign_in_with_google();
    Ok(())
}

#[tauri::command]
async fn cancel_google(model: M<'_>) -> Answer {
    model.update(|s| s.waiting_for_browser = false);
    Ok(())
}

#[tauri::command]
async fn reset_password(model: M<'_>, email: String) -> Answer {
    model.send_password_reset(email.trim()).await.map_err(|e| e.message())
}

#[tauri::command]
async fn sign_out(model: M<'_>) -> Answer {
    model.sign_out();
    Ok(())
}

#[tauri::command]
async fn delete_account(model: M<'_>) -> Answer {
    model.delete_account().await.map_err(|e| e.message())
}

#[tauri::command]
async fn add_contact(model: M<'_>, code: String, label: String, color: i64, close: bool) -> Answer<String> {
    model.add_contact(code.trim(), label.trim(), color, close).await.map_err(|e| e.message())
}

#[tauri::command]
async fn update_contact(model: M<'_>, id: String, label: String, color: i64) -> Answer {
    model.update_contact(&id, label.trim(), color).await.map_err(|e| e.message())
}

#[tauri::command]
async fn remove_contact(model: M<'_>, id: String) -> Answer {
    model.remove(&id).await.map_err(|e| e.message())
}

#[tauri::command]
async fn set_close(model: M<'_>, id: String, close: bool) -> Answer {
    model.set_close(&id, close);
    Ok(())
}

#[tauri::command]
async fn place(model: M<'_>, id: String, close_at: Option<usize>) -> Answer {
    model.place(&id, close_at);
    Ok(())
}

#[tauri::command]
async fn move_circle(model: M<'_>, id: String, target: String) -> Answer {
    model.move_circle(&id, &target);
    Ok(())
}

#[tauri::command]
async fn select_contact(model: M<'_>, id: Option<String>) -> Answer {
    model.update(|s| s.book_selection = id);
    Ok(())
}

/// One setting by its name in settings.json; the rest stay as they are.
#[tauri::command]
async fn set_setting(model: M<'_>, key: String, value: Value) -> Answer {
    match key.as_str() {
        "acceptAnyone" => model.set_accept_anyone(value.as_bool().unwrap_or(true)),
        "launchAtLogin" => model.set_launch_at_login(value.as_bool().unwrap_or(false)),
        "language" => model.set_language(value.as_str().unwrap_or("")),
        _ => model.update(|s| {
            let mut all = serde_json::to_value(&s.settings).unwrap_or_default();
            all[key.as_str()] = value;
            if let Ok(settings) = serde_json::from_value(all) {
                s.settings = settings;
                s.settings.min_gap = settings::nearest_gap(s.settings.min_gap);
            }
        }),
    }
    Ok(())
}

#[tauri::command]
async fn set_problem(model: M<'_>, problem: Option<String>) -> Answer {
    model.update(|s| s.problem = problem);
    Ok(())
}

#[tauri::command]
async fn set_route(model: M<'_>, route: Route) -> Answer {
    model.set_route(route);
    Ok(())
}

#[tauri::command]
async fn pause(model: M<'_>, length: PauseLength) -> Answer {
    model.pause(length);
    Ok(())
}

#[tauri::command]
async fn resume(model: M<'_>) -> Answer {
    model.resume();
    Ok(())
}

#[tauri::command]
async fn devices(model: M<'_>) -> Answer<Vec<api::Device>> {
    model.api.devices().await.map_err(|e| e.message())
}

#[tauri::command]
async fn add_device(model: M<'_>, name: String, capabilities: Vec<String>) -> Answer<api::Device> {
    let (device, token) = model.api.add_device(name.trim(), &capabilities).await.map_err(|e| match e {
        Error::Status(409, _) => tr!("Устройств уже много, отключи лишние"),
        Error::SignedOut => e.message(),
        _ => tr!("Не получилось, попробуй ещё раз"),
    })?;
    model.update(|s| s.created_device = Some((device.name.clone(), token)));
    Ok(device)
}

#[tauri::command]
async fn forget_created_device(model: M<'_>) -> Answer {
    model.update(|s| s.created_device = None);
    Ok(())
}

#[tauri::command]
async fn remove_device(model: M<'_>, id: String) -> Answer {
    model.api.delete_device(&id).await.map_err(|e| e.message())
}

#[tauri::command]
async fn copy(app: AppHandle, text: String) -> Answer {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// The code of an invite link on the clipboard, unless it is the user's own.
#[tauri::command]
async fn pasted_invite(app: AppHandle, model: M<'_>) -> Answer<Option<String>> {
    Ok(model.pasted_invite(app.clipboard().read_text().ok()))
}

#[tauri::command]
async fn open_book(app: AppHandle, contact: Option<String>) -> Answer {
    ui::show_book(&app, contact);
    Ok(())
}

#[tauri::command]
async fn open_settings(app: AppHandle) -> Answer {
    ui::show_settings(&app);
    Ok(())
}

#[tauri::command]
async fn play_effect(model: M<'_>) -> Answer {
    model.play_effect();
    Ok(())
}

#[tauri::command]
async fn fit(window: WebviewWindow, height: f64) -> Answer {
    ui::fit(&window, height);
    Ok(())
}

#[tauri::command]
async fn ready(app: AppHandle, window: WebviewWindow) -> Answer {
    ui::ready(&app, &window);
    Ok(())
}

#[tauri::command]
async fn hide(app: AppHandle, window: WebviewWindow) -> Answer {
    if window.label() == "popover" {
        ui::hide_popover(&app);
    } else {
        let _ = window.close();
    }
    Ok(())
}

#[tauri::command]
async fn open_url(app: AppHandle, url: String) -> Answer {
    if url.starts_with("https://") || url.starts_with("mailto:") {
        app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn check_updates(app: AppHandle) -> Answer {
    updates::check_now(&app);
    Ok(())
}

#[tauri::command]
async fn quit(app: AppHandle) -> Answer {
    app.exit(0);
    Ok(())
}

fn handle_urls(app: &AppHandle, urls: impl IntoIterator<Item = String>) -> bool {
    let model = app.state::<Arc<Model>>();
    let mut any = false;
    for url in urls {
        if config::URL_SCHEMES.iter().any(|s| url.starts_with(&format!("{s}:"))) {
            model.handle_url(&url);
            any = true;
        }
    }
    any
}

fn main() {
    let app = tauri::Builder::default()
        // First: a second launch (a click on a lovearound:// link, the Start menu)
        // hands its arguments to the running app and quits.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if !handle_urls(app, argv.into_iter().skip(1)) {
                ui::show_popover(app, None);
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![AUTOSTART_FLAG])))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            state,
            send,
            send_anyone,
            sign_in,
            google_sign_in,
            cancel_google,
            reset_password,
            sign_out,
            delete_account,
            add_contact,
            update_contact,
            remove_contact,
            set_close,
            place,
            move_circle,
            select_contact,
            set_setting,
            strings,
            set_problem,
            set_route,
            pause,
            resume,
            devices,
            add_device,
            forget_created_device,
            remove_device,
            copy,
            pasted_invite,
            open_book,
            open_settings,
            play_effect,
            fit,
            ready,
            hide,
            open_url,
            check_updates,
            quit,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let model = Model::new(handle.clone());
            app.manage(model.clone());

            // The installer registers lovearound:// and needed://; this keeps
            // them pointing at this copy of the app even if it was moved.
            let _ = app.deep_link().register_all();
            let links = handle.clone();
            app.deep_link().on_open_url(move |event| {
                handle_urls(&links, event.urls().into_iter().map(|u| u.to_string()));
            });

            tray::create(&handle)?;
            overlay::watch(&handle);
            updates::watch(&handle);
            model.start();

            let launched_by_link = app
                .deep_link()
                .get_current()
                .ok()
                .flatten()
                .is_some_and(|urls| handle_urls(&handle, urls.into_iter().map(|u| u.to_string())));
            let at_login = std::env::args().any(|a| a == AUTOSTART_FLAG);
            // The heart is often hidden in the tray's overflow: a launch by hand
            // opens the popover, so the app is visibly there.
            if !launched_by_link && (!at_login || !model.read(|s| s.signed_in)) {
                ui::show_popover(&handle, None);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Love Around");

    app.run(|_app, event| {
        // Closing the book or the settings is not quitting: the app lives in the tray.
        if let RunEvent::ExitRequested { api, code: None, .. } = event {
            api.prevent_exit();
        }
    });
}
