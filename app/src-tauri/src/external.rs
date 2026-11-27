//! Opening web addresses in the system browser, and keeping the webview inside the app. The
//! interface can only ask for `https` addresses of the sites the app already links to.

use tauri::AppHandle;

use crate::contract::{UiError, UiErrorKind};

/// The sites the interface links to: the repositories on GitHub and the GIF providers' key pages.
/// Keep it equal to `HOSTS` in `ExternalLinks.kt`, which also allows `wa.me`.
const ALLOWED_HOSTS: [&str; 3] = ["github.com", "developers.giphy.com", "partner.klipy.com"];
const MAX_URL_LENGTH: usize = 2048;

fn refused() -> UiError {
    UiError::new(
        UiErrorKind::InvalidEntry,
        "esta dirección no se puede abrir desde la app",
    )
}

/// `https://<allowed host>[/path][?query][#fragment]`, with no user info or port.
pub(crate) fn check_external(url: &str) -> Result<&str, UiError> {
    let rest = url.strip_prefix("https://").ok_or_else(refused)?;
    if url.len() > MAX_URL_LENGTH || !url.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(refused());
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if ALLOWED_HOSTS.contains(&host) {
        Ok(url)
    } else {
        Err(refused())
    }
}

/// Whether the webview may navigate to `url`: only what the app itself serves. Anything else is
/// opened by `launch` or not at all.
pub(crate) fn is_app_address(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "asset" | "ipc" | "blob" | "about" => true,
        "http" | "https" => {
            let host = url.host_str().unwrap_or_default();
            matches!(
                host,
                "tauri.localhost" | "asset.localhost" | "ipc.localhost"
            ) || (host == "localhost" && cfg!(debug_assertions))
        }
        _ => false,
    }
}

/// The plugin that blocks navigations of the webview to a foreign address. On Android it only
/// covers `shouldOverrideUrlLoading` (clicks, redirects and `location`); the main defence there
/// is the click interceptor in the interface.
pub(crate) fn navigation_guard() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("navigation-guard")
        .on_navigation(|_, url| is_app_address(url))
        .build()
}

/// Opens `url` outside the app. `url` may carry a phone number: it never goes into a message.
#[cfg(target_os = "android")]
pub(crate) fn launch(app: &AppHandle, url: &str) -> Result<(), UiError> {
    crate::mobile::open_url(app, url)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn launch(_app: &AppHandle, url: &str) -> Result<(), UiError> {
    browser::open(url)
}

#[cfg(not(any(target_os = "android", target_os = "linux", target_os = "macos")))]
pub(crate) fn launch(_app: &AppHandle, _url: &str) -> Result<(), UiError> {
    Err(UiError::new(
        UiErrorKind::Other,
        "en este sistema abre la dirección a mano en el navegador",
    ))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod browser {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use tracing::warn;

    use crate::contract::{UiError, UiErrorKind};

    #[cfg(target_os = "linux")]
    const OPENER: &str = "xdg-open";
    #[cfg(target_os = "macos")]
    const OPENER: &str = "open";

    /// How long a failing opener has to say so; it may also stay alive as long as the browser.
    const FAILURE_WINDOW: Duration = Duration::from_secs(2);

    pub(super) fn open(url: &str) -> Result<(), UiError> {
        let failed = |reason: String| {
            UiError::new(
                UiErrorKind::Other,
                format!("no se pudo abrir el navegador: {reason}"),
            )
        };
        let mut child = Command::new(OPENER)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| failed(format!("{OPENER}: {error}")))?;
        let started = Instant::now();
        while started.elapsed() < FAILURE_WINDOW {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => return Err(failed(format!("{OPENER} terminó con {status}"))),
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(error) => return Err(failed(error.to_string())),
            }
        }
        // Still running: reap it whenever it ends.
        thread::spawn(move || {
            if let Err(error) = child.wait() {
                warn!(%error, "no se pudo esperar al programa que abre el navegador");
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_addresses_of_known_sites_are_external() {
        for url in [
            "https://github.com/Duk3lo/conversor-stickers-comunidad/blob/main/CONTRIBUTING.md",
            "https://github.com",
            "https://developers.giphy.com",
            "https://partner.klipy.com/?a=1#b",
        ] {
            assert_eq!(check_external(url).unwrap(), url);
        }
        for url in [
            "",
            "http://github.com/x",
            "https://example.com",
            "https://github.com.evil.example/x",
            "https://evil.example/github.com",
            "https://user@github.com/",
            "https://github.com@evil.example/",
            "https://github.com:8443/",
            "https://github.com\\@evil.example/",
            "https://github.com/a b",
            "https://GitHub.com/",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "intent://github.com#Intent;end",
            "https://github.com/\n",
        ] {
            assert!(check_external(url).is_err(), "{url}");
        }
        let long = format!("https://github.com/{}", "a".repeat(MAX_URL_LENGTH));
        assert!(check_external(&long).is_err());
    }

    #[test]
    fn the_webview_only_navigates_inside_the_app() {
        let url = |text: &str| tauri::Url::parse(text).unwrap();
        for inside in [
            "tauri://localhost/index.html",
            "http://tauri.localhost/",
            "https://tauri.localhost/",
            "http://asset.localhost/a.webp",
            "blob:http://tauri.localhost/1",
            "about:blank",
        ] {
            assert!(is_app_address(&url(inside)), "{inside}");
        }
        for outside in [
            "https://github.com/Duk3lo",
            "http://example.com/",
            "https://tauri.localhost.evil.example/",
            "intent://x#Intent;end",
            "file:///etc/passwd",
        ] {
            assert!(!is_app_address(&url(outside)), "{outside}");
        }
    }
}
