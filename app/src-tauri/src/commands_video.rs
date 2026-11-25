//! «Save video»: the video of a downloaded entry, with its audio, kept as a normal file. On Android
//! it goes to `Movies/ConversorStickers` through the bridge; on desktop the person picks the file
//! in a save dialog that opens in the videos folder.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use conversor_core::DownloadProgress;
use tauri::ipc::Channel;
use tauri::{AppHandle, State, Window};
use tracing::warn;

use crate::commands::{in_background, install_bridge};
use crate::contract::{SaveVideoEvent, UiError, UiErrorKind};
use crate::session::{CutToSave, Session};

/// Answers the name it was saved as, or `None` if the person cancelled (the download or the
/// dialog). Progress goes through `channel`.
#[tauri::command]
pub(crate) async fn save_video(
    id: u32,
    channel: Channel<SaveVideoEvent>,
    app: AppHandle,
    window: Window,
    session: State<'_, Arc<Session>>,
) -> Result<Option<String>, UiError> {
    let session = Arc::clone(&session);
    in_background(move || {
        install_bridge(&app, &session);
        let send = |event| {
            if let Err(error) = channel.send(event) {
                warn!(%error, "no se pudo enviar el avance de guardar el video");
            }
        };
        let video = session.video_to_save(id, &|progress| {
            let DownloadProgress::Downloading { bytes, total } = progress;
            send(SaveVideoEvent::Downloading { bytes, total });
        });
        let video = match video {
            Ok(video) => video,
            Err(error) if error.kind == UiErrorKind::Cancelled => return Ok(None),
            Err(error) => return Err(error),
        };
        send(SaveVideoEvent::Writing);
        keep_video(&app, &window, video.path(), video.name())
    })
    .await
}

/// Saves the audio of the clip `[start_s, start_s + seconds)` so it can be picked later as the
/// sound of a sticker. Same answer and progress as [`save_video`].
#[tauri::command]
pub(crate) async fn save_sound(
    id: u32,
    start_s: f64,
    seconds: f64,
    channel: Channel<SaveVideoEvent>,
    app: AppHandle,
    window: Window,
    session: State<'_, Arc<Session>>,
) -> Result<Option<String>, UiError> {
    let session = Arc::clone(&session);
    in_background(move || {
        install_bridge(&app, &session);
        let send = |event| {
            if let Err(error) = channel.send(event) {
                warn!(%error, "no se pudo enviar el avance de guardar el sonido");
            }
        };
        let sound = session.sound_to_save(id, start_s, seconds, &|progress| {
            let DownloadProgress::Downloading { bytes, total } = progress;
            send(SaveVideoEvent::Downloading { bytes, total });
        });
        let sound = match sound {
            Ok(sound) => sound,
            Err(error) if error.kind == UiErrorKind::Cancelled => return Ok(None),
            Err(error) => return Err(error),
        };
        send(SaveVideoEvent::Writing);
        keep_sound(&app, &window, &sound)
    })
    .await
}

/// Saves the clip `[start_s, start_s + seconds)` of the video, with its audio and without
/// re-encoding it, so WhatsApp can make a sticker from it. Same answer and progress as
/// [`save_video`].
#[tauri::command]
pub(crate) async fn save_clip_video(
    id: u32,
    start_s: f64,
    seconds: f64,
    channel: Channel<SaveVideoEvent>,
    app: AppHandle,
    window: Window,
    session: State<'_, Arc<Session>>,
) -> Result<Option<String>, UiError> {
    let session = Arc::clone(&session);
    in_background(move || {
        install_bridge(&app, &session);
        let send = |event| {
            if let Err(error) = channel.send(event) {
                warn!(%error, "no se pudo enviar el avance de guardar el recorte");
            }
        };
        let clip = session.clip_video_to_save(id, start_s, seconds, &|progress| {
            let DownloadProgress::Downloading { bytes, total } = progress;
            send(SaveVideoEvent::Downloading { bytes, total });
        });
        let clip = match clip {
            Ok(clip) => clip,
            Err(error) if error.kind == UiErrorKind::Cancelled => return Ok(None),
            Err(error) => return Err(error),
        };
        send(SaveVideoEvent::Writing);
        keep_video(&app, &window, clip.path(), clip.name())
    })
    .await
}

#[cfg(target_os = "android")]
fn keep_sound(
    app: &AppHandle,
    _window: &Window,
    sound: &CutToSave,
) -> Result<Option<String>, UiError> {
    crate::mobile::save_to_music(app, sound.path(), sound.name()).map(Some)
}

#[cfg(not(target_os = "android"))]
fn keep_sound(
    app: &AppHandle,
    window: &Window,
    sound: &CutToSave,
) -> Result<Option<String>, UiError> {
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app
        .dialog()
        .file()
        .set_parent(window)
        .set_title("Guardar sonido")
        .set_file_name(sound.name());
    if let Ok(music) = app.path().audio_dir() {
        dialog = dialog.set_directory(music);
    }
    if let Some(extension) = sound.path().extension().and_then(|part| part.to_str()) {
        dialog = dialog.add_filter(extension.to_uppercase(), &[extension]);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|error| {
        UiError::new(
            UiErrorKind::Other,
            format!("el diálogo no devolvió una ruta de archivo: {error}"),
        )
    })?;
    copy_video(sound.path(), &path)?;
    Ok(Some(path.file_name().map_or_else(
        || sound.name().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )))
}

#[cfg(target_os = "android")]
fn keep_video(
    app: &AppHandle,
    _window: &Window,
    path: &Path,
    name: &str,
) -> Result<Option<String>, UiError> {
    crate::mobile::save_to_movies(app, path, name).map(Some)
}

#[cfg(not(target_os = "android"))]
fn keep_video(
    app: &AppHandle,
    window: &Window,
    path: &Path,
    name: &str,
) -> Result<Option<String>, UiError> {
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app
        .dialog()
        .file()
        .set_parent(window)
        .set_title("Guardar vídeo")
        .set_file_name(name);
    if let Ok(videos) = app.path().video_dir() {
        dialog = dialog.set_directory(videos);
    }
    if let Some(extension) = path.extension().and_then(|part| part.to_str()) {
        dialog = dialog.add_filter(extension.to_uppercase(), &[extension]);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let destination = picked.into_path().map_err(|error| {
        UiError::new(
            UiErrorKind::Other,
            format!("el diálogo no devolvió una ruta de archivo: {error}"),
        )
    })?;
    copy_video(path, &destination)?;
    Ok(Some(destination.file_name().map_or_else(
        || name.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )))
}

/// Copies through a neighbouring `.part` so a failure never leaves half a video under its name.
pub(crate) fn copy_video(origin: &Path, destination: &Path) -> Result<(), UiError> {
    let mut partial = destination.as_os_str().to_owned();
    partial.push(".part");
    let partial = Path::new(&partial);
    let result = fs::copy(origin, partial).and_then(|_| fs::rename(partial, destination));
    if let Err(error) = result {
        let _ = fs::remove_file(partial);
        return Err(UiError::new(
            UiErrorKind::Other,
            format!("no se pudo guardar el video: {error}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_leaves_the_video_whole_and_no_partial_file() {
        let folder = tempfile::tempdir().unwrap();
        let origin = folder.path().join("origen.mp4");
        fs::write(&origin, b"video con audio").unwrap();
        let destination = folder.path().join("guardado.mp4");

        copy_video(&origin, &destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"video con audio");
        assert!(!folder.path().join("guardado.mp4.part").exists());
    }

    #[test]
    fn a_failed_copy_says_why_and_leaves_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let destination = folder.path().join("guardado.mp4");

        let error = copy_video(&folder.path().join("no-existe.mp4"), &destination).unwrap_err();

        assert_eq!(error.kind, UiErrorKind::Other);
        assert!(error.message.starts_with("no se pudo guardar el video"));
        assert!(!destination.exists());
        assert!(!folder.path().join("guardado.mp4.part").exists());
    }
}
