use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::model::{Model, Route};

/// The popover's width; its height follows the content (`fit`).
const POPOVER_WIDTH: f64 = 330.0;
/// Space between the popover and the taskbar or the screen edge, in logical px.
const MARGIN: f64 = 10.0;

/// Where the tray icon was last clicked, in physical px: the popover opens next to it.
static TRAY_RECT: Mutex<Option<(i32, i32, u32, u32)>> = Mutex::new(None);
/// A click on the tray icon first takes the focus from the popover, which
/// hides it: that click must not show it again right away.
static HIDDEN_AT: Mutex<Option<Instant>> = Mutex::new(None);

fn popover(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(window) = app.get_webview_window("popover") {
        return Some(window);
    }
    let window = WebviewWindowBuilder::new(app, "popover", WebviewUrl::App("popover.html".into()))
        .title("Love Around")
        .additional_browser_args(BROWSER_ARGS)
        .inner_size(POPOVER_WIDTH, 420.0)
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .shadow(true)
        .visible(false)
        .build()
        .ok()?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            hide_popover(&handle);
        }
    });
    Some(window)
}

pub fn toggle_popover(app: &AppHandle, rect: Option<(i32, i32, u32, u32)>) {
    let visible = app.get_webview_window("popover").is_some_and(|w| w.is_visible().unwrap_or(false));
    if visible {
        hide_popover(app);
        return;
    }
    if HIDDEN_AT.lock().unwrap().is_some_and(|at| at.elapsed() < Duration::from_millis(300)) {
        return;
    }
    show_popover(app, rect);
}

pub fn show_popover(app: &AppHandle, rect: Option<(i32, i32, u32, u32)>) {
    if rect.is_some() {
        *TRAY_RECT.lock().unwrap() = rect;
    }
    let Some(window) = popover(app) else { return };
    place(&window);
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Each opening starts on the people. Except a half-filled new contact: its
/// link is often being copied elsewhere.
pub fn hide_popover(app: &AppHandle) {
    let Some(window) = app.get_webview_window("popover") else { return };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let _ = window.hide();
    *HIDDEN_AT.lock().unwrap() = Some(Instant::now());
    let model = app.state::<Arc<Model>>();
    if !matches!(model.read(|s| s.route.clone()), Route::Add { .. }) {
        model.set_route(Route::Main);
    }
}

/// Next to the tray icon, inside the monitor's work area: above a taskbar at
/// the bottom, below one at the top. Without a click (a link, a launch) it
/// goes to the corner where the notification area usually is.
fn place(window: &WebviewWindow) {
    let rect = *TRAY_RECT.lock().unwrap();
    let monitor = match rect {
        Some((x, y, w, h)) => window.monitor_from_point(x as f64 + w as f64 / 2.0, y as f64 + h as f64 / 2.0).ok().flatten(),
        None => None,
    }
    .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let (wx, wy, ww, wh) = (work.position.x, work.position.y, work.size.width as i32, work.size.height as i32);
    let Ok(size) = window.outer_size() else { return };
    let (pw, ph) = (size.width as i32, size.height as i32);
    let margin = (MARGIN * scale) as i32;

    let (cx, below) = match rect {
        Some((x, y, w, h)) => (x + w as i32 / 2, y + h as i32 / 2 < wy + wh / 2),
        None => (wx + ww, false),
    };
    let x = (cx - pw / 2).clamp(wx + margin, (wx + ww - pw - margin).max(wx + margin));
    let y = if below { wy + margin } else { (wy + wh - ph - margin).max(wy) };
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// The content's height, from the page: the window follows it.
pub fn fit(window: &WebviewWindow, height: f64) {
    let width = match window.label() {
        "popover" => POPOVER_WIDTH,
        "settings" => SETTINGS_WIDTH,
        _ => return,
    };
    let max = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| m.work_area().size.height as f64 / m.scale_factor() - 2.0 * MARGIN)
        .unwrap_or(900.0);
    let _ = window.set_size(LogicalSize::new(width, height.clamp(120.0, max)));
    if window.label() == "popover" {
        place(window);
    }
}

const SETTINGS_WIDTH: f64 = 420.0;

/// The same for every window, or WebView2 refuses to make the second one:
/// Tauri's defaults plus sound without a click, for the chime in the overlay.
pub const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

/// The settings and the book: ordinary windows, for what takes longer than a
/// glance. Each is brought to the front when it is asked for.
fn window(app: &AppHandle, label: &str, title: &str, page: &str, size: (f64, f64), min: Option<(f64, f64)>) -> Option<WebviewWindow> {
    if let Some(window) = app.get_webview_window(label) {
        return Some(window);
    }
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title(title)
        .additional_browser_args(BROWSER_ARGS)
        .inner_size(size.0, size.1)
        .resizable(min.is_some())
        .maximizable(min.is_some())
        .center()
        .visible(false);
    if let Some((w, h)) = min {
        builder = builder.min_inner_size(w, h);
    }
    builder.build().ok()
}

fn bring(app: &AppHandle, window: &WebviewWindow) {
    hide_popover(app);
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

pub fn show_settings(app: &AppHandle) {
    let model = app.state::<Arc<Model>>();
    if !model.read(|s| s.signed_in) {
        return;
    }
    // Shown by the page once it has its size, so it does not jump.
    let fresh = app.get_webview_window("settings").is_none();
    let Some(window) = window(app, "settings", &tr!("Настройки"), "settings.html", (SETTINGS_WIDTH, 640.0), None) else { return };
    if fresh {
        hide_popover(app);
    } else {
        bring(app, &window);
    }
}

/// `contact`: the one to show on the right ("Изменить" in the popover).
pub fn show_book(app: &AppHandle, contact: Option<String>) {
    let model = app.state::<Arc<Model>>();
    if !model.read(|s| s.signed_in) {
        return;
    }
    if contact.is_some() {
        model.update(|s| s.book_selection = contact);
    }
    let Some(window) = window(app, "book", &tr!("Книга"), "book.html", (700.0, 540.0), Some((620.0, 440.0))) else { return };
    bring(app, &window);
}

/// The page is drawn: a fresh window is shown only now.
pub fn ready(app: &AppHandle, window: &WebviewWindow) {
    if window.label() != "popover" && !window.is_visible().unwrap_or(true) {
        bring(app, window);
    }
}

/// The open windows' titles in the language just chosen (the pages reload themselves).
pub fn retitle(app: &AppHandle) {
    for (label, title) in [("settings", tr!("Настройки")), ("book", tr!("Книга"))] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.set_title(&title);
        }
    }
}

/// Nothing to show once signed out (or the account is gone).
pub fn close_windows(app: &AppHandle) {
    for label in ["settings", "book"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.close();
        }
    }
}

/// The popover, the book or the settings are in use: an update waits.
pub fn in_use(app: &AppHandle) -> bool {
    ["popover", "settings", "book"]
        .iter()
        .filter_map(|label| app.get_webview_window(label))
        .any(|w| w.is_visible().unwrap_or(false))
}
