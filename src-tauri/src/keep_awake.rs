//! The app never naps while it runs: it serves the phone, streams terminals and syncs with its window hidden or minimized, so every OS energy throttle on it is opted out (docs/feat/background-refresh.md § Never throttled). System sleep is not blocked.
//! Three layers, each per platform: the process (`platform::process`), the webview engine's hidden-page throttling (`platform::webview`), and the webview suspension Tauri exposes as config (`tauri.conf.json` `backgroundThrottling`, plus `additionalBrowserArgs` for WebView2).

use tauri::Manager;

/// Opts the process and every open webview out of energy throttling for the app's lifetime; call once at setup.
pub fn hold(app: &tauri::AppHandle) {
    report("process", platform::process());
    for window in app.webview_windows().values() {
        let applied = window.with_webview(|webview| report("webview", platform::webview(&webview)));
        if let Err(e) = applied {
            report("webview", Err(e.to_string()));
        }
    }
}

fn report(layer: &str, result: Result<(), String>) {
    match result {
        Ok(()) => crate::logger::info("POWER", &format!("{layer} throttling opted out")),
        Err(e) => crate::logger::error("POWER", &format!("{layer} throttling opt-out failed: {e}")),
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2::{msg_send, sel};
    use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString};
    use objc2_web_kit::WKWebView;

    /// App Nap throttles a hidden app's process; a user-initiated activity exempts it.
    pub fn process() -> Result<(), String> {
        let reason =
            NSString::from_str("Serves the phone companion, terminals and syncs while hidden");
        let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
            NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
            &reason,
        );
        // The exemption ends when the token is released, so it must outlive the process.
        std::mem::forget(activity);
        Ok(())
    }

    /// WebKit clamps a hidden page's timers to one per second and suppresses its WebContent process; `backgroundThrottling` covers neither.
    pub fn webview(webview: &tauri::webview::PlatformWebview) -> Result<(), String> {
        let wk = unsafe { &*(webview.inner() as *const WKWebView) };
        let prefs = unsafe { wk.configuration().preferences() };
        // Private WKPreferences setters: probed first, because a missing selector would raise and abort.
        let throttling = sel!(_setHiddenPageDOMTimerThrottlingEnabled:);
        let suppression = sel!(_setPageVisibilityBasedProcessSuppressionEnabled:);
        if !prefs.respondsToSelector(throttling) || !prefs.respondsToSelector(suppression) {
            return Err("this WebKit has no hidden-page throttling switch".into());
        }
        unsafe {
            let _: () = msg_send![&*prefs, _setHiddenPageDOMTimerThrottlingEnabled: false];
            let _: () = msg_send![&*prefs, _setPageVisibilityBasedProcessSuppressionEnabled: false];
        }
        Ok(())
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation,
        PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_STATE,
    };

    /// EcoQoS throttles a background process's CPU; control bit set with state bit clear opts out.
    pub fn process() -> Result<(), String> {
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: 0,
        };
        let ok = unsafe {
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                &state as *const PROCESS_POWER_THROTTLING_STATE as *const core::ffi::c_void,
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            )
        };
        if ok == 0 {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(())
        }
    }

    /// WebView2's background throttling is Chromium's, switched off by the browser flags in `tauri.conf.json` `additionalBrowserArgs`.
    pub fn webview(_webview: &tauri::webview::PlatformWebview) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    /// Linux desktops have no per-app nap to opt out of.
    pub fn process() -> Result<(), String> {
        Ok(())
    }

    /// WebKitGTK exposes no hidden-page throttling switch; measure it when a Linux build ships.
    pub fn webview(_webview: &tauri::webview::PlatformWebview) -> Result<(), String> {
        Ok(())
    }
}
