//! «Mi bot» commands: thin wrappers over `bot`. Only in the `github` edition.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::bot::{BotClient, BotError, ConnectionLink, ConnectionStore, PendingLink};
use crate::commands::in_background;
use crate::contract::{UiError, UiErrorKind};
use crate::contract_bot::{BotConnectionUi, BotStatusUi, LinkMethod};
use crate::external;
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

/// Connects with the link the bot shows (pasted by the person): the address and the token are
/// checked here, and the token never goes back to the interface.
#[tauri::command]
pub(crate) async fn bot_connect_link(
    link: String,
    store: Store<'_>,
) -> Result<BotConnectionUi, UiError> {
    let store = Arc::clone(&store);
    in_background(move || Ok(store.connect_link(ConnectionLink::parse(&link)?)?)).await
}

/// Android: collects the link another app opened (the camera scanning the bot's QR) and keeps it
/// until the person confirms it. Returns only its address, or `None` if nothing was opened.
#[tauri::command]
pub(crate) async fn bot_receive_link(
    app: AppHandle,
    pending: State<'_, Arc<PendingLink>>,
    store: Store<'_>,
) -> Result<Option<String>, UiError> {
    let (pending, store) = (Arc::clone(&pending), Arc::clone(&store));
    in_background(move || {
        let Some(link) = mobile::take_bot_link(&app)? else {
            return Ok(None);
        };
        Ok(Some(offer_received(&store, &pending, &link)?))
    })
    .await
}

/// An enabled connection is never replaced by a link from another app, and nothing is left
/// pending if the link is refused.
fn offer_received(
    store: &ConnectionStore,
    pending: &PendingLink,
    link: &str,
) -> Result<String, BotError> {
    store.ensure_not_enabled()?;
    Ok(pending.offer(ConnectionLink::parse(link)?))
}

/// The person confirmed the address `bot_receive_link` returned.
#[tauri::command]
pub(crate) async fn bot_accept_link(
    pending: State<'_, Arc<PendingLink>>,
    store: Store<'_>,
) -> Result<BotConnectionUi, UiError> {
    let (pending, store) = (Arc::clone(&pending), Arc::clone(&store));
    in_background(move || {
        let link = pending.take().ok_or_else(|| {
            UiError::new(
                UiErrorKind::InvalidEntry,
                "no hay ningún enlace por confirmar",
            )
        })?;
        Ok(store.connect_received_link(link)?)
    })
    .await
}

/// The person refused it: the link, token included, is dropped.
#[tauri::command]
pub(crate) fn bot_reject_link(pending: State<'_, Arc<PendingLink>>) {
    drop(pending.take());
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
pub(crate) async fn bot_link(
    method: LinkMethod,
    phone: Option<String>,
    store: Store<'_>,
) -> Result<BotStatusUi, UiError> {
    link_bot(&store, method, phone).await
}

/// The number is checked before the connection is even looked at: a bad one never reaches the bot.
async fn link_bot(
    store: &Arc<ConnectionStore>,
    method: LinkMethod,
    phone: Option<String>,
) -> Result<BotStatusUi, UiError> {
    if let Some(number) = &phone {
        mobile::link_number(number)?;
    }
    let store = Arc::clone(store);
    in_background(move || Ok(store.client()?.link(method, phone.as_deref())?)).await
}

#[tauri::command]
pub(crate) async fn bot_unlink(store: Store<'_>) -> Result<BotStatusUi, UiError> {
    ask_bot(store, BotClient::unlink).await
}

/// Opens the bot's chat (`https://wa.me/<number>`) outside the app, on desktop and Android. The
/// interface cannot open any other address.
#[tauri::command]
pub(crate) async fn bot_open_chat(number: String, app: AppHandle) -> Result<(), UiError> {
    let url = chat_url(&number)?;
    in_background(move || external::launch(&app, &url)).await
}

fn chat_url(number: &str) -> Result<String, UiError> {
    Ok(format!("https://wa.me/{}", mobile::bot_number(number)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invalid_received_link_leaves_nothing_pending() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(directory.path().join("bot.json"));
        let pending = PendingLink::default();
        for link in ["", "https://example.com", "conversorstickers://bot#u=x"] {
            assert!(offer_received(&store, &pending, link).is_err());
            assert!(pending.take().is_none());
        }
    }

    #[test]
    fn an_enabled_connection_refuses_a_received_link() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(directory.path().join("bot.json"));
        let token = "A".repeat(43);
        let link = format!("conversorstickers://bot#u=https%3A%2F%2Fequipo.red.ts.net&t={token}");
        let pending = PendingLink::default();
        offer_received(&store, &pending, &link).unwrap();
        store
            .connect_received_link(pending.take().unwrap())
            .unwrap();
        let error = offer_received(&store, &pending, &link).unwrap_err();
        assert!(matches!(error, BotError::AlreadyConnected));
        assert!(pending.take().is_none());
    }

    #[test]
    fn an_invalid_number_is_refused_before_asking_the_bot() {
        // Without a saved connection `client()` would fail with `NotEnabled`: getting
        // `InvalidEntry` shows the number was checked first.
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(ConnectionStore::new(directory.path().join("bot.json")));
        for number in ["", "1234567", "+34612345678", "34612345678x"] {
            let error = tauri::async_runtime::block_on(link_bot(
                &store,
                LinkMethod::Code,
                Some(number.to_owned()),
            ))
            .err()
            .unwrap();
            assert_eq!(error.kind, UiErrorKind::InvalidEntry, "{number}");
            assert!(error.message.contains("número a vincular"), "{number}");
        }
        let error = tauri::async_runtime::block_on(link_bot(&store, LinkMethod::Qr, None))
            .err()
            .unwrap();
        assert_ne!(error.kind, UiErrorKind::InvalidEntry);
    }

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
