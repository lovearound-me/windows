use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// The incoming-signal effect: a glow along the screen edges and emoji
/// floating up, on every display, above everything, never taking a click.
/// One transparent window per monitor, drawn by overlay.html.

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Emoji {
    /// Hearts among hugs, cats and the rest: warmth from anyone close.
    Warm,
    /// What the sender of a signal to anyone sees.
    Stars,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Play {
    color: String,
    emoji: Emoji,
    count: i64,
    calm: bool,
    sound: bool,
}

/// The monitors the windows were made for, to notice a change.
static MONITORS: Mutex<Vec<(i32, i32, u32, u32)>> = Mutex::new(Vec::new());
/// Each play hides the windows after it; a newer play cancels that.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn monitors(app: &AppHandle) -> Vec<(i32, i32, u32, u32)> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| (m.position().x, m.position().y, m.size().width, m.size().height))
        .collect()
}

/// Displays come and go: one window per monitor, recreated on each change.
pub fn watch(app: &AppHandle) {
    rebuild(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if *MONITORS.lock().unwrap() != monitors(&app) {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || rebuild(&handle));
            }
        }
    });
}

fn rebuild(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    for (label, window) in app.webview_windows() {
        if label.starts_with("overlay-") {
            let _ = window.destroy();
        }
    }
    let list = monitors(app);
    for (i, &(x, y, w, h)) in list.iter().enumerate() {
        let built = WebviewWindowBuilder::new(app, format!("overlay-{i}"), WebviewUrl::App("overlay.html".into()))
            .title("Love Around")
            .additional_browser_args(crate::ui::BROWSER_ARGS)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .visible(false)
            .build();
        let Ok(window) = built else { continue };
        let _ = window.set_position(PhysicalPosition::new(x, y));
        let _ = window.set_size(PhysicalSize::new(w, h));
        let _ = window.set_ignore_cursor_events(true);
        native::prepare(&window);
    }
    *MONITORS.lock().unwrap() = list;
}

fn overlays(app: &AppHandle) -> Vec<WebviewWindow> {
    let mut list: Vec<_> = app.webview_windows().into_iter().filter(|(label, _)| label.starts_with("overlay-")).collect();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    list.into_iter().map(|(_, w)| w).collect()
}

/// `calm`: one soft fade instead of flashes, and nothing flies. The sound
/// plays once, from the first monitor's window.
pub fn play(app: &AppHandle, color: u32, emoji: Emoji, count: i64, calm: bool, sound: bool) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let windows = overlays(app);
    for (i, window) in windows.iter().enumerate() {
        native::show(window);
        let play = Play { color: format!("#{color:06X}"), emoji, count, calm, sound: sound && i == 0 };
        let _ = window.emit_to(window.label(), "play", play);
    }
    // The glow takes 2.6 s; emoji start within 1.3 s and rise for up to 4.6 s.
    let longest = if calm || count == 0 { 2.9 } else { 6.2 };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs_f64(longest)).await;
        if GENERATION.load(Ordering::SeqCst) == generation {
            for window in overlays(&app) {
                let _ = window.emit_to(window.label(), "clear", ());
                let _ = window.hide();
            }
        }
    });
}

#[cfg(windows)]
mod native {
    use tauri::WebviewWindow;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowDisplayAffinity, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE,
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE, WDA_EXCLUDEFROMCAPTURE, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    };

    fn hwnd(window: &WebviewWindow) -> Option<HWND> {
        window.hwnd().ok().map(|h| HWND(h.0 as _))
    }

    /// Never takes a click or the focus, not in Alt+Tab, and not in screen
    /// sharing or recordings: a glow is for the person at the screen.
    pub fn prepare(window: &WebviewWindow) {
        let Some(hwnd) = hwnd(window) else { return };
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let extra = (WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW).0 as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | extra);
            let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
    }

    /// Shown without taking the focus from whatever the person is doing.
    pub fn show(window: &WebviewWindow) {
        let Some(hwnd) = hwnd(window) else {
            let _ = window.show();
            return;
        };
        // A raw handle is not Send: it crosses to the main thread as a number.
        let raw = hwnd.0 as isize;
        let _ = window.run_on_main_thread(move || unsafe {
            let hwnd = HWND(raw as _);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        });
    }
}

#[cfg(not(windows))]
mod native {
    use tauri::WebviewWindow;

    pub fn prepare(_window: &WebviewWindow) {}

    pub fn show(window: &WebviewWindow) {
        let _ = window.show();
    }
}
