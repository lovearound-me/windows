use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::model::{Model, PauseLength, State};
use crate::{ui, updates};

const ID: &str = "main";

/// What the tray shows: the icon and the right-click menu follow it.
#[derive(Clone, PartialEq)]
pub struct Look {
    signed_in: bool,
    paused: bool,
    pause_text: String,
    lang: &'static str,
}

impl Look {
    pub fn of(state: &State) -> Self {
        Look { signed_in: state.signed_in, paused: state.is_paused(), pause_text: state.pause_text(), lang: crate::i18n::lang() }
    }
}

static LAST: Mutex<Option<Look>> = Mutex::new(None);
/// The signal color the heart briefly takes, with a counter so a newer flash wins.
static FLASH: Mutex<Option<u32>> = Mutex::new(None);
static FLASH_GEN: AtomicU64 = AtomicU64::new(0);

/// The heart in the notification area: a click opens the popover, a right
/// click a small menu with the pause, the book, settings, updates and quitting.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(ID)
        .icon(icon(false, None))
        .tooltip("Love Around")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                let scale = tray.app_handle().primary_monitor().ok().flatten().map(|m| m.scale_factor()).unwrap_or(1.0);
                let position = rect.position.to_physical::<i32>(scale);
                let size = rect.size.to_physical::<u32>(scale);
                ui::toggle_popover(tray.app_handle(), Some((position.x, position.y, size.width, size.height)));
            }
        })
        .on_menu_event(|app, event| {
            let model = app.state::<Arc<Model>>();
            match event.id().as_ref() {
                "resume" => model.resume(),
                "pause-hour" => model.pause(PauseLength::Hour),
                "pause-morning" => model.pause(PauseLength::UntilMorning),
                "pause-resumed" => model.pause(PauseLength::UntilResumed),
                "book" => ui::show_book(app, None),
                "settings" => ui::show_settings(app),
                "updates" => updates::check_now(app),
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

fn tray(app: &AppHandle) -> Option<TrayIcon<Wry>> {
    app.tray_by_id(ID)
}

/// Rebuilt only when something it shows has changed.
pub fn refresh(app: &AppHandle, look: Look) {
    {
        let mut last = LAST.lock().unwrap();
        if last.as_ref() == Some(&look) {
            return;
        }
        *last = Some(look.clone());
    }
    let Some(tray) = tray(app) else { return };
    let flash = *FLASH.lock().unwrap();
    let _ = tray.set_icon(Some(icon(look.paused, flash)));
    let _ = tray.set_tooltip(Some(if look.paused { format!("Love Around — {}", look.pause_text.to_lowercase()) } else { "Love Around".into() }));
    if let Ok(menu) = menu(app, &look) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn menu(app: &AppHandle, look: &Look) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    let item = |id: &str, title: &str| MenuItem::with_id(app, id, title, true, None::<&str>);
    if look.signed_in {
        if look.paused {
            // No action: shown greyed, as a status line.
            menu.append(&MenuItem::with_id(app, "pause-text", &look.pause_text, false, None::<&str>)?)?;
            menu.append(&item("resume", &tr!("Снять паузу"))?)?;
        } else {
            menu.append(&item("pause-hour", &tr!("Пауза на час"))?)?;
            menu.append(&item("pause-morning", &tr!("Пауза до утра"))?)?;
            menu.append(&item("pause-resumed", &tr!("Пауза, пока не сниму"))?)?;
        }
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&item("book", &tr!("Книга…"))?)?;
        menu.append(&item("settings", &tr!("Настройки…"))?)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&item("updates", &tr!("Проверить обновления…"))?)?;
    menu.append(&item("quit", &tr!("Закрыть приложение"))?)?;
    Ok(menu)
}

/// The icon briefly takes the signal color when a signal arrives.
pub fn flash(app: &AppHandle, color: u32) {
    let generation = FLASH_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    *FLASH.lock().unwrap() = Some(color);
    let paused = LAST.lock().unwrap().as_ref().is_some_and(|l| l.paused);
    if let Some(tray) = tray(app) {
        let _ = tray.set_icon(Some(icon(paused, Some(color))));
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1400)).await;
        if FLASH_GEN.load(Ordering::SeqCst) != generation {
            return;
        }
        *FLASH.lock().unwrap() = None;
        let paused = LAST.lock().unwrap().as_ref().is_some_and(|l| l.paused);
        if let Some(tray) = tray(&app) {
            let _ = tray.set_icon(Some(icon(paused, None)));
        }
    });
}

/// A heart in a circle, drawn at 32 px (Windows scales it to the tray):
/// pink and filled, grey and crossed out on pause, the signal color in a flash.
fn icon(paused: bool, flash: Option<u32>) -> Image<'static> {
    const SIZE: u32 = 32;
    let s = SIZE as f32;
    let mut pixmap = Pixmap::new(SIZE, SIZE).expect("pixmap");
    let rgb = |c: u32| Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, 255);
    let paint = |color: Color| {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        paint
    };
    let circle = PathBuilder::from_circle(s / 2.0, s / 2.0, s / 2.0 - 1.0).expect("circle");
    let heart = heart(s);

    if paused && flash.is_none() {
        let grey = rgb(0x8A8A8A);
        let stroke = Stroke { width: 2.2, ..Stroke::default() };
        let inner = PathBuilder::from_circle(s / 2.0, s / 2.0, s / 2.0 - 2.1).expect("circle");
        pixmap.stroke_path(&inner, &paint(grey), &stroke, Transform::identity(), None);
        pixmap.fill_path(&heart, &paint(grey), FillRule::Winding, Transform::identity(), None);
        let mut slash = PathBuilder::new();
        slash.move_to(s * 0.22, s * 0.22);
        slash.line_to(s * 0.78, s * 0.78);
        let slash = slash.finish().expect("slash");
        // A clear gap around the slash, then the slash itself.
        let mut clear = Paint::default();
        clear.set_color(Color::TRANSPARENT);
        clear.blend_mode = tiny_skia::BlendMode::Clear;
        pixmap.stroke_path(&slash, &clear, &Stroke { width: 5.0, ..Stroke::default() }, Transform::identity(), None);
        pixmap.stroke_path(&slash, &paint(grey), &Stroke { width: 2.2, ..Stroke::default() }, Transform::identity(), None);
    } else {
        let fill = rgb(flash.unwrap_or(0xE0596B));
        pixmap.fill_path(&circle, &paint(fill), FillRule::Winding, Transform::identity(), None);
        pixmap.fill_path(&heart, &paint(rgb(0xFFFFFF)), FillRule::Winding, Transform::identity(), None);
    }
    // tiny-skia keeps premultiplied pixels; the tray wants straight RGBA.
    let rgba: Vec<u8> = pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect();
    Image::new_owned(rgba, SIZE, SIZE)
}

fn heart(s: f32) -> tiny_skia::Path {
    let p = |x: f32, y: f32| (s * (0.2 + x * 0.6), s * (0.2 + y * 0.6));
    let mut b = PathBuilder::new();
    let (x, y) = p(0.5, 0.92);
    b.move_to(x, y);
    let curve = |b: &mut PathBuilder, a: (f32, f32), c: (f32, f32), d: (f32, f32)| {
        let (a, c, d) = (p(a.0, a.1), p(c.0, c.1), p(d.0, d.1));
        b.cubic_to(a.0, a.1, c.0, c.1, d.0, d.1);
    };
    curve(&mut b, (0.12, 0.62), (-0.04, 0.38), (0.1, 0.18));
    curve(&mut b, (0.24, 0.0), (0.44, 0.04), (0.5, 0.24));
    curve(&mut b, (0.56, 0.04), (0.76, 0.0), (0.9, 0.18));
    curve(&mut b, (1.04, 0.38), (0.88, 0.62), (0.5, 0.92));
    b.close();
    b.finish().expect("heart")
}
