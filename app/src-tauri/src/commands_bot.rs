//! «Mi bot» commands: thin wrappers over `bot`. Only in the `github` edition.

use std::sync::Arc;

use tauri::State;

use crate::bot::{BotClient, BotError, ConnectionStore};
use crate::commands::in_background;
use crate::contract::UiError;
use crate::contract_bot::{BotConnectionUi, BotStatusUi, LinkMethod};
use crate::mobile;

type Store<'a> = State<'a, Arc<ConnectionStore>>;

/// Runs one request to the bot off the UI thread, only while the connection is enabled.
async fn ask_bot(
    store: Store<'_>,
    request: impl FnOnce(&BotClient) -> Result<BotStatusUi, BotError> + Send + 'static,
) -> Result<BotStatusUi, UiError> {
    let store = Arc::clone(&store);
    in_background(move || Ok(request(&store.client()?)?)).await
}

#[tauri::command]
pub(crate) async fn bot_connection(store: Store<'_>) -> Result<BotConnectionUi, UiError> {
    let store = Arc::clone(&store);
    in_background(move || Ok(store.summary()?)).await
}

#[tauri::command]
pub(crate) async fn bot_connect(
    url: String,
    token: Option<String>,
    store: Store<'_>,
) -> Result<BotConnectionUi, UiError> {
    let store = Arc::clone(&store);
    in_background(move || Ok(store.connect(&url, token.as_deref())?)).await
}

#[tauri::command]
pub(crate) async fn bot_disable(store: Store<'_>) -> Result<BotConnectionUi, UiError> {
    let store = Arc::clone(&store);
    in_background(move || Ok(store.disable()?)).await
}

#[tauri::command]
pub(crate) async fn bot_status(store: Store<'_>) -> Result<BotStatusUi, UiError> {
    ask_bot(store, BotClient::status).await
}

#[tauri::command]
pub(crate) async fn bot_activate(store: Store<'_>) -> Result<BotStatusUi, UiError> {
    ask_bot(store, BotClient::activate).await
}

#[tauri::command]
pub(crate) async fn bot_pause(store: Store<'_>) -> Result<BotStatusUi, UiError> {
    ask_bot(store, BotClient::pause).await
}

#[tauri::command]
pub(crate) async fn bot_link(method: LinkMethod, store: Store<'_>) -> Result<BotStatusUi, UiError> {
    ask_bot(store, move |client| client.link(method)).await
}

#[tauri::command]
pub(crate) async fn bot_unlink(store: Store<'_>) -> Result<BotStatusUi, UiError> {
    ask_bot(store, BotClient::unlink).await
}

/// Desktop only: opens the bot's chat in the browser. The interface cannot open any other address.
#[tauri::command]
pub(crate) async fn bot_open_chat(number: String) -> Result<(), UiError> {
    let url = chat_url(&number)?;
    in_background(move || browser::open(&url)).await
}

fn chat_url(number: &str) -> Result<String, UiError> {
    Ok(format!("https://wa.me/{}", mobile::bot_number(number)?))
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

    /// `url` carries the bot's number: it never goes into a message or a log.
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

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod browser {
    use crate::contract::{UiError, UiErrorKind};

    pub(super) fn open(_url: &str) -> Result<(), UiError> {
        Err(UiError::new(
            UiErrorKind::Other,
            "en este sistema abre tú el chat del bot en WhatsApp",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_chat_with_a_valid_number_can_be_opened() {
        assert_eq!(
            chat_url("34612345678").unwrap(),
            "https://wa.me/34612345678"
        );
        for number in [
            "",
            "1234567",
            "1234567890123456",
            "+34612345678",
            "34612345678?text=a",
            "34612345678/../x",
            "example.com",
        ] {
            assert!(chat_url(number).is_err(), "{number}");
        }
    }
}
