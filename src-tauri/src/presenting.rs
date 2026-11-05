/// The screen is likely being shown to others: something runs full screen (a
/// slideshow, a game, a video) or Windows is in presentation mode. The glow
/// itself is left out of screen sharing, but the sound is not, and a glow in
/// the middle of a talk is a distraction all the same.
///
/// Windows says "busy" (`QUNS_BUSY`) far more often than something is really
/// full screen, and a glow lost that way is lost for good: "busy" counts only
/// when the window in front covers its whole monitor, taskbar included.
#[cfg(windows)]
pub fn now() -> bool {
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    match unsafe { SHQueryUserNotificationState() } {
        Ok(state) if state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE => true,
        Ok(state) if state == QUNS_BUSY => foreground_full_screen(),
        _ => false,
    }
}

/// The window in front fills its monitor exactly (not just maximized, which
/// leaves the taskbar), and it is neither the desktop nor one of ours.
#[cfg(windows)]
fn foreground_full_screen() -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetShellWindow, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    };
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() || window == GetShellWindow() || !IsWindowVisible(window).as_bool() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == std::process::id() {
            return false;
        }
        let mut class = [0u16; 64];
        let len = GetClassNameW(window, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..len]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            return false;
        }
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONULL);
        if monitor.is_invalid() {
            return false;
        }
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let mut rect = RECT::default();
        if !GetMonitorInfoW(monitor, &mut info).as_bool() || GetWindowRect(window, &mut rect).is_err() {
            return false;
        }
        let m = info.rcMonitor;
        rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom
    }
}

#[cfg(not(windows))]
pub fn now() -> bool {
    false
}
