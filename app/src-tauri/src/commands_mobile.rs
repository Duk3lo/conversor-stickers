//! Commands that only make sense on Android. They exist on every platform so `generate_handler!`
//! needs no `cfg`; on desktop they answer that they are Android-only. Logic that does not need
//! Tauri lives in separate functions so it can be tested.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, State, Url};
use tauri_plugin_dialog::FilePath;
use tracing::{info, warn};

use crate::commands::in_background;
use crate::commands_packs::LibraryState;
use crate::contract::{EntryUi, MediaKind, MediaOrigin, MediaUi, UiError, UiErrorKind};
use crate::mobile::{self, AddResponse, SharedMedia, android_only};
use crate::session::{PASTE_LIMIT_BYTES, Session};
use crate::whatsapp::{self, WhatsappError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AddOutcome {
    Added,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WhatsappOutcome {
    pub(crate) result: AddOutcome,
    pub(crate) notice: Option<String>,
}

const NO_MEDIA_IN_CLIPBOARD: &str = "No hay ninguna imagen ni video en el portapapeles";

pub(crate) fn whatsapp_directory(app_data: &Path) -> PathBuf {
    app_data.join("whatsapp")
}

impl From<WhatsappError> for UiError {
    fn from(error: WhatsappError) -> Self {
        let kind = match error {
            WhatsappError::InvalidPack(_) => UiErrorKind::InvalidPack,
            WhatsappError::Disk { .. } => UiErrorKind::Other,
        };
        UiError::new(kind, error.to_string())
    }
}

pub(crate) fn translate_response(response: AddResponse) -> Result<WhatsappOutcome, UiError> {
    let result = |result| {
        Ok(WhatsappOutcome {
            result,
            notice: None,
        })
    };
    match response.result.as_str() {
        "added" => result(AddOutcome::Added),
        "cancelled" => result(AddOutcome::Cancelled),
        "not_installed" => Err(UiError::new(
            UiErrorKind::Other,
            "WhatsApp no está instalado",
        )),
        "validation_error" => Err(UiError::new(
            UiErrorKind::InvalidPack,
            response
                .reason
                .unwrap_or_else(|| "WhatsApp no aceptó el paquete".to_string()),
        )),
        other => {
            warn!(response = other, "el puente dio una respuesta desconocida");
            Err(UiError::new(
                UiErrorKind::Other,
                "WhatsApp dio una respuesta que no se entiende",
            ))
        }
    }
}

fn content_uri(uri: &str) -> Result<Url, UiError> {
    Url::parse(uri)
        .ok()
        .filter(|url| url.scheme() == "content")
        .ok_or_else(|| {
            UiError::new(
                UiErrorKind::InvalidEntry,
                "lo que hay en el portapapeles no se puede leer",
            )
        })
}

fn read_and_delete_backup(path: &Path) -> Result<Vec<u8>, UiError> {
    let read = fs::read(path);
    let _ = fs::remove_file(path);
    if let Some(folder) = path.parent() {
        let _ = fs::remove_dir(folder);
    }
    read.map_err(|error| {
        UiError::new(
            UiErrorKind::InvalidEntry,
            format!("no se pudo leer lo pegado: {error}"),
        )
    })
}

pub(crate) fn prune_on_startup(app_data: PathBuf, library: Arc<LibraryState>) {
    if !cfg!(target_os = "android") {
        return;
    }
    std::thread::spawn(move || prune_backups(&app_data, &library));
}

pub(crate) fn prune_backups(app_data: &Path, library: &LibraryState) {
    let ids = match library.with(|opened| Ok(opened.publishable_ids())) {
        Ok(ids) => ids,
        Err(error) => {
            warn!(%error, "no se podaron las copias de WhatsApp: la biblioteca no abre");
            return;
        }
    };
    match whatsapp::prune(&whatsapp_directory(app_data), &ids) {
        Ok(0) => {}
        Ok(removed) => info!(
            removed,
            "copias de WhatsApp de paquetes que ya no valen, podadas"
        ),
        Err(error) => warn!(%error, "no se pudieron podar las copias de WhatsApp"),
    }
}

#[tauri::command]
pub(crate) async fn add_to_whatsapp(
    pack: u32,
    app: AppHandle,
    library: State<'_, Arc<LibraryState>>,
) -> Result<WhatsappOutcome, UiError> {
    if !cfg!(target_os = "android") {
        return Err(android_only());
    }
    let library = Arc::clone(&library);
    in_background(move || {
        let publishable = library.with(|opened| opened.to_publish(pack))?;
        let app_data = app.path().app_data_dir().map_err(|error| {
            UiError::new(
                UiErrorKind::Other,
                format!("sin carpeta de datos de la app: {error}"),
            )
        })?;
        whatsapp::check_files(&publishable)?;
        let published = whatsapp::publish(&whatsapp_directory(&app_data), &publishable)?;
        match published.image_version.parse::<u32>() {
            Ok(version) => library.with(|opened| opened.mark_published(pack, version))?,
            Err(error) => warn!(pack, %error, "la versión publicada no cabe en un número"),
        }
        let response = mobile::add_whatsapp_pack(&app, &published.id, &published.name, false)?;
        translate_response(response)
    })
    .await
}

fn send_quick(
    app: &AppHandle,
    library: &LibraryState,
    bytes: &[u8],
    media_origin: MediaOrigin,
) -> Result<WhatsappOutcome, UiError> {
    let (pack, publishable) = library.with(|opened| {
        let pack = opened.add_quick_sticker(bytes, media_origin)?;
        Ok((pack, opened.to_publish_filled(pack)?))
    })?;
    let app_data = app.path().app_data_dir().map_err(|error| {
        UiError::new(
            UiErrorKind::Other,
            format!("sin carpeta de datos de la app: {error}"),
        )
    })?;
    whatsapp::check_files(&publishable)?;
    let published = whatsapp::publish(&whatsapp_directory(&app_data), &publishable)?;
    match published.image_version.parse::<u32>() {
        Ok(version) => library.with(|opened| opened.mark_published(pack, version))?,
        Err(error) => warn!(pack, %error, "la versión publicada no cabe en un número"),
    }
    let response = mobile::add_whatsapp_pack(app, &published.id, &published.name, true)?;
    translate_response(response)
}

#[tauri::command]
pub(crate) async fn send_result_to_whatsapp(
    id: u32,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
    library: State<'_, Arc<LibraryState>>,
) -> Result<WhatsappOutcome, UiError> {
    require_android()?;
    let session = Arc::clone(&session);
    let library = Arc::clone(&library);
    in_background(move || {
        if session.proposal_saved(id)?.extension != "webp" {
            return Err(UiError::new(
                UiErrorKind::InvalidEntry,
                "solo se pueden enviar stickers",
            ));
        }
        send_quick(
            &app,
            &library,
            &session.result(id)?,
            session.media_origin(id)?,
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn send_sticker_to_whatsapp(
    pack: u32,
    sticker: u32,
    app: AppHandle,
    library: State<'_, Arc<LibraryState>>,
) -> Result<WhatsappOutcome, UiError> {
    require_android()?;
    let library = Arc::clone(&library);
    in_background(move || {
        let exported = library.with(|opened| opened.sticker_to_export(pack, sticker))?;
        send_quick(&app, &library, &exported.bytes, exported.media_origin)
    })
    .await
}

#[tauri::command]
pub(crate) async fn paste_from_clipboard(
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<EntryUi, UiError> {
    if !cfg!(target_os = "android") {
        return Err(android_only());
    }
    let session = Arc::clone(&session);
    let entry = in_background(move || {
        crate::commands::prepare_executables(&app, &session)?;
        let content = mobile::read_clipboard(&app)?;
        let Some(uri) = content.uri else {
            return Err(UiError::new(
                UiErrorKind::InvalidEntry,
                NO_MEDIA_IN_CLIPBOARD,
            ));
        };
        let url = content_uri(&uri)?;
        let backup = mobile::local_path(&app, FilePath::Url(url), PASTE_LIMIT_BYTES as u64)?;
        let bytes = read_and_delete_backup(&backup)?;
        let entry = session.paste_bytes(&bytes)?;
        if let Some(MediaUi::Image { path, .. }) = &entry.media
            && let Err(error) = app.asset_protocol_scope().allow_file(path)
        {
            warn!(%error, "no se pudo permitir ver la imagen pegada");
        }
        Ok(entry)
    })
    .await?;
    Ok(entry)
}

const SHARED_VIDEO_LIMIT_BYTES: u64 = 500 * 1024 * 1024;

fn shared_media_kind(mime: Option<&str>) -> MediaKind {
    match mime {
        Some("image/png" | "image/jpeg" | "image/webp" | "image/bmp") => MediaKind::Image,
        _ => MediaKind::Video,
    }
}

fn media_limit(kind: MediaKind) -> u64 {
    match kind {
        MediaKind::Image => PASTE_LIMIT_BYTES as u64,
        MediaKind::Video => SHARED_VIDEO_LIMIT_BYTES,
    }
}

#[tauri::command]
pub(crate) async fn receive_shared(
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<Vec<EntryUi>, UiError> {
    require_android()?;
    let session = Arc::clone(&session);
    in_background(move || {
        let shared = mobile::take_shared(&app)?;
        crate::edition::require_direct_shared_links(crate::edition::PLAY, &shared.links)?;
        let mut entries = Vec::new();
        for SharedMedia { uri, mime } in &shared.media {
            crate::commands::prepare_executables(&app, &session)?;
            let url = content_uri(uri)?;
            let kind = shared_media_kind(mime.as_deref());
            let backup = mobile::local_path(&app, FilePath::Url(url), media_limit(kind))?;
            let new_ones = match kind {
                MediaKind::Image => session.add_images(vec![backup]),
                MediaKind::Video => session.add_files(vec![backup]),
            };
            new_ones
                .iter()
                .for_each(|entry| crate::commands::allow_image(&app, entry));
            entries.extend(new_ones);
        }
        if !shared.links.is_empty() {
            entries.extend(session.add_links(&shared.links.join("\n"))?);
        }
        Ok(entries)
    })
    .await
}

fn require_android() -> Result<(), UiError> {
    if cfg!(target_os = "android") {
        Ok(())
    } else {
        Err(android_only())
    }
}

fn share_bytes(app: &AppHandle, bytes: &[u8]) -> Result<(), UiError> {
    let cache = app.path().app_cache_dir().map_err(|error| {
        UiError::new(
            UiErrorKind::Other,
            format!("sin carpeta de caché para compartir: {error}"),
        )
    })?;
    let path =
        mobile::stage_for_share(&mobile::share_directory(&cache), bytes).map_err(|error| {
            UiError::new(
                UiErrorKind::Other,
                format!("no se pudo preparar el sticker para compartir: {error}"),
            )
        })?;
    mobile::share_file(app, &path)
}

#[tauri::command]
pub(crate) async fn share_sticker(
    pack: u32,
    sticker: u32,
    app: AppHandle,
    library: State<'_, Arc<LibraryState>>,
) -> Result<(), UiError> {
    require_android()?;
    let library = Arc::clone(&library);
    in_background(move || {
        let exported = library.with(|opened| opened.sticker_to_export(pack, sticker))?;
        share_bytes(&app, &exported.bytes)
    })
    .await
}

#[tauri::command]
pub(crate) async fn share_result(
    id: u32,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<(), UiError> {
    require_android()?;
    let session = Arc::clone(&session);
    in_background(move || {
        if session.proposal_saved(id)?.extension != "webp" {
            return Err(UiError::new(
                UiErrorKind::InvalidEntry,
                "solo se pueden compartir stickers",
            ));
        }
        share_bytes(&app, &session.result(id)?)
    })
    .await
}

/// Opens the folder where the app saved videos or sounds in the file manager.
#[tauri::command]
pub(crate) async fn open_saved_folder(kind: String, app: AppHandle) -> Result<(), UiError> {
    require_android()?;
    let kind = mobile::saved_folder_kind(&kind)?;
    in_background(move || mobile::open_saved_folder(&app, kind)).await
}

#[tauri::command]
pub(crate) async fn preview_path(
    id: u32,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<String, UiError> {
    require_android()?;
    let path = session.local_path(id)?;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|error| {
            UiError::new(
                UiErrorKind::Other,
                format!("no se pudo permitir la vista previa: {error}"),
            )
        })?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const EXAMPLES: &str = include_str!("../../src/contract-mobile.examples.json");

    fn example(name: &str) -> Value {
        let examples: Value = serde_json::from_str(EXAMPLES).unwrap();
        examples[name].clone()
    }

    fn response(result: &str, reason: Option<&str>) -> AddResponse {
        serde_json::from_value(json!({ "result": result, "reason": reason })).unwrap()
    }

    #[test]
    fn bridge_response_reaches_the_interface() {
        let added = translate_response(response("added", None)).unwrap();
        assert_eq!(
            serde_json::to_value(added).unwrap(),
            example("whatsapp_result_added")
        );
        let cancelled = translate_response(response("cancelled", None)).unwrap();
        assert_eq!(
            serde_json::to_value(cancelled).unwrap(),
            example("whatsapp_result_cancelled")
        );
    }

    #[test]
    fn whatsapp_not_installed_and_validation_give_their_error() {
        assert_eq!(
            translate_response(response("not_installed", None)).unwrap_err(),
            UiError::new(UiErrorKind::Other, "WhatsApp no está instalado")
        );
        assert_eq!(
            translate_response(response("validation_error", Some("nombre largo"))).unwrap_err(),
            UiError::new(UiErrorKind::InvalidPack, "nombre largo")
        );
        let without_reason = translate_response(response("validation_error", None)).unwrap_err();
        assert_eq!(without_reason.kind, UiErrorKind::InvalidPack);
        let odd = translate_response(response("quizas", None)).unwrap_err();
        assert_eq!(odd.kind, UiErrorKind::Other);
    }

    #[test]
    fn android_grants_its_commands_and_desktop_none() {
        let android: Value =
            serde_json::from_str(include_str!("../capabilities/android.json")).unwrap();
        let permissions = android["permissions"].as_array().unwrap();
        for command in [
            "allow-add-to-whatsapp",
            "allow-paste-from-clipboard",
            "allow-preview-path",
            "allow-receive-shared",
            "allow-share-sticker",
            "allow-share-result",
            "allow-send-result-to-whatsapp",
            "allow-send-sticker-to-whatsapp",
            "allow-open-saved-folder",
        ] {
            assert!(permissions.contains(&json!(command)), "{command}");
            assert!(!include_str!("../capabilities/main.json").contains(command));
        }
    }

    #[test]
    fn shared_content_is_split_between_image_and_video() {
        assert_eq!(shared_media_kind(Some("image/png")), MediaKind::Image);
        assert_eq!(shared_media_kind(Some("image/jpeg")), MediaKind::Image);
        for video in [Some("video/mp4"), Some("image/gif"), None] {
            assert_eq!(shared_media_kind(video), MediaKind::Video);
        }
        assert!(media_limit(MediaKind::Video) > media_limit(MediaKind::Image));
    }

    #[test]
    fn shared_content_from_bridge_is_read() {
        let read: mobile::Shared = serde_json::from_value(json!({
            "links": ["https://a.example/v"],
            "media": [{ "uri": "content://m/1", "mime": null }]
        }))
        .unwrap();
        assert_eq!(read.links, ["https://a.example/v"]);
        assert_eq!(read.media[0].mime, None);
    }

    #[test]
    fn bridge_response_without_reason_is_read() {
        let read: AddResponse =
            serde_json::from_value(json!({ "result": "not_installed" })).unwrap();
        assert_eq!(read.reason, None);
    }

    #[test]
    fn clipboard_only_returns_content_uris() {
        assert!(content_uri("content://media/external/images/1").is_ok());
        for bad in ["file:///data/x.png", "https://x.org/a.png", "no es url", ""] {
            let error = content_uri(bad).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidEntry, "{bad}");
        }
    }

    #[test]
    fn read_copy_is_deleted_with_its_folder() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("0");
        fs::create_dir(&folder).unwrap();
        let backup = folder.join("foto.png");
        fs::write(&backup, b"png").unwrap();
        assert_eq!(read_and_delete_backup(&backup).unwrap(), b"png");
        assert!(!folder.exists());
        assert!(read_and_delete_backup(&backup).is_err());
    }

    #[test]
    fn whatsapp_error_flags_if_it_is_the_packs() {
        let not_valid = UiError::from(WhatsappError::InvalidPack("x".into()));
        assert_eq!(not_valid.kind, UiErrorKind::InvalidPack);
        let disk = UiError::from(WhatsappError::Disk {
            action: "copiar",
            source: std::io::Error::other("lleno"),
        });
        assert_eq!(disk.kind, UiErrorKind::Other);
    }

    #[test]
    #[cfg(not(target_os = "android"))]
    fn on_desktop_commands_say_it_is_android_only() {
        let expected = UiError::new(UiErrorKind::Other, "solo está disponible en Android");
        let error = require_android().unwrap_err();
        assert_eq!(error, expected);
    }
}
