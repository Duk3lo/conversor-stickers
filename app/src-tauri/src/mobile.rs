//! Android specifics: FFmpeg paths, temp files, file copies and memory. Almost everything is pure
//! logic tested on desktop; what touches the platform is behind `cfg(android)`.

// On desktop only the tests use part of this module.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use conversor_core::{DownloadError, DownloadMode, DownloadProgress, FfmpegDecoder};
use serde::Deserialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Wry};
use tauri_plugin_dialog::FilePath;

use crate::contract::{UiError, UiErrorKind};

/// Prefix of the session temp directories (`session.rs`) and of the import ones (`wastickers.rs`).
const TEMP_PREFIX: &str = "conversor-stickers-";

/// Memory limit of the decoded RGBA clip.
pub(crate) const ANDROID_CLIP_MAX_MEMORY: u64 = 256 * 1024 * 1024;

const NAME_MAX_LENGTH: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Executables {
    pub(crate) ffmpeg: PathBuf,
    pub(crate) ffprobe: PathBuf,
}

impl Default for Executables {
    fn default() -> Self {
        Executables {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
        }
    }
}

impl Executables {
    pub(crate) fn decoder(&self, path: &Path) -> FfmpegDecoder {
        FfmpegDecoder::new(path).with_executables(&self.ffmpeg, &self.ffprobe)
    }

    pub(crate) fn in_native_libraries(directory: &Path) -> Self {
        Executables {
            ffmpeg: directory.join("libffmpeg.so"),
            ffprobe: directory.join("libffprobe.so"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BackupError {
    #[error("el archivo supera el tope de {limit} bytes")]
    ExceedsLimit { limit: u64 },
    #[error("no se pudo copiar el archivo: {0}")]
    Io(#[from] io::Error),
}

/// Copies at most `limit` bytes from `origin` into `destination`. If there are more, or anything
/// fails, no partial file is left.
pub(crate) fn copy_bounded(
    origin: impl Read,
    destination: &Path,
    limit: u64,
) -> Result<u64, BackupError> {
    let result = copy_into(origin, destination, limit);
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn copy_into(origin: impl Read, destination: &Path, limit: u64) -> Result<u64, BackupError> {
    let mut file = File::create(destination)?;
    let copied = io::copy(&mut origin.take(limit.saturating_add(1)), &mut file)?;
    if copied > limit {
        return Err(BackupError::ExceedsLimit { limit });
    }
    Ok(copied)
}

/// Safe file name from `DISPLAY_NAME`, which comes from another app: only `[A-Za-z0-9._-]`, no
/// leading `.` or `-` (hidden file, `..`, command-line option), at most 60 bytes, and `video` if
/// nothing is left.
pub(crate) fn sanitized_name(name: &str) -> String {
    let filtered: String = name
        .chars()
        .filter_map(|char| {
            if char.is_ascii_alphanumeric() || "._-".contains(char) {
                Some(char)
            } else if char.is_whitespace() {
                Some('_')
            } else {
                None
            }
        })
        .collect();
    let without_prefix = filtered.trim_start_matches(['.', '-']);
    let short = &without_prefix[..without_prefix.len().min(NAME_MAX_LENGTH)];
    if short.is_empty() {
        "video".to_string()
    } else {
        short.to_string()
    }
}

pub(crate) fn clean_leftovers(directory: &Path) -> io::Result<usize> {
    let mut deleted = 0;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with(TEMP_PREFIX) {
            continue;
        }
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
        deleted += 1;
    }
    Ok(deleted)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("En el móvil caben como mucho {max_seconds} s con este destino; acorta el tramo")]
pub(crate) struct ClipTooLarge {
    pub(crate) max_seconds: u32,
}

pub(crate) fn check_clip_memory(seconds: f64, fps: u32, side: u32) -> Result<(), ClipTooLarge> {
    let bytes_by_frame = u64::from(side) * u64::from(side) * 4;
    let frames = (seconds * f64::from(fps)).ceil() as u64;
    if frames.saturating_mul(bytes_by_frame) <= ANDROID_CLIP_MAX_MEMORY {
        return Ok(());
    }
    let per_second = bytes_by_frame * u64::from(fps);
    let max_seconds = (ANDROID_CLIP_MAX_MEMORY / per_second.max(1)) as u32;
    Err(ClipTooLarge { max_seconds })
}

#[cfg(target_os = "android")]
const ANDROID_PACKAGE: &str = "es.conversorstickers.app";

#[cfg(target_os = "android")]
struct AndroidBridge(tauri::plugin::PluginHandle<Wry>);

#[derive(Deserialize)]
struct PathsResponse {
    native_libraries: PathBuf,
}

/// Plugin that registers the Kotlin bridge class. It has no commands for the webview, so it asks
/// for no permission. On desktop it does nothing.
pub(crate) fn bridge_plugin() -> TauriPlugin<Wry> {
    Builder::new("android-bridge")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let bridge = _api.register_android_plugin(ANDROID_PACKAGE, "AndroidBridge")?;
                _app.manage(AndroidBridge(bridge));
            }
            Ok(())
        })
        .build()
}

static EXECUTABLES: OnceLock<Executables> = OnceLock::new();

pub(crate) fn executables(app: &AppHandle) -> Result<Executables, UiError> {
    if let Some(executables) = EXECUTABLES.get() {
        return Ok(executables.clone());
    }
    let executables = find_executables(app)?;
    Ok(EXECUTABLES.get_or_init(|| executables).clone())
}

#[cfg(not(target_os = "android"))]
fn find_executables(_app: &AppHandle) -> Result<Executables, UiError> {
    Ok(Executables::default())
}

#[cfg(target_os = "android")]
fn find_executables(app: &AppHandle) -> Result<Executables, UiError> {
    use tauri::Manager;

    let bridge = app.state::<AndroidBridge>();
    let paths: PathsResponse = bridge
        .0
        .run_mobile_plugin("paths", ())
        .map_err(|failure| UiError::new(UiErrorKind::FfmpegNotFound, format!("{failure}")))?;
    let executables = Executables::in_native_libraries(&paths.native_libraries);
    Ok(executables)
}

/// Prepares the Android temp files: `std::env::temp_dir()` points to `/data/local/tmp`, where the
/// app cannot write. Deletes leftovers of a previous run and redirects `tempfile` to the app cache.
pub(crate) fn prepare_temp(app: &AppHandle) {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        let cache = match app.path().app_cache_dir() {
            Ok(cache) => cache,
            Err(failure) => {
                tracing::warn!(%failure, "sin directorio de caché; los temporales pueden fallar");
                return;
            }
        };
        if let Err(failure) = fs::create_dir_all(&cache) {
            tracing::warn!(%failure, cache = %cache.display(), "no se pudo crear la caché");
            return;
        }
        match clean_leftovers(&cache) {
            Ok(0) => {}
            Ok(deleted) => tracing::info!(deleted, "restos de una ejecución anterior borrados"),
            Err(failure) => tracing::warn!(%failure, "no se pudieron borrar restos de la caché"),
        }
        if let Err(busy) = tempfile::env::override_temp_dir(&cache) {
            tracing::warn!(already_set = %busy.display(), "el directorio temporal ya estaba fijado");
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

pub(crate) fn check_executables_on_startup(app: &AppHandle) {
    #[cfg(target_os = "android")]
    {
        let app = app.clone();
        std::thread::spawn(move || {
            if let Err(failure) = executables(&app) {
                tracing::warn!(%failure, "el puente no dio las rutas de FFmpeg");
            }
        });
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

#[cfg(target_os = "android")]
/// Cache directory where picked files are copied. It starts with the temp prefix so
/// `clean_leftovers` deletes it.
const PICKED_DIRECTORY: &str = "conversor-stickers-picked";

pub(crate) fn local_path(
    app: &AppHandle,
    picked: FilePath,
    limit: u64,
) -> Result<PathBuf, UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, limit);
        picked.into_path().map_err(|error| {
            UiError::new(
                UiErrorKind::InvalidEntry,
                format!("el diálogo no devolvió una ruta de archivo: {error}"),
            )
        })
    }
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::{AtomicU32, Ordering};

        use tauri::Manager;
        use tauri_plugin_fs::{FsExt, OpenOptions};

        static NEXT: AtomicU32 = AtomicU32::new(0);

        let name = match &picked {
            FilePath::Url(url) => app.path().file_name(url.as_str()),
            FilePath::Path(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
        }
        .unwrap_or_default();
        let origin = app
            .fs()
            .open(picked, OpenOptions::new())
            .map_err(|failure| file_error(format!("no se pudo abrir el archivo: {failure}")))?;
        let folder = app
            .path()
            .app_cache_dir()
            .map_err(|failure| file_error(format!("sin directorio de caché: {failure}")))?
            .join(PICKED_DIRECTORY)
            .join(NEXT.fetch_add(1, Ordering::Relaxed).to_string());
        fs::create_dir_all(&folder)
            .map_err(|failure| file_error(format!("no se pudo preparar la copia: {failure}")))?;
        let destination = folder.join(sanitized_name(&name));
        match copy_bounded(origin, &destination, limit) {
            Ok(_) => Ok(destination),
            Err(error) => {
                let _ = fs::remove_dir(&folder);
                Err(file_error(error.to_string()))
            }
        }
    }
}

pub(crate) fn picked_name(app: &AppHandle, picked: &FilePath) -> String {
    use tauri::Manager;

    let name = match picked {
        FilePath::Url(url) => app.path().file_name(url.as_str()),
        FilePath::Path(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
    };
    name.unwrap_or_else(|| "archivo".to_string())
}

/// Writes `bytes` to what the user picked. Atomic on desktop; on Android it cannot be (there is no
/// `rename` over `content://`), so the URI is opened in `wt` mode.
pub(crate) fn write_to(app: &AppHandle, picked: FilePath, bytes: &[u8]) -> Result<(), UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        let path = picked.into_path().map_err(|error| {
            file_error(format!(
                "el diálogo no devolvió una ruta de archivo: {error}"
            ))
        })?;
        crate::storage::write_atomic(&path, bytes)
            .map_err(|failure| file_error(format!("no se pudo guardar: {failure}")))
    }
    #[cfg(target_os = "android")]
    {
        use std::io::Write;

        use tauri_plugin_fs::{FsExt, OpenOptions};

        let mut options = OpenOptions::new();
        options.read(false).write(true).truncate(true);
        let mut destination = app
            .fs()
            .open(picked, options)
            .map_err(|failure| file_error(format!("no se pudo abrir el destino: {failure}")))?;
        destination
            .write_all(bytes)
            .and_then(|()| destination.sync_all())
            .map_err(|failure| file_error(format!("no se pudo guardar: {failure}")))
    }
}

fn file_error(message: String) -> UiError {
    UiError::new(UiErrorKind::InvalidEntry, message)
}

/// What the bridge says after launching the WhatsApp intent (`AndroidBridge.kt`): `result` is
/// `added`, `cancelled`, `not_installed` or `validation_error` with its `reason`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct AddResponse {
    pub(crate) result: String,
    #[serde(default)]
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ClipboardContent {
    pub(crate) uri: Option<String>,
    pub(crate) mime: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Shared {
    pub(crate) links: Vec<String>,
    pub(crate) media: Vec<SharedMedia>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct SharedMedia {
    pub(crate) uri: String,
    pub(crate) mime: Option<String>,
}

/// How the `yt-dlp` download made by the bridge is going; it is the JSON of
/// `AndroidDownloads.status`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub(crate) enum DownloadPhase {
    Idle,
    InProgress { bytes: u64, total: Option<u64> },
    Ready { path: PathBuf },
    Cancelled,
    Error { message: String },
}

const POLL_INTERVAL: Duration = Duration::from_millis(250);

pub(crate) fn wait_for_download(
    mut poll: impl FnMut() -> Result<DownloadPhase, DownloadError>,
    mut cancel: impl FnMut(),
    progress: &dyn Fn(DownloadProgress),
    cancellation: &AtomicBool,
    limit: Duration,
    interval: Duration,
) -> Result<PathBuf, DownloadError> {
    let start = Instant::now();
    let mut cancelled = false;
    let mut timed_out = false;
    loop {
        match poll()? {
            DownloadPhase::Ready { path } => return Ok(path),
            DownloadPhase::Cancelled if timed_out => {
                return Err(DownloadError::TimedOut { limit });
            }
            DownloadPhase::Cancelled => return Err(DownloadError::Cancelled),
            DownloadPhase::Error { message } => {
                return Err(DownloadError::FailedOwnDownload {
                    reason: format!("Este sitio no se pudo descargar: {message}"),
                });
            }
            DownloadPhase::Idle => {
                return Err(DownloadError::FailedOwnDownload {
                    reason: "la descarga no está en marcha".to_string(),
                });
            }
            DownloadPhase::InProgress { bytes, total } => {
                progress(DownloadProgress::Downloading { bytes, total });
            }
        }
        if !cancelled {
            timed_out = start.elapsed() >= limit;
            if timed_out || cancellation.load(Ordering::Relaxed) {
                cancelled = true;
                cancel();
            }
        }
        std::thread::sleep(interval);
    }
}

pub(crate) fn download_with_bridge(
    app: &AppHandle,
    link: &str,
    mode: DownloadMode,
    destination: &Path,
    progress: &dyn Fn(DownloadProgress),
    cancellation: &AtomicBool,
    limit: Duration,
) -> Result<PathBuf, DownloadError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, link, mode, destination, progress, cancellation, limit);
        Err(DownloadError::FailedOwnDownload {
            reason: "solo está disponible en Android".to_string(),
        })
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        #[derive(serde::Serialize)]
        struct Request<'a> {
            link: &'a str,
            destination: &'a Path,
            mode: &'a str,
        }
        let mode = match mode {
            DownloadMode::Sticker => "sticker",
            DownloadMode::Save => "save",
        };
        let bridge = &app.state::<AndroidBridge>().0;
        let failure = |reason: String| DownloadError::FailedOwnDownload { reason };
        bridge
            .run_mobile_plugin::<()>(
                "start_download",
                Request {
                    link,
                    destination,
                    mode,
                },
            )
            .map_err(|error| failure(format!("no se pudo iniciar la descarga: {error}")))?;
        wait_for_download(
            || {
                bridge
                    .run_mobile_plugin::<DownloadPhase>("download_status", ())
                    .map_err(|error| failure(format!("no se pudo consultar la descarga: {error}")))
            },
            || {
                if let Err(error) = bridge.run_mobile_plugin::<()>("cancel_download", ()) {
                    tracing::warn!(%error, "no se pudo cancelar la descarga");
                }
            },
            progress,
            cancellation,
            limit,
            POLL_INTERVAL,
        )
    }
}

/// Cache folder whose files the FileProvider may hand to other apps. It starts with the temp
/// prefix so `clean_leftovers` empties it on startup; `file_paths.xml` names it too.
const SHARE_DIRECTORY: &str = "conversor-stickers-share";

pub(crate) fn share_directory(cache: &Path) -> PathBuf {
    cache.join(SHARE_DIRECTORY)
}

/// Puts `bytes` in `directory` as a new `sticker-<n>.webp` and deletes the copies shared before,
/// so at most one stays on disk.
pub(crate) fn stage_for_share(directory: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let mut next = 0u64;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if let Some(number) = shared_number(&entry.file_name().to_string_lossy()) {
            next = next.max(number + 1);
        }
        fs::remove_file(entry.path())?;
    }
    let path = directory.join(format!("sticker-{next}.webp"));
    fs::write(&path, bytes)?;
    Ok(path)
}

fn shared_number(name: &str) -> Option<u64> {
    name.strip_prefix("sticker-")?
        .strip_suffix(".webp")?
        .parse()
        .ok()
}

pub(crate) fn share_file(app: &AppHandle, path: &Path) -> Result<(), UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, path);
        Err(android_only())
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        #[derive(serde::Serialize)]
        struct Arguments<'a> {
            path: &'a Path,
        }
        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin::<()>("share_file", Arguments { path })
            .map_err(|failure| {
                UiError::new(
                    UiErrorKind::Other,
                    format!("no se pudo abrir el menú de compartir: {failure}"),
                )
            })
    }
}

/// The folders the app saves into that can be opened: `videos` and `sounds`. Checked here and
/// again in Kotlin; the name goes to the platform only if it is one of these.
pub(crate) fn saved_folder_kind(kind: &str) -> Result<&'static str, UiError> {
    match kind {
        "videos" => Ok("videos"),
        "sounds" => Ok("sounds"),
        _ => Err(UiError::new(
            UiErrorKind::InvalidEntry,
            "carpeta no válida: solo vídeos o sonidos",
        )),
    }
}

pub(crate) fn open_saved_folder(app: &AppHandle, kind: &'static str) -> Result<(), UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, kind);
        Err(android_only())
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        #[derive(serde::Serialize)]
        struct Arguments {
            kind: &'static str,
        }
        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin::<()>("open_saved_folder", Arguments { kind })
            .map_err(|failure| UiError::new(UiErrorKind::Other, failure.to_string()))
    }
}

/// Brings WhatsApp to the front so the user can pick the saved video in its stickers.
pub(crate) fn open_whatsapp(app: &AppHandle) -> Result<(), UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(android_only())
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin::<()>("open_whatsapp", ())
            .map_err(|failure| UiError::new(UiErrorKind::Other, failure.to_string()))
    }
}

/// Opens a web address in the system browser. Rust has already checked it.
#[cfg(target_os = "android")]
pub(crate) fn open_url(app: &AppHandle, url: &str) -> Result<(), UiError> {
    use tauri::Manager;

    #[derive(serde::Serialize)]
    struct Arguments<'a> {
        url: &'a str,
    }
    app.state::<AndroidBridge>()
        .0
        .run_mobile_plugin::<()>("open_url", Arguments { url })
        // The failure text could repeat the address, which may carry a phone number.
        .map_err(|_| {
            UiError::new(
                UiErrorKind::Other,
                "no se pudo abrir el navegador del sistema",
            )
        })
}

#[cfg(edition = "github")]
const BOT_NUMBER_DIGITS: std::ops::RangeInclusive<usize> = 8..=15;
#[cfg(edition = "github")]
const BOT_CAPTION_MAX_CHARS: usize = 80;

#[cfg(edition = "github")]
/// The bot's number: 8 to 15 digits. Checked here and again in Kotlin, which builds the chat id.
pub(crate) fn bot_number(number: &str) -> Result<&str, UiError> {
    phone_digits(
        number,
        "el número del bot no es válido: solo dígitos, de 8 a 15",
    )
}

#[cfg(edition = "github")]
/// The number of the WhatsApp to link by code: same shape as the bot's, its own message.
pub(crate) fn link_number(number: &str) -> Result<&str, UiError> {
    phone_digits(
        number,
        "el número a vincular no es válido: solo dígitos, de 8 a 15, con código de país",
    )
}

#[cfg(edition = "github")]
fn phone_digits<'a>(number: &'a str, message: &'static str) -> Result<&'a str, UiError> {
    if BOT_NUMBER_DIGITS.contains(&number.len()) && number.bytes().all(|byte| byte.is_ascii_digit())
    {
        Ok(number)
    } else {
        Err(UiError::new(UiErrorKind::InvalidEntry, message))
    }
}

#[cfg(edition = "github")]
/// The settings caption (`inicio=2 fin=7 sonido=5`): short, with only the characters it needs.
pub(crate) fn bot_caption(caption: &str) -> Result<&str, UiError> {
    let allowed = |character: char| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '=' | ' ' | '.')
    };
    if caption.chars().count() <= BOT_CAPTION_MAX_CHARS && caption.chars().all(allowed) {
        Ok(caption)
    } else {
        Err(UiError::new(
            UiErrorKind::InvalidEntry,
            "el texto de ajustes del bot no es válido",
        ))
    }
}

#[cfg(edition = "github")]
/// Shares the last saved clip to the bot's chat in WhatsApp.
pub(crate) fn send_to_bot(
    app: &AppHandle,
    number: &str,
    caption: Option<&str>,
) -> Result<(), UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, number, caption);
        Err(android_only())
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        #[derive(serde::Serialize)]
        struct Arguments<'a> {
            number: &'a str,
            caption: Option<&'a str>,
        }
        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin::<()>("send_to_bot", Arguments { number, caption })
            .map_err(|failure| UiError::new(UiErrorKind::Other, failure.to_string()))
    }
}

pub(crate) fn android_only() -> UiError {
    UiError::new(UiErrorKind::Other, "solo está disponible en Android")
}

pub(crate) fn add_whatsapp_pack(
    app: &AppHandle,
    id: &str,
    name: &str,
    open_after: bool,
) -> Result<AddResponse, UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, id, name, open_after);
        Err(android_only())
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        #[derive(serde::Serialize)]
        struct Arguments<'a> {
            id: &'a str,
            name: &'a str,
            open_after: bool,
        }
        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin(
                "add_whatsapp_pack",
                Arguments {
                    id,
                    name,
                    open_after,
                },
            )
            .map_err(|failure| {
                UiError::new(
                    UiErrorKind::Other,
                    format!("no se pudo abrir WhatsApp: {failure}"),
                )
            })
    }
}

pub(crate) fn take_shared(app: &AppHandle) -> Result<Shared, UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(android_only())
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin("take_shared", ())
            .map_err(|failure| {
                UiError::new(
                    UiErrorKind::Other,
                    format!("no se pudo leer lo compartido: {failure}"),
                )
            })
    }
}

/// Android: the `conversorstickers://bot#...` link another app opened, delivered only once. It is
/// raw text; `ConnectionLink::parse` is the one that checks it.
#[cfg(edition = "github")]
pub(crate) fn take_bot_link(app: &AppHandle) -> Result<Option<String>, UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        #[derive(serde::Deserialize)]
        struct Opened {
            link: Option<String>,
        }
        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin::<Opened>("take_bot_link", ())
            .map(|opened| opened.link)
            .map_err(|_| {
                UiError::new(
                    UiErrorKind::Other,
                    "no se pudo leer el enlace de conexión del bot",
                )
            })
    }
}

pub(crate) fn read_clipboard(app: &AppHandle) -> Result<ClipboardContent, UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(android_only())
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin("read_clipboard", ())
            .map_err(|failure| {
                UiError::new(
                    UiErrorKind::Other,
                    format!("no se pudo leer el portapapeles: {failure}"),
                )
            })
    }
}

#[derive(Deserialize)]
struct SavedInMovies {
    name: String,
}

/// Copies the video to `Movies/ConversorStickers` through MediaStore and returns the name it got
/// (Android renames it if that one already exists).
pub(crate) fn save_to_movies(app: &AppHandle, video: &Path, name: &str) -> Result<String, UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, video, name);
        Err(android_only())
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        #[derive(serde::Serialize)]
        struct Request<'a> {
            path: &'a Path,
            name: &'a str,
        }
        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin::<SavedInMovies>("save_to_movies", Request { path: video, name })
            .map(|saved| saved.name)
            .map_err(|failure| {
                UiError::new(
                    UiErrorKind::Other,
                    format!("no se pudo guardar el video: {failure}"),
                )
            })
    }
}

/// Copies the sound to `Music/ConversorStickers` through MediaStore and returns the name it got.
pub(crate) fn save_to_music(app: &AppHandle, sound: &Path, name: &str) -> Result<String, UiError> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, sound, name);
        Err(android_only())
    }
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;

        #[derive(serde::Serialize)]
        struct Request<'a> {
            path: &'a Path,
            name: &'a str,
        }
        app.state::<AndroidBridge>()
            .0
            .run_mobile_plugin::<SavedInMovies>("save_to_music", Request { path: sound, name })
            .map(|saved| saved.name)
            .map_err(|failure| {
                UiError::new(
                    UiErrorKind::Other,
                    format!("no se pudo guardar el sonido: {failure}"),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::io::Cursor;

    use super::*;

    #[cfg(edition = "github")]
    #[test]
    fn each_number_has_its_own_message() {
        let bot = bot_number("1").unwrap_err().message;
        let link = link_number("1").unwrap_err().message;
        assert!(bot.contains("número del bot"), "{bot}");
        assert!(link.contains("número a vincular"), "{link}");
        assert!(link_number("34612345678").is_ok());
    }

    #[cfg(edition = "github")]
    #[test]
    fn bot_number_is_eight_to_fifteen_digits() {
        assert!(bot_number("34612345678").is_ok());
        assert!(bot_number("12345678").is_ok());
        assert!(bot_number("123456789012345").is_ok());
        for bad in [
            "",
            "1234567",
            "1234567890123456",
            "+34612345678",
            "3461234567a",
            "٣٤٦١٢٣٤٥٦٧٨",
        ] {
            assert_eq!(
                bot_number(bad).unwrap_err().kind,
                UiErrorKind::InvalidEntry,
                "{bad}"
            );
        }
    }

    #[cfg(edition = "github")]
    #[test]
    fn bot_caption_accepts_only_the_settings_format() {
        assert!(bot_caption("inicio=2 fin=7.5 sonido=5").is_ok());
        assert!(bot_caption("").is_ok());
        for bad in [
            "inicio=2\nfin=7",
            "Inicio=2",
            "inicio=2;rm",
            &"a".repeat(81),
        ] {
            assert_eq!(
                bot_caption(bad).unwrap_err().kind,
                UiErrorKind::InvalidEntry,
                "{bad}"
            );
        }
    }

    #[test]
    fn only_videos_and_sounds_folders_can_be_opened() {
        assert_eq!(saved_folder_kind("videos").unwrap(), "videos");
        assert_eq!(saved_folder_kind("sounds").unwrap(), "sounds");
        for kind in ["", "stickers", "../Download", "Videos", "primary:Movies"] {
            assert!(saved_folder_kind(kind).is_err(), "{kind}");
        }
    }

    #[test]
    fn only_the_latest_shared_copy_stays() {
        let directory = tempfile::tempdir().unwrap();
        let folder = share_directory(directory.path());
        let first = stage_for_share(&folder, b"uno").unwrap();
        let second = stage_for_share(&folder, b"dos").unwrap();
        assert_ne!(first, second);
        assert!(!first.exists());
        assert_eq!(fs::read(&second).unwrap(), b"dos");
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
        assert_eq!(second.parent().unwrap(), folder);
        assert!(shared_number("sticker-7.webp") == Some(7));
        assert!(shared_number("otro.webp").is_none());
    }

    fn in_progress(bytes: u64) -> DownloadPhase {
        DownloadPhase::InProgress {
            bytes,
            total: Some(100),
        }
    }

    fn wait(
        phases: Vec<DownloadPhase>,
        cancellation: &AtomicBool,
        limit: Duration,
        progress_updates: &RefCell<Vec<u64>>,
        cancellations: &Cell<u32>,
    ) -> Result<PathBuf, DownloadError> {
        let mut phases = phases.into_iter();
        wait_for_download(
            || Ok(phases.next().expect("la prueba da fases de sobra")),
            || cancellations.set(cancellations.get() + 1),
            &|DownloadProgress::Downloading { bytes, .. }| {
                progress_updates.borrow_mut().push(bytes)
            },
            cancellation,
            limit,
            Duration::ZERO,
        )
    }

    #[test]
    fn bridge_phase_is_read() {
        let read: DownloadPhase =
            serde_json::from_str(r#"{"bytes":7,"total":null,"phase":"in_progress"}"#).unwrap();
        assert_eq!(
            read,
            DownloadPhase::InProgress {
                bytes: 7,
                total: None
            }
        );
        let list: DownloadPhase =
            serde_json::from_str(r#"{"result":"ready","path":"/c/v.mp4","phase":"ready"}"#)
                .unwrap();
        assert_eq!(
            list,
            DownloadPhase::Ready {
                path: "/c/v.mp4".into()
            }
        );
    }

    #[test]
    fn wait_reports_progress_and_returns_path() {
        let progress_updates = RefCell::new(Vec::new());
        let cancellations = Cell::new(0);
        let path = wait(
            vec![
                in_progress(10),
                in_progress(60),
                DownloadPhase::Ready {
                    path: "/c/v.mp4".into(),
                },
            ],
            &AtomicBool::new(false),
            Duration::from_secs(60),
            &progress_updates,
            &cancellations,
        )
        .unwrap();
        assert_eq!(path, PathBuf::from("/c/v.mp4"));
        assert_eq!(*progress_updates.borrow(), [10, 60]);
        assert_eq!(cancellations.get(), 0);
    }

    #[test]
    fn wait_cancels_once_and_waits_for_confirmation() {
        let progress_updates = RefCell::new(Vec::new());
        let cancellations = Cell::new(0);
        let error = wait(
            vec![
                in_progress(10),
                in_progress(20),
                in_progress(30),
                DownloadPhase::Cancelled,
            ],
            &AtomicBool::new(true),
            Duration::from_secs(60),
            &progress_updates,
            &cancellations,
        )
        .unwrap_err();
        assert!(matches!(error, DownloadError::Cancelled));
        assert_eq!(cancellations.get(), 1);
    }

    #[test]
    fn wait_past_limit_cancels_and_times_out() {
        let progress_updates = RefCell::new(Vec::new());
        let cancellations = Cell::new(0);
        let error = wait(
            vec![in_progress(10), in_progress(20), DownloadPhase::Cancelled],
            &AtomicBool::new(false),
            Duration::ZERO,
            &progress_updates,
            &cancellations,
        )
        .unwrap_err();
        assert!(matches!(error, DownloadError::TimedOut { .. }));
        assert_eq!(cancellations.get(), 1);
    }

    #[test]
    fn wait_translates_yt_dlp_error_and_idle_state() {
        let progress_updates = RefCell::new(Vec::new());
        let cancellations = Cell::new(0);
        let error = wait(
            vec![DownloadPhase::Error {
                message: "Unsupported URL".into(),
            }],
            &AtomicBool::new(false),
            Duration::from_secs(60),
            &progress_updates,
            &cancellations,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Este sitio no se pudo descargar: Unsupported URL"
        );
        let idle = wait(
            vec![DownloadPhase::Idle],
            &AtomicBool::new(false),
            Duration::from_secs(60),
            &progress_updates,
            &cancellations,
        );
        assert!(matches!(idle, Err(DownloadError::FailedOwnDownload { .. })));
    }

    #[test]
    fn bounded_copy_accepts_limit_and_rejects_one_more() {
        let folder = tempfile::tempdir().unwrap();
        let destination = folder.path().join("v.mp4");

        let copied = copy_bounded(Cursor::new(vec![7u8; 10]), &destination, 10).unwrap();
        assert_eq!(copied, 10);
        assert_eq!(fs::metadata(&destination).unwrap().len(), 10);

        let other = folder.path().join("w.mp4");
        let error = copy_bounded(Cursor::new(vec![7u8; 11]), &other, 10).unwrap_err();
        assert!(matches!(error, BackupError::ExceedsLimit { limit: 10 }));
        assert!(!other.exists(), "no debe quedar el parcial");
    }

    #[test]
    fn copy_failing_midway_leaves_no_partial() {
        struct Broken(bool);
        impl Read for Broken {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    return Err(io::Error::other("se cortó"));
                }
                self.0 = true;
                buf[0] = 1;
                Ok(1)
            }
        }
        let folder = tempfile::tempdir().unwrap();
        let destination = folder.path().join("v.mp4");
        let error = copy_bounded(Broken(false), &destination, 10).unwrap_err();
        assert!(matches!(error, BackupError::Io(_)));
        assert!(!destination.exists());
    }

    #[test]
    fn other_app_name_is_sanitized() {
        for (entry, expected) in [
            ("../x", "x"),
            ("", "video"),
            ("-a", "a"),
            ("..", "video"),
            ("Canción de Ñandú.mp4", "Cancin_de_and.mp4"),
            ("mi video.mp4", "mi_video.mp4"),
        ] {
            assert_eq!(sanitized_name(entry), expected, "{entry:?}");
        }
        assert_eq!(sanitized_name(&"a".repeat(200)).len(), NAME_MAX_LENGTH);
    }

    #[test]
    fn executables_come_from_the_native_directory() {
        let executables = Executables::in_native_libraries(Path::new("/data/app/x/lib/x86_64"));
        assert_eq!(
            executables.ffmpeg,
            Path::new("/data/app/x/lib/x86_64/libffmpeg.so")
        );
        assert_eq!(
            executables.ffprobe,
            Path::new("/data/app/x/lib/x86_64/libffprobe.so")
        );
        assert_eq!(Executables::default().ffmpeg, Path::new("ffmpeg"));
        assert_eq!(Executables::default().ffprobe, Path::new("ffprobe"));
    }

    #[test]
    fn whatsapp_10_s_fits_and_discord_30_s_does_not() {
        assert_eq!(check_clip_memory(10.0, 20, 512), Ok(()));
        let error = check_clip_memory(30.0, 15, 480).unwrap_err();
        assert_eq!(error.max_seconds, 19);
        assert_eq!(
            error.to_string(),
            "En el móvil caben como mucho 19 s con este destino; acorta el tramo"
        );
    }

    #[test]
    fn cleanup_deletes_only_app_files() {
        let folder = tempfile::tempdir().unwrap();
        fs::create_dir(folder.path().join("conversor-stickers-abc")).unwrap();
        fs::write(folder.path().join("conversor-stickers-abc/v.mp4"), b"x").unwrap();
        fs::create_dir(folder.path().join("conversor-stickers-importar-1")).unwrap();
        fs::create_dir(folder.path().join("otra-cosa")).unwrap();
        fs::write(folder.path().join("elegidos.txt"), b"x").unwrap();

        assert_eq!(clean_leftovers(folder.path()).unwrap(), 2);
        assert!(!folder.path().join("conversor-stickers-abc").exists());
        assert!(folder.path().join("otra-cosa").exists());
        assert!(folder.path().join("elegidos.txt").exists());
    }
}
