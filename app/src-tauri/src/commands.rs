//! Interface commands for the conversion flow. They stay thin: they validate what arrives over
//! IPC and call `session`, which holds the logic and is tested without Tauri. Anything that
//! blocks (dialogs, `ffprobe`, conversion, disk) runs in `spawn_blocking`.

use std::sync::Arc;

use tauri::ipc::{Channel, InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, State, Window};
use tauri_plugin_dialog::{DialogExt, FilePath, PickerMode};
use tracing::warn;

use crate::contract::{
    CreateRequest, CreateResult, EntryUi, EstimateUi, MediaKind, MediaUi, PresetUi, ProgressUi,
    QueueEvent, StaticRequest, UiError, UiErrorKind,
};
use crate::mobile;
use crate::session::{self, PASTE_LIMIT_BYTES, Session};

const ANDROID_VIDEO_LIMIT_BYTES: u64 = 500 * 1024 * 1024;

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp"];

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "webm", "mkv", "avi", "gif", "3gp", "mpg", "mpeg", "ogv",
];

pub(crate) async fn in_background<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, UiError> + Send + 'static,
) -> Result<T, UiError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            UiError::new(
                UiErrorKind::Other,
                format!("la tarea se interrumpió sin terminar: {error}"),
            )
        })?
}

pub(crate) fn prepare_executables(app: &AppHandle, session: &Session) -> Result<(), UiError> {
    session.set_executables(mobile::executables(app)?);
    Ok(())
}

/// On Android the links are downloaded by the Kotlin bridge (yt-dlp inside the app); on desktop
/// the session runs the system `yt-dlp`.
pub(crate) fn install_bridge(app: &AppHandle, session: &Session) {
    #[cfg(target_os = "android")]
    {
        let app = app.clone();
        session.set_bridge_download(Box::new(
            move |link, mode, temp, progress, cancellation, limit| {
                mobile::download_with_bridge(
                    &app,
                    link.as_text(),
                    mode,
                    temp,
                    progress,
                    cancellation,
                    limit,
                )
            },
        ));
    }
    #[cfg(not(target_os = "android"))]
    let _ = (app, session);
}

#[tauri::command]
pub(crate) fn presets() -> Vec<PresetUi> {
    session::presets()
}

#[tauri::command]
pub(crate) async fn pick_from_gallery(
    kind: MediaKind,
    app: AppHandle,
    window: Window,
    session: State<'_, Arc<Session>>,
) -> Result<Vec<EntryUi>, UiError> {
    let session = Arc::clone(&session);
    in_background(move || {
        let (title, folder, filter, extensions) = match kind {
            MediaKind::Image => (
                "Elige una o varias imágenes",
                app.path().picture_dir(),
                "Imágenes",
                IMAGE_EXTENSIONS,
            ),
            MediaKind::Video => (
                "Elige uno o varios videos",
                app.path().video_dir(),
                "Videos",
                VIDEO_EXTENSIONS,
            ),
        };
        prepare_executables(&app, &session)?;
        let dialog = app.dialog().file();
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let (mode, limit) = match kind {
            MediaKind::Image => (PickerMode::Image, PASTE_LIMIT_BYTES as u64),
            // `Video` would filter by `video/*` and grey out GIF and animated WebP, which are images.
            MediaKind::Video => (PickerMode::Media, ANDROID_VIDEO_LIMIT_BYTES),
        };
        let mut dialog = dialog
            .set_picker_mode(mode)
            .set_title(title)
            .add_filter(filter, extensions)
            .add_filter("Todos los archivos", &["*"]);
        if let Ok(folder) = folder
            && folder.is_dir()
        {
            dialog = dialog.set_directory(folder);
        }
        let Some(picked) = dialog.blocking_pick_files() else {
            return Ok(Vec::new());
        };
        let paths = picked
            .into_iter()
            .map(|picked| mobile::local_path(&app, picked, limit))
            .collect::<Result<Vec<_>, _>>()?;
        let entries = match kind {
            MediaKind::Image => session.add_images(paths),
            MediaKind::Video => session.add_files(paths),
        };
        for entry in &entries {
            allow_image(&app, entry);
        }
        Ok(entries)
    })
    .await
}

fn binary_body(body: &InvokeBody) -> Result<&[u8], UiError> {
    match body {
        InvokeBody::Raw(bytes) => Ok(bytes),
        InvokeBody::Json(_) => Err(UiError::new(
            UiErrorKind::InvalidEntry,
            "lo pegado tiene que llegar como bytes",
        )),
    }
}

pub(crate) fn allow_image(app: &AppHandle, entry: &EntryUi) {
    if let Some(MediaUi::Image { path, .. }) = &entry.media
        && let Err(error) = app.asset_protocol_scope().allow_file(path)
    {
        warn!(%error, "no se pudo permitir ver la imagen");
    }
}

#[tauri::command]
pub(crate) async fn paste(
    request: Request<'_>,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<EntryUi, UiError> {
    let bytes = binary_body(request.body())?.to_vec();
    let session = Arc::clone(&session);
    let app_handle = app.clone();
    let entry = in_background(move || {
        prepare_executables(&app_handle, &session)?;
        session.paste_bytes(&bytes)
    })
    .await?;
    allow_image(&app, &entry);
    Ok(entry)
}

#[tauri::command]
pub(crate) fn add_links(
    text: String,
    session: State<'_, Arc<Session>>,
) -> Result<Vec<EntryUi>, UiError> {
    session.add_links(&text)
}

#[tauri::command]
pub(crate) async fn download_queue(
    channel: Channel<QueueEvent>,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<(), UiError> {
    let session = Arc::clone(&session);
    in_background(move || {
        prepare_executables(&app, &session)?;
        install_bridge(&app, &session);
        session.download_queue(&|event| {
            if let Err(error) = channel.send(event) {
                warn!(%error, "no se pudo enviar el avance de la cola a la interfaz");
            }
        });
        Ok(())
    })
    .await
}

#[tauri::command]
pub(crate) fn cancel_downloads(session: State<'_, Arc<Session>>) {
    session.cancel_downloads();
}

#[tauri::command]
pub(crate) async fn remove_entry(
    id: u32,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<(), UiError> {
    let session = Arc::clone(&session);
    let pasted = session.pasted_image(id);
    in_background(move || {
        drop(session.remove_entry(id)?);
        Ok(())
    })
    .await?;
    if let Some(path) = pasted
        && let Err(error) = app.asset_protocol_scope().forbid_file(&path)
    {
        warn!(%error, "no se pudo prohibir ver la imagen quitada");
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn preview(
    id: u32,
    from_s: f64,
    session: State<'_, Arc<Session>>,
) -> Result<Response, UiError> {
    let session = Arc::clone(&session);
    in_background(move || session.preview(id, from_s))
        .await
        .map(Response::new)
}

/// Paths of small frames spread over the whole video. Each file is allowed in the asset scope
/// one by one, so nothing else of the temp directory becomes visible.
#[tauri::command]
pub(crate) async fn timeline_thumbnails(
    id: u32,
    count: u32,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<Vec<String>, UiError> {
    let session = Arc::clone(&session);
    let scope_app = app.clone();
    in_background(move || {
        prepare_executables(&app, &session)?;
        let paths = session.timeline_thumbnails(id, count)?;
        paths
            .into_iter()
            .map(|path| {
                scope_app
                    .asset_protocol_scope()
                    .allow_file(&path)
                    .map_err(|error| {
                        UiError::new(
                            UiErrorKind::Other,
                            format!("no se pudo permitir ver las miniaturas: {error}"),
                        )
                    })?;
                Ok(path.to_string_lossy().into_owned())
            })
            .collect()
    })
    .await
}

#[tauri::command]
pub(crate) fn cancel_preview(session: State<'_, Arc<Session>>) {
    session.cancel_preview();
}

#[tauri::command]
pub(crate) async fn create(
    request: CreateRequest,
    channel: Channel<ProgressUi>,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<CreateResult, UiError> {
    let session = Arc::clone(&session);
    in_background(move || {
        prepare_executables(&app, &session)?;
        let report = |stage: ProgressUi| {
            if let Err(error) = channel.send(stage) {
                warn!(%error, "no se pudo enviar el progreso a la interfaz");
            }
        };
        session.create(&request, &report)
    })
    .await
}

#[tauri::command]
pub(crate) fn cancel_creation(session: State<'_, Arc<Session>>) {
    session.cancel_creation();
}

#[tauri::command]
pub(crate) async fn estimate(
    request: CreateRequest,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<EstimateUi, UiError> {
    let session = Arc::clone(&session);
    in_background(move || {
        prepare_executables(&app, &session)?;
        session.estimate(&request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn create_static(
    request: StaticRequest,
    app: AppHandle,
    session: State<'_, Arc<Session>>,
) -> Result<CreateResult, UiError> {
    let session = Arc::clone(&session);
    in_background(move || {
        prepare_executables(&app, &session)?;
        session.create_static(&request)
    })
    .await
}

#[tauri::command]
pub(crate) fn result(id: u32, session: State<'_, Arc<Session>>) -> Result<Response, UiError> {
    session.result(id).map(Response::new)
}

#[tauri::command]
pub(crate) async fn save(
    id: u32,
    app: AppHandle,
    window: Window,
    session: State<'_, Arc<Session>>,
) -> Result<Option<String>, UiError> {
    let session = Arc::clone(&session);
    in_background(move || {
        let proposal = session.proposal_saved(id)?;
        let dialog = app.dialog().file();
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let picked = dialog
            .set_title("Guardar")
            .set_file_name(&proposal.name)
            .add_filter(proposal.extension.to_uppercase(), &[proposal.extension])
            .blocking_save_file();
        let Some(path) = picked else {
            return Ok(None);
        };
        match path {
            FilePath::Path(path) => session.save_to(id, &path).map(Some),
            url => {
                let bytes = session.result(id)?;
                let name = mobile::picked_name(&app, &url);
                mobile::write_to(&app, url, &bytes)?;
                Ok(Some(name))
            }
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_body_instead_of_binary_is_an_error() {
        let json = InvokeBody::Json(serde_json::json!({ "bytes": [1, 2, 3] }));
        let error = binary_body(&json).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);

        let binary = InvokeBody::Raw(vec![1, 2, 3]);
        assert_eq!(binary_body(&binary).unwrap(), &[1, 2, 3]);
    }
}
