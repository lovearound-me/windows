# Windows app

The Mac app's twin for Windows 10 and 11: a heart in the tray, the same
popover, book, settings and glow, the same server and account. Tauri 2: Rust
for everything the Mac app does in Swift, WebView2 pages for the views. The
bundle id is `me.lovearound.windows`; links open through `lovearound://` (and the
old `needed://`) as on the Mac. Data lives in
`%APPDATA%\me.lovearound.windows` (`session.json`, `settings.json`): colors
and settings stay on this PC, as on the Mac.

## What it does

As the [Mac app](https://github.com/lovearound-me/mac#readme) does, with Windows in place of macOS:

- A left click on the heart in the tray opens the popover next to it; a
  right click: pause, "Книга…", "Настройки…", "Проверить обновления…", quit.
- The overlay is one transparent, click-through, never-focused window per
  monitor, hidden from screen capture (`WDA_EXCLUDEFROMCAPTURE`).
- "Тихо, когда что-то на весь экран" asks `SHQueryUserNotificationState`:
  a full-screen game or video, a presentation, or "busy" keep signals quiet.
- "Запускать при входе в Windows": the Run key in HKCU (autostart plugin);
  on by default after the first launch.
- Updates: hourly, `lovearound.me/download/windows/latest.json`, signed with
  the minisign key in `~/.config/lovearound/tauri_updater.key` (public half
  in `tauri.conf.json`). Installed once nothing of the app is open. Without
  that key no update reaches the installed apps: keep a copy elsewhere.

## Layout

| Path | Contains |
|---|---|
| `src-tauri/src/config.rs` | API base, Firebase key, site, scheme |
| `auth.rs`, `api.rs`, `stream.rs` | Firebase Auth over REST, the `/v1` client, `WS /v1/stream` — ports of the Swift files |
| `model.rs` | the state (`AppModel.swift`); every change is sent whole to the pages as `state` |
| `settings.rs` | what stays on this PC |
| `tray.rs`, `ui.rs` | the tray icon and menu; the popover, book and settings windows |
| `overlay.rs`, `presenting.rs` | the glow windows; full screen detection |
| `updates.rs` | the updater |
| `main.rs` | commands the pages call, plugins, startup |
| `ui/` | the pages: Preact + htm (vendored, no build step), `lib.js` shared |

## Build

No Windows machine needed: `./build.sh` cross-builds in Docker
(`Dockerfile.build`: cargo-xwin with the MSVC target, NSIS) and leaves
`dist/LoveAroundSetup.exe`, its updater `.sig` and `LoveAround.version`.
`./publish.sh` makes them the GitHub release `v<version>` of this repository,
with `latest.json` for the updater; lovearound.me/download/LoveAroundSetup.exe
and /download/windows/latest.json redirect to the latest release. The version
is `0.2.<commit count of this repository>` (0.1.x came from the old shared one).
Normally a release is **Actions → Release → Run workflow** (or
`gh workflow run release -R lovearound-me/windows`): the same two scripts on
a GitHub runner, with the updater key in the secret `TAURI_SIGNING_PRIVATE_KEY`.

The installer has no Authenticode signature, so SmartScreen warns on the first
run ("Подробнее" → "Выполнить в любом случае"); the site's FAQ says so. It
installs for the current user, no admin rights, and fetches WebView2 if the
PC lacks it.
