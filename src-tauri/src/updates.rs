use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

use crate::ui;

/// Checks lovearound.me every hour, downloads a new build in the background
/// and installs it on its own. A tray app is rarely quit, so the install does
/// not wait for a quit: it goes as soon as the person isn't using the app.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            let _ = install_when_idle(&app).await;
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
}

async fn install_when_idle(app: &AppHandle) -> tauri_plugin_updater::Result<()> {
    let Some(update) = app.updater()?.check().await? else { return Ok(()) };
    let bytes = update.download(|_, _| {}, || {}).await?;
    // Not while the popover or a window is open: the restart would close it
    // under the cursor. Signals during the restart are missed, like any time
    // the app is offline.
    while ui::in_use(app) {
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
    update.install(bytes)?;
    app.restart();
}

/// "Проверить обновления…": says what it found, and installs on a yes.
pub fn check_now(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let found = match app.updater() {
            Ok(updater) => updater.check().await,
            Err(e) => Err(e),
        };
        let dialog = app.dialog();
        match found {
            Ok(Some(update)) => {
                let yes = dialog
                    .message(tr!("Вышла версия {new}, у тебя {current}. Установить сейчас?", new = update.version, current = update.current_version))
                    .title(tr!("Есть обновление"))
                    .buttons(MessageDialogButtons::OkCancelCustom(tr!("Установить"), tr!("Позже")))
                    .blocking_show();
                if yes {
                    match update.download_and_install(|_, _| {}, || {}).await {
                        Ok(()) => app.restart(),
                        Err(_) => {
                            dialog
                                .message(tr!("Не получилось скачать обновление. Попробуй позже или скачай с lovearound.me."))
                                .kind(MessageDialogKind::Error)
                                .title("Love Around")
                                .blocking_show();
                        }
                    }
                }
            }
            Ok(None) => {
                dialog
                    .message(tr!("У тебя последняя версия, {version}.", version = app.package_info().version))
                    .title("Love Around")
                    .blocking_show();
            }
            Err(_) => {
                dialog
                    .message(tr!("Не получилось проверить обновления. Проверь интернет и попробуй ещё раз."))
                    .kind(MessageDialogKind::Error)
                    .title("Love Around")
                    .blocking_show();
            }
        }
    });
}
