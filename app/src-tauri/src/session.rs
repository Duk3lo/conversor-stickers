//! App state and command logic, without Tauri so it can be tested directly: the video queue, the
//! conversion in progress and the last file created for each video.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use conversor_core::{
    Align, Background, Canvas, ConversionError, ConversionOptions, ConversionParams, Decoder as _,
    DecodingError, DownloadedVideo, Fps, FramingMode, FramingSpec, ImageInfo, Link, OptionsError,
    PRESETS, Preset, Priority, Progress, Shape, Speed, TimePlan, Tracking, VideoInfo,
    WHATSAPP_STATIC_STICKER, YtDlpDownloader, convert, convert_image, decimal, decimal_1,
    find_preset, plan_time, readable_size,
};

use tempfile::TempDir;
use tracing::warn;

use crate::contract::{
    AlignUi, BackgroundUi, CreateRequest, CreateResult, EntryState, EntryUi, EstimateUi,
    FpsRangeUi, FramingModeUi, FramingSpecUi, MediaKind, MediaOrigin, MediaUi, Origin, PresetUi,
    PriorityUi, ProgressUi, ShapeUi, SideRangeUi, StaticRequest, UiError, UiErrorKind,
};
use crate::mobile::{self, Executables};
use crate::storage::write_atomic;

// Link downloader (yt-dlp): its entry points exist only in the github edition.
#[cfg_attr(not(edition = "github"), allow(dead_code))]
mod downloads;
#[cfg(test)]
mod end_to_end;
mod preview;
mod thumbnails;

use downloads::BridgeDownload;
pub(crate) use downloads::CutToSave;
use preview::Previewer;

const CLOSE_DEADLINE: Duration = Duration::from_secs(5);

pub(crate) fn presets() -> Vec<PresetUi> {
    PRESETS
        .iter()
        .map(|preset| PresetUi {
            name: preset.name.to_string(),
            title: preset
                .description
                .split_once(':')
                .map_or(preset.name, |(title, _)| title)
                .to_string(),
            extension: preset.format.extension().to_string(),
            side: preset.canvas.side(),
            max_size_bytes: preset.max_size.bytes(),
            max_duration_s: preset.max_duration_s,
            max_fps: preset.max_fps(),
            fps_range: FpsRangeUi {
                min: Fps::MIN,
                max: preset.max_fps(),
            },
            side_range_px: side_range(preset),
        })
        .collect()
}

/// Smallest side a destination with a free canvas accepts.
const MIN_SIDE_PX: u32 = 64;

const FPS_NOT_SUPPORTED: &str = "Esos fps no se admiten en este destino";

const SIDE_NOT_SUPPORTED: &str = "Ese tamaño no se admite en este destino";

fn side_range(preset: &Preset) -> Option<SideRangeUi> {
    let Canvas::Max(max) = preset.canvas else {
        return None;
    };
    Some(SideRangeUi {
        min: MIN_SIDE_PX,
        max,
        suggested: vec![max / 2, max * 2 / 3, max],
    })
}

struct Created {
    bytes: Vec<u8>,
    extension: &'static str,
}

struct Video {
    path: PathBuf,
    info: VideoInfo,
}

struct Image {
    path: PathBuf,
    info: ImageInfo,
}

struct Entry {
    id: u32,
    name: String,
    name_base: String,
    origin: Origin,
    state: EntryState,
    #[cfg_attr(not(edition = "github"), allow(dead_code))]
    link: Option<Link>,
    /// Where it came from, kept after the download starts so "save video" can download it again
    /// with audio.
    source: Option<Link>,
    downloaded: Option<DownloadedVideo>,
    video: Option<Video>,
    image: Option<Image>,
    pasted: Option<PathBuf>,
    created: Option<Created>,
    media_origin: MediaOrigin,
}

enum Media {
    Video(Video),
    Image(Image),
}

impl Entry {
    fn set_media(&mut self, media: Result<Media, DecodingError>) {
        match media {
            Ok(Media::Video(video)) => self.video = Some(video),
            Ok(Media::Image(image)) => self.image = Some(image),
            Err(error) => {
                self.state = EntryState::Error {
                    error: UiError::from(ConversionError::from(error)),
                };
            }
        }
    }

    fn ui(&self) -> EntryUi {
        EntryUi {
            id: self.id,
            name: self.name.clone(),
            origin: self.origin,
            state: self.state.clone(),
            media: match (&self.video, &self.image) {
                (Some(video), _) => Some(MediaUi::Video {
                    duration_s: video.info.duration_s,
                    width: video.info.width,
                    height: video.info.height,
                    has_audio: video.info.has_audio,
                }),
                (None, Some(image)) => Some(MediaUi::Image {
                    path: image.path.to_string_lossy().into_owned(),
                    width: image.info.width,
                    height: image.info.height,
                }),
                (None, None) => None,
            },
        }
    }
}

#[derive(Default)]
struct State {
    next_id: u32,
    entries: Vec<Entry>,
    closing: bool,
    downloading: bool,
    converting: Option<u32>,
    estimate: Option<Arc<AtomicBool>>,
    estimating: usize,
    preview: Option<(u32, Arc<AtomicBool>)>,
    previewing: usize,
    deleting: usize,
    temp: Option<TempDir>,
}

impl State {
    fn entry(&self, id: u32) -> Result<&Entry, UiError> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| missing_entry(id))
    }

    fn entry_mut(&mut self, id: u32) -> Result<&mut Entry, UiError> {
        self.entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or_else(|| missing_entry(id))
    }

    fn temp(&mut self) -> io::Result<&Path> {
        let temp = match self.temp.take() {
            Some(temp) => temp,
            None => tempfile::Builder::new()
                .prefix("conversor-stickers-")
                .tempdir()?,
        };
        Ok(self.temp.insert(temp).path())
    }

    fn cancel_preview(&self) {
        if let Some((_, cancellation)) = &self.preview {
            cancellation.store(true, Ordering::Relaxed);
        }
    }

    fn cancel_estimate(&self) {
        if let Some(cancellation) = &self.estimate {
            cancellation.store(true, Ordering::Relaxed);
        }
    }

    fn new_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }
}

fn missing_entry(id: u32) -> UiError {
    UiError::new(
        UiErrorKind::InvalidEntry,
        format!("no hay ningún video con el número {id} en la cola"),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProposalSaved {
    pub(crate) name: String,
    pub(crate) extension: &'static str,
}

#[derive(Default)]
pub(crate) struct Session {
    state: Mutex<State>,
    executables: OnceLock<Executables>,
    limit_memory: bool,
    creation_cancellation: AtomicBool,
    inspection_cancellation: AtomicBool,
    downloads_cancellation: AtomicBool,
    stopped_jobs: Condvar,
    downloader: YtDlpDownloader,
    bridge_download: OnceLock<BridgeDownload>,
    previewer: Previewer,
}

impl Session {
    pub(crate) fn new() -> Self {
        Session {
            limit_memory: cfg!(target_os = "android"),
            ..Session::default()
        }
    }

    pub(crate) fn set_executables(&self, executables: Executables) {
        let _ = self.executables.set(executables);
    }

    // Only Android and the tests set it.
    #[cfg_attr(not(any(test, target_os = "android")), allow(dead_code))]
    pub(crate) fn set_bridge_download(&self, download: BridgeDownload) {
        let _ = self.bridge_download.set(download);
    }

    pub(crate) fn executables(&self) -> Executables {
        self.executables.get().cloned().unwrap_or_default()
    }

    fn check_memory(&self, request: &CreateRequest, preset: &Preset) -> Result<(), UiError> {
        if !self.limit_memory {
            return Ok(());
        }
        let priority = priority_of(request.priority);
        let output_s = (request.seconds / request.speed).min(preset.max_duration_s);
        mobile::check_clip_memory(
            output_s,
            request.fps.unwrap_or(priority.default_fps().value()),
            preset.canvas.side(),
        )
        .map_err(|error| UiError::new(UiErrorKind::InvalidOptions, error.to_string()))
    }

    pub(crate) fn local_path(&self, id: u32) -> Result<PathBuf, UiError> {
        let state = self.state();
        let entry = state.entry(id)?;
        match (&entry.video, &entry.image) {
            (Some(video), _) => Ok(video.path.clone()),
            (_, Some(image)) => Ok(image.path.clone()),
            _ => Err(missing_entry(id)),
        }
    }

    #[cfg(test)]
    fn with_downloader(downloader: YtDlpDownloader) -> Self {
        Session {
            downloader,
            ..Session::default()
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn add_files(&self, paths: Vec<PathBuf>) -> Vec<EntryUi> {
        self.add_paths(paths, MediaKind::Video)
    }

    pub(crate) fn add_images(&self, paths: Vec<PathBuf>) -> Vec<EntryUi> {
        self.add_paths(paths, MediaKind::Image)
    }

    fn add_paths(&self, paths: Vec<PathBuf>, kind: MediaKind) -> Vec<EntryUi> {
        paths
            .into_iter()
            .map(|path| {
                let (name, name_base) = match kind {
                    MediaKind::Video => (
                        text_of(path.file_name(), "video"),
                        text_of(path.file_stem(), "video"),
                    ),
                    MediaKind::Image => (
                        text_of(path.file_name(), "imagen"),
                        text_of(path.file_stem(), "imagen"),
                    ),
                };
                let result = self.inspect_media(&path, kind);
                let mut session_state = self.state();
                let mut entry = Entry {
                    id: session_state.new_id(),
                    name,
                    name_base,
                    origin: Origin::File,
                    state: EntryState::Ready,
                    link: None,
                    source: None,
                    downloaded: None,
                    video: None,
                    image: None,
                    pasted: None,
                    created: None,
                    media_origin: MediaOrigin::Own,
                };
                entry.set_media(result);
                let ui = entry.ui();
                session_state.entries.push(entry);
                ui
            })
            .collect()
    }

    fn inspect_media(&self, path: &Path, kind: MediaKind) -> Result<Media, DecodingError> {
        match kind {
            MediaKind::Video => {
                inspect(path, &self.inspection_cancellation, &self.executables()).map(Media::Video)
            }
            MediaKind::Image => {
                inspect_image(path, &self.inspection_cancellation, &self.executables())
                    .map(Media::Image)
            }
        }
    }

    pub(crate) fn paste_bytes(&self, bytes: &[u8]) -> Result<EntryUi, UiError> {
        let not_valid = |message: &str| UiError::new(UiErrorKind::InvalidEntry, message);
        if bytes.is_empty() || bytes.len() > PASTE_LIMIT_BYTES {
            return Err(not_valid(&format!(
                "lo pegado tiene que pesar entre 1 B y {}",
                readable_size(PASTE_LIMIT_BYTES as u64)
            )));
        }
        let Some((kind, extension)) = kind_from_bytes(bytes) else {
            return Err(not_valid(
                "lo pegado no es una imagen ni un video que se pueda usar (PNG, JPEG, WebP, GIF, MP4, MOV o WebM)",
            ));
        };
        let (id, path) = {
            let mut state = self.state();
            if state.closing {
                return Err(UiError::from(ConversionError::Cancelled));
            }
            let id = state.new_id();
            let temp = state.temp().map_err(|error| {
                UiError::new(
                    UiErrorKind::Other,
                    format!("no se pudo crear la carpeta temporal: {error}"),
                )
            })?;
            (id, temp.join(format!("pegado-{id}.{extension}")))
        };
        fs::write(&path, bytes).map_err(|error| {
            let _ = fs::remove_file(&path);
            UiError::new(
                UiErrorKind::Other,
                format!("no se pudo guardar lo pegado: {error}"),
            )
        })?;
        let media = match self.inspect_media(&path, kind) {
            Ok(media) => media,
            Err(error) => {
                delete_pasted(&path);
                return Err(UiError::from(ConversionError::from(error)));
            }
        };
        let mut state = self.state();
        if state.closing {
            drop(state);
            delete_pasted(&path);
            return Err(UiError::from(ConversionError::Cancelled));
        }
        let mut entry = Entry {
            id,
            name: format!("pegado.{extension}"),
            name_base: "pegado".to_string(),
            origin: Origin::Pasted,
            state: EntryState::Ready,
            link: None,
            source: None,
            downloaded: None,
            video: None,
            image: None,
            pasted: Some(path),
            created: None,
            media_origin: MediaOrigin::Own,
        };
        entry.set_media(Ok(media));
        let ui = entry.ui();
        state.entries.push(entry);
        Ok(ui)
    }

    pub(crate) fn pasted_image(&self, id: u32) -> Option<PathBuf> {
        let state = self.state();
        let entry = state.entry(id).ok()?;
        entry.pasted.as_ref()?;
        entry.image.as_ref().map(|image| image.path.clone())
    }

    pub(crate) fn remove_entry(&self, id: u32) -> Result<Option<Deleted<'_>>, UiError> {
        let mut state = self.state();
        let taken = state.converting == Some(id)
            || matches!(state.entry(id)?.state, EntryState::Downloading { .. });
        if taken {
            return Err(UiError::new(
                UiErrorKind::Busy,
                "no se puede quitar un video mientras se descarga o se convierte",
            ));
        }
        let entry = state.entry_mut(id)?;
        let downloaded = entry.downloaded.take();
        if let Some(pasted) = entry.pasted.take() {
            delete_pasted(&pasted);
        }
        if state.preview.as_ref().is_some_and(|(of, _)| *of == id) {
            state.cancel_preview();
        }
        state.entries.retain(|entry| entry.id != id);
        if let Some(temp) = &state.temp {
            thumbnails::forget(temp.path(), id);
        }
        Ok(downloaded.map(|downloaded| {
            state.deleting += 1;
            Deleted {
                session: self,
                downloaded: Some(downloaded),
            }
        }))
    }

    pub(crate) fn close(&self) {
        let mut state = self.state();
        state.closing = true;
        self.cancel_creation();
        self.cancel_downloads();
        state.cancel_preview();
        state.cancel_estimate();
        self.inspection_cancellation.store(true, Ordering::Relaxed);
        let (mut state, waits) = self
            .stopped_jobs
            .wait_timeout_while(state, CLOSE_DEADLINE, |state| {
                state.downloading
                    || state.converting.is_some()
                    || state.previewing > 0
                    || state.estimating > 0
                    || state.deleting > 0
            })
            .unwrap_or_else(PoisonError::into_inner);
        if waits.timed_out() {
            warn!(
                "no todo se detuvo a tiempo al cerrar; puede quedar yt-dlp, ffmpeg o un temporal"
            );
        }
        state.entries.clear();
        if let Some(temp) = state.temp.take()
            && let Err(error) = temp.close()
        {
            warn!(%error, "no se pudo borrar el temporal de la sesión");
        }
    }

    pub(crate) fn cancel_creation(&self) {
        self.creation_cancellation.store(true, Ordering::Relaxed);
    }

    pub(crate) fn create(
        &self,
        request: &CreateRequest,
        progress: &(dyn Fn(ProgressUi) + Sync),
    ) -> Result<CreateResult, UiError> {
        let preset = find_preset_of(request)?;
        let options = conversion_options(request, &preset)?;
        let framing = framing_spec(&request.framing)?;
        self.check_memory(request, &preset)?;

        let (path, info) = self.reserve_conversion(request.id, |entry| {
            let Some(video) = &entry.video else {
                return Err(UiError::new(
                    UiErrorKind::InvalidEntry,
                    format!("«{}» no es un video listo para convertir", entry.name),
                ));
            };
            Ok((video.path.clone(), video.info))
        })?;
        let _conversion_end = ConversionEnd(self);

        let decoder = self.executables().decoder(&path).with_framing(framing);
        let report = |stage: Progress| progress(progress_ui(stage));
        let result = convert(
            &decoder,
            &options,
            Tracking::new(&report, &self.creation_cancellation),
        );
        let report = match result {
            Ok(report) => report,
            Err(ConversionError::DoesNotFitInSize {
                limit_bytes,
                best_bytes,
                best_attempt,
                suggested_seconds,
                ..
            }) => {
                let duration_s = plan_time(info.duration_s, &options)
                    .map_or(request.seconds, |plan: TimePlan| plan.output_duration_s);
                let speed = request.speed;
                return Ok(CreateResult {
                    meets_limit: false,
                    size_bytes: best_bytes,
                    max_size_bytes: limit_bytes,
                    fps: Some(best_attempt.fps.value()),
                    duration_s: Some(duration_s),
                    reductions: Vec::new(),
                    notice: Some(does_not_fit_notice(
                        limit_bytes,
                        best_bytes,
                        suggested_seconds.map(|seconds| seconds * speed),
                    )),
                });
            }
            Err(error) => return Err(UiError::from(error)),
        };
        if self.creation_cancellation.load(Ordering::Relaxed) {
            return Err(UiError::from(ConversionError::Cancelled));
        }

        let result = CreateResult {
            meets_limit: true,
            size_bytes: report.size_bytes(),
            max_size_bytes: report.max_size.bytes(),
            fps: Some(report.attempt.fps.value()),
            duration_s: Some(report.duration_ms as f64 / 1000.0),
            reductions: report.reductions.iter().map(ToString::to_string).collect(),
            notice: None,
        };
        self.state().entry_mut(request.id)?.created = Some(Created {
            bytes: report.bytes,
            extension: preset.format.extension(),
        });
        Ok(result)
    }

    pub(crate) fn estimate(&self, request: &CreateRequest) -> Result<EstimateUi, UiError> {
        let preset = find_preset_of(request)?;
        let options = conversion_options(request, &preset)?;
        let framing = framing_spec(&request.framing)?;
        self.check_memory(request, &preset)?;
        let (path, cancellation) = {
            let mut state = self.state();
            if state.closing {
                return Err(UiError::from(ConversionError::Cancelled));
            }
            if state.converting.is_some() {
                return Err(UiError::new(
                    UiErrorKind::Busy,
                    "se está creando un archivo; la estimación espera a que termine",
                ));
            }
            let path = match &state.entry(request.id)?.video {
                Some(video) => video.path.clone(),
                None => {
                    return Err(UiError::new(
                        UiErrorKind::InvalidEntry,
                        "la entrada no es un video listo para estimar",
                    ));
                }
            };
            state.cancel_estimate();
            let cancellation = Arc::new(AtomicBool::new(false));
            state.estimate = Some(Arc::clone(&cancellation));
            state.estimating += 1;
            (path, cancellation)
        };
        let _fin = EstimateEnd(self);

        let no_progress = |_: Progress| {};
        let estimate = conversor_core::estimate(
            &self.executables().decoder(&path).with_framing(framing),
            &options,
            Tracking::new(&no_progress, &cancellation),
        )?;
        Ok(EstimateUi {
            estimated_size_bytes: estimate.estimated_size_bytes,
            max_size_bytes: estimate.max_size.bytes(),
            fits: estimate.fits,
            fps: estimate.attempt.fps.value(),
            max_seconds_s: estimate.max_seconds_s * request.speed,
            max_range_s: estimate.max_seconds_s * request.speed,
            detail_px: estimate.attempt.detail_side(),
        })
    }

    fn reserve_conversion<T>(
        &self,
        id: u32,
        data: impl FnOnce(&Entry) -> Result<T, UiError>,
    ) -> Result<T, UiError> {
        let mut state = self.state();
        if state.closing {
            return Err(UiError::from(ConversionError::Cancelled));
        }
        if state.converting.is_some() {
            return Err(UiError::new(
                UiErrorKind::Busy,
                "ya se está creando otro archivo; espera a que termine o cancélalo",
            ));
        }
        let entry = state.entry_mut(id)?;
        let data = data(entry)?;
        entry.created = None;
        state.converting = Some(id);
        state.cancel_estimate();
        self.creation_cancellation.store(false, Ordering::Relaxed);
        Ok(data)
    }

    pub(crate) fn create_static(&self, request: &StaticRequest) -> Result<CreateResult, UiError> {
        let preset = &WHATSAPP_STATIC_STICKER;
        let framing = framing_spec(&request.framing)?;
        let path = self.reserve_conversion(request.id, |entry| {
            let Some(image) = &entry.image else {
                return Err(UiError::new(
                    UiErrorKind::InvalidEntry,
                    format!("«{}» no es una imagen lista para convertir", entry.name),
                ));
            };
            Ok(image.path.clone())
        })?;
        let _conversion_end = ConversionEnd(self);

        let image = self
            .executables()
            .decoder(&path)
            .decode_image(&self.creation_cancellation)
            .map_err(|e| UiError::from(ConversionError::from(e)))?;
        let no_progress = |_: Progress| {};
        let report = match convert_image(
            &image,
            preset,
            &framing,
            Tracking::new(&no_progress, &self.creation_cancellation),
        ) {
            Ok(report) => report,
            Err(ConversionError::DoesNotFitInSize {
                limit_bytes,
                best_bytes,
                ..
            }) => {
                return Ok(CreateResult {
                    meets_limit: false,
                    size_bytes: best_bytes,
                    max_size_bytes: limit_bytes,
                    fps: None,
                    duration_s: None,
                    reductions: Vec::new(),
                    notice: Some(format!(
                        "No cabe en {}: lo más ligero pesó {}. Prueba con otra imagen.",
                        readable_size(limit_bytes),
                        readable_size(best_bytes)
                    )),
                });
            }
            Err(error) => return Err(UiError::from(error)),
        };
        if self.creation_cancellation.load(Ordering::Relaxed) {
            return Err(UiError::from(ConversionError::Cancelled));
        }
        let result = CreateResult {
            meets_limit: true,
            size_bytes: report.bytes.len() as u64,
            max_size_bytes: report.max_size.bytes(),
            fps: None,
            duration_s: None,
            reductions: report.reductions.iter().map(ToString::to_string).collect(),
            notice: None,
        };
        self.state().entry_mut(request.id)?.created = Some(Created {
            bytes: report.bytes,
            extension: "webp",
        });
        Ok(result)
    }

    pub(crate) fn result(&self, id: u32) -> Result<Vec<u8>, UiError> {
        let state = self.state();
        Ok(created(state.entry(id)?)?.bytes.clone())
    }

    pub(crate) fn media_origin(&self, id: u32) -> Result<MediaOrigin, UiError> {
        Ok(self.state().entry(id)?.media_origin)
    }

    pub(crate) fn mark_media_origin(&self, id: u32, origin: MediaOrigin) -> Result<(), UiError> {
        self.state().entry_mut(id)?.media_origin = origin;
        Ok(())
    }

    pub(crate) fn proposal_saved(&self, id: u32) -> Result<ProposalSaved, UiError> {
        let state = self.state();
        let entry = state.entry(id)?;
        let extension = created(entry)?.extension;
        Ok(ProposalSaved {
            name: format!("{}.{extension}", entry.name_base),
            extension,
        })
    }

    pub(crate) fn save_to(&self, id: u32, path: &Path) -> Result<String, UiError> {
        let (bytes, extension, video) = {
            let state = self.state();
            let entry = state.entry(id)?;
            let created = created(entry)?;
            (
                created.bytes.clone(),
                created.extension,
                entry.video.as_ref().map(|video| video.path.clone()),
            )
        };
        let path = with_extension(path, extension);
        if let (Some(video), Ok(destination)) = (video, fs::canonicalize(&path))
            && video == destination
        {
            return Err(UiError::new(
                UiErrorKind::InvalidOptions,
                "no se puede guardar encima del video original; elige otro nombre",
            ));
        }
        write_atomic(&path, &bytes).map_err(|error| {
            UiError::new(
                UiErrorKind::Other,
                format!("no se pudo guardar {}: {error}", path.display()),
            )
        })?;
        Ok(text_of(path.file_name(), "archivo"))
    }
}

struct EstimateEnd<'a>(&'a Session);

impl Drop for EstimateEnd<'_> {
    fn drop(&mut self) {
        self.0.state().estimating -= 1;
        self.0.stopped_jobs.notify_all();
    }
}

struct ConversionEnd<'a>(&'a Session);

impl Drop for ConversionEnd<'_> {
    fn drop(&mut self) {
        self.0.state().converting = None;
        self.0.stopped_jobs.notify_all();
    }
}

pub(crate) struct Deleted<'a> {
    session: &'a Session,
    downloaded: Option<DownloadedVideo>,
}

impl std::fmt::Debug for Deleted<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Borrado").finish_non_exhaustive()
    }
}

impl Drop for Deleted<'_> {
    fn drop(&mut self) {
        drop(self.downloaded.take());
        self.session.state().deleting -= 1;
        self.session.stopped_jobs.notify_all();
    }
}

fn created(entry: &Entry) -> Result<&Created, UiError> {
    entry.created.as_ref().ok_or_else(|| {
        UiError::new(
            UiErrorKind::InvalidEntry,
            format!("todavía no se ha creado nada a partir de «{}»", entry.name),
        )
    })
}

fn inspect(
    path: &Path,
    cancellation: &AtomicBool,
    executables: &Executables,
) -> Result<Video, DecodingError> {
    let decoder = executables.decoder(path);
    let path = decoder.validate_file()?;
    let info = decoder.inspect_cancelable(cancellation)?;
    Ok(Video { path, info })
}

fn inspect_image(
    path: &Path,
    cancellation: &AtomicBool,
    executables: &Executables,
) -> Result<Image, DecodingError> {
    let path = fs::canonicalize(path).map_err(|source| DecodingError::UnreadableFile {
        path: path.to_path_buf(),
        source,
    })?;
    let info = executables
        .decoder(&path)
        .inspect_image_cancelable(cancellation)?;
    Ok(Image { path, info })
}

pub(crate) const PASTE_LIMIT_BYTES: usize = 50 * 1024 * 1024;

fn kind_from_bytes(bytes: &[u8]) -> Option<(MediaKind, &'static str)> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some((MediaKind::Image, "png"))
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some((MediaKind::Image, "jpg"))
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some((MediaKind::Image, "webp"))
    } else if bytes.starts_with(b"GIF8") {
        Some((MediaKind::Video, "gif"))
    } else if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        Some((MediaKind::Video, "mp4"))
    } else if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some((MediaKind::Video, "webm"))
    } else {
        None
    }
}

fn delete_pasted(path: &Path) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != io::ErrorKind::NotFound
    {
        warn!(%error, "no se pudo borrar un archivo pegado");
    }
}

fn text_of(name: Option<&OsStr>, default: &str) -> String {
    name.map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub(crate) fn with_extension(path: &Path, extension: &str) -> PathBuf {
    let already_has_it = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|current| current.eq_ignore_ascii_case(extension));
    if already_has_it {
        path.to_path_buf()
    } else {
        let mut name = path.as_os_str().to_owned();
        name.push(".");
        name.push(extension);
        PathBuf::from(name)
    }
}

fn priority_of(priority: PriorityUi) -> Priority {
    match priority {
        PriorityUi::Sharpness => Priority::Sharpness,
        PriorityUi::Balance => Priority::Balance,
        PriorityUi::Smoothness => Priority::Smoothness,
        PriorityUi::FitAnyway => Priority::FitAnyway,
        PriorityUi::Fluid => Priority::Fluid,
    }
}

fn framing_spec(framing: &FramingSpecUi) -> Result<FramingSpec, UiError> {
    let not_valid = |message: String| UiError::new(UiErrorKind::InvalidOptions, message);
    let mode = match framing.mode {
        FramingModeUi::Crop => FramingMode::Crop,
        FramingModeUi::Fit => FramingMode::Fit,
    };
    let background = match &framing.background {
        BackgroundUi::Transparent => Background::Transparent,
        BackgroundUi::Blur => Background::Blur,
        BackgroundUi::Color { rgb } => Background::Color(
            parse_color(rgb)
                .ok_or_else(|| not_valid("El color de fondo tiene que ser #rrggbb".to_string()))?,
        ),
    };
    let shape = match framing.shape {
        ShapeUi::Square => Shape::Square,
        ShapeUi::Rounded => Shape::Rounded,
        ShapeUi::Circle => Shape::Circle,
    };
    let align = match framing.align {
        AlignUi::Top => Align::Top,
        AlignUi::Center => Align::Center,
        AlignUi::Bottom => Align::Bottom,
    };
    FramingSpec::new(mode, framing.center_x, framing.center_y, framing.zoom)
        .and_then(|spec| spec.with_style(background, shape, framing.scale, align))
        .map_err(|error| not_valid(error.to_string()))
}

fn parse_color(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&digits[range], 16).ok();
    Some([channel(0..2)?, channel(2..4)?, channel(4..6)?])
}

/// The preset the request asks for, with the output side it picks when the canvas is free.
fn find_preset_of(request: &CreateRequest) -> Result<Preset, UiError> {
    let invalid = |message: String| UiError::new(UiErrorKind::InvalidOptions, message);
    let preset = find_preset(&request.preset)
        .ok_or_else(|| invalid(format!("no existe el destino «{}»", request.preset)))?;
    let Some(side) = request.side_px else {
        return Ok(*preset);
    };
    match side_range(preset) {
        Some(range) if (range.min..=range.max).contains(&side) => Ok(Preset {
            canvas: Canvas::Max(side),
            ..*preset
        }),
        _ => Err(invalid(SIDE_NOT_SUPPORTED.to_string())),
    }
}

fn conversion_options(
    request: &CreateRequest,
    preset: &Preset,
) -> Result<ConversionOptions, UiError> {
    let not_valid = |message: String| UiError::new(UiErrorKind::InvalidOptions, message);
    if !(request.start_s.is_finite() && request.start_s >= 0.0) {
        return Err(not_valid(
            "el inicio del tramo tiene que ser un número de segundos mayor o igual que 0"
                .to_string(),
        ));
    }
    let speed = Speed::new(request.speed).map_err(|_| {
        not_valid(format!(
            "la velocidad tiene que estar entre {} y {}",
            decimal(Speed::MIN),
            decimal(Speed::MAX)
        ))
    })?;
    if !(request.seconds > 0.0 && request.seconds.is_finite()) {
        return Err(not_valid("el tramo tiene que durar más de 0 s".to_string()));
    }
    let output_s = request.seconds / speed.value();
    let cap_s = preset.max_duration_s;
    if output_s > cap_s + 1e-9 {
        return Err(not_valid(format!(
            "un tramo de {} s a {}x dura {} s y el máximo es {} s",
            decimal_1(request.seconds),
            decimal(speed.value()),
            decimal_1(output_s),
            decimal(cap_s)
        )));
    }
    if let Some(fps) = request.fps
        && !(Fps::MIN..=preset.max_fps()).contains(&fps)
    {
        return Err(not_valid(FPS_NOT_SUPPORTED.to_string()));
    }
    let priority = priority_of(request.priority);
    let params = ConversionParams {
        max_seconds: Some(output_s.min(cap_s)),
        start_s: Some(request.start_s),
        fps: Some(request.fps.unwrap_or(priority.default_fps().value())),
        ..ConversionParams::for_preset(*preset)
    };
    let options =
        ConversionOptions::new(&params).map_err(|e| UiError::from(ConversionError::from(e)))?;
    let options = options
        .with_speed(speed)
        .map_err(|e| not_valid(e.to_string()))?;
    Ok(options.with_priority(priority))
}

fn progress_ui(stage: Progress) -> ProgressUi {
    match stage {
        Progress::Decoding => ProgressUi::Decoding,
        Progress::Preview => ProgressUi::Preview,
        Progress::FittingSize { attempt, max } => ProgressUi::FittingSize { attempt, max },
        Progress::SearchingDuration { attempt, max } => {
            ProgressUi::SearchingDuration { attempt, max }
        }
    }
}

fn does_not_fit_notice(
    limit_bytes: u64,
    best_bytes: u64,
    suggested_seconds: Option<f64>,
) -> String {
    let suggestion = match suggested_seconds {
        Some(seconds) => {
            format!("Prueba con un tramo de {} s como mucho.", decimal(seconds))
        }
        None => "No se encontró una duración que quepa; prueba con otro tramo.".to_string(),
    };
    format!(
        "No cabe en {}: lo más ligero pesó {}. {suggestion}",
        readable_size(limit_bytes),
        readable_size(best_bytes)
    )
}

impl From<ConversionError> for UiError {
    fn from(error: ConversionError) -> Self {
        let kind = match &error {
            ConversionError::Options(OptionsError::DoesNotFitEvenSpedUp {
                available_s,
                max_s,
                crop_s,
                ..
            }) => {
                return UiError::new(
                    UiErrorKind::DoesNotFitEvenSpedUp,
                    format!(
                        "El tramo dura {} s y, acelerando 2x, caben {} s. Acórtalo al menos {} s.",
                        decimal_1(*available_s),
                        decimal_1(*max_s),
                        decimal_1(*crop_s)
                    ),
                );
            }
            ConversionError::Options(_) => UiErrorKind::InvalidOptions,
            ConversionError::Decoding(decoding) => match decoding {
                DecodingError::FfmpegNotFound { .. } => UiErrorKind::FfmpegNotFound,
                DecodingError::TimedOut { .. } => UiErrorKind::TimedOut,
                DecodingError::Cancelled => UiErrorKind::Cancelled,
                DecodingError::FileNotFound { .. }
                | DecodingError::NotRegularFile { .. }
                | DecodingError::UnreadableFile { .. }
                | DecodingError::FileTooLarge { .. }
                | DecodingError::FfmpegFailed { .. }
                | DecodingError::WithoutVideoTrack
                | DecodingError::WithoutAudioTrack
                | DecodingError::AudioNotSupported { .. }
                | DecodingError::VideoCodecNotSupported { .. }
                | DecodingError::UnreadableDuration { .. }
                | DecodingError::DurationNotValid { .. }
                | DecodingError::DimensionsNotValid { .. } => UiErrorKind::InvalidEntry,
                DecodingError::VideoWithoutFrames => UiErrorKind::InvalidOptions,
                DecodingError::ProcessNotLaunched { .. }
                | DecodingError::ThreadNotLaunched { .. }
                | DecodingError::UnreadableOutput { .. }
                | DecodingError::Clip(_) => UiErrorKind::Other,
            },
            ConversionError::Cancelled => UiErrorKind::Cancelled,
            ConversionError::DoesNotFitInSize { .. }
            | ConversionError::Preview(_)
            | ConversionError::SizeFitting(_)
            | ConversionError::PreviewNotSaved(_) => UiErrorKind::Other,
        };
        UiError::new(kind, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::sync::Mutex as TestMutex;
    use std::sync::atomic::AtomicU64;

    use conversor_core::FfmpegDecoder;

    use super::*;

    static VIDEO_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn presets_come_from_core_with_their_title() {
        let presets = presets();
        let names: Vec<_> = presets.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "whatsapp-sticker",
                "discord-gif",
                "discord-emoji",
                "discord-sticker"
            ]
        );
        let whatsapp = &presets[0];
        assert_eq!(whatsapp.title, "Sticker animado de WhatsApp");
        assert_eq!(whatsapp.extension, "webp");
        assert_eq!(whatsapp.side, 512);
        assert_eq!(whatsapp.max_size_bytes, 500_000);
        assert_eq!(whatsapp.max_duration_s, 10.0);
        assert_eq!(presets[1].max_size_bytes, 10_000_000);
        assert_eq!(presets[3].title, "Sticker de Discord");
        assert_eq!(whatsapp.fps_range, FpsRangeUi { min: 5, max: 30 });
    }

    fn generate_video(directory: &Path, filter: &str, seconds: f64) -> PathBuf {
        let number = VIDEO_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!("video-{number}.mp4"));
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", filter])
            .args(["-t", &seconds.to_string()])
            .args([
                "-c:v", "mpeg4", "-q:v", "3", "-pix_fmt", "yuv420p", "-f", "mp4",
            ])
            .arg(&path)
            .output()
            .expect("hace falta ffmpeg para estas pruebas");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        path
    }

    fn pattern(directory: &Path, seconds: f64) -> PathBuf {
        generate_video(directory, "testsrc2=size=320x240:rate=30", seconds)
    }

    fn request(id: u32, preset: &str, start_s: f64, seconds: f64) -> CreateRequest {
        CreateRequest {
            id,
            preset: preset.to_string(),
            start_s,
            seconds,
            speed: 1.0,
            priority: PriorityUi::Balance,
            fps: None,
            side_px: None,
            framing: FramingSpecUi::default(),
        }
    }

    fn no_progress(_: ProgressUi) {}

    fn session_with_video(directory: &Path, seconds: f64) -> (Session, u32) {
        let session = Session::default();
        let entries = session.add_files(vec![pattern(directory, seconds)]);
        assert_eq!(entries[0].state, EntryState::Ready);
        (session, entries[0].id)
    }

    #[test]
    fn add_files_inspects_them_and_flags_invalid_ones() {
        let directory = tempfile::tempdir().unwrap();
        let video = pattern(directory.path(), 2.0);
        let broken = directory.path().join("roto.mp4");
        fs::write(&broken, b"esto no es un video").unwrap();
        let session = Session::default();

        let entries = session.add_files(vec![
            video.clone(),
            broken,
            directory.path().join("no-existe.mp4"),
        ]);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].state, EntryState::Ready);
        assert_eq!(entries[0].origin, Origin::File);
        let Some(MediaUi::Video {
            duration_s,
            width,
            height,
            ..
        }) = entries[0].media
        else {
            panic!("{:?}", entries[0]);
        };
        let saved = session.state().entries[0]
            .video
            .as_ref()
            .unwrap()
            .path
            .clone();
        assert_eq!(saved, fs::canonicalize(&video).unwrap());
        assert!((duration_s - 2.0).abs() < 0.1, "{duration_s}");
        assert_eq!((width, height), (320, 240));
        assert_eq!(entries[1].name, "roto.mp4");
        for entry in &entries[1..] {
            let EntryState::Error { error } = &entry.state else {
                panic!("{entry:?}");
            };
            assert_eq!(error.kind, UiErrorKind::InvalidEntry);
            assert!(entry.media.is_none());
        }
        let ids: Vec<_> = entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, [1, 2, 3]);
    }

    #[test]
    fn invalid_requests_fail_before_decoding() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 1.0);
        let cases = [
            (
                request(id, "telegram", 0.0, 1.0),
                UiErrorKind::InvalidOptions,
            ),
            (
                request(99, "whatsapp-sticker", 0.0, 1.0),
                UiErrorKind::InvalidEntry,
            ),
            (
                request(id, "whatsapp-sticker", f64::NAN, 1.0),
                UiErrorKind::InvalidOptions,
            ),
            (
                request(id, "whatsapp-sticker", -1.0, 1.0),
                UiErrorKind::InvalidOptions,
            ),
            (
                request(id, "whatsapp-sticker", 0.0, f64::NAN),
                UiErrorKind::InvalidOptions,
            ),
            (
                request(id, "whatsapp-sticker", 0.0, f64::INFINITY),
                UiErrorKind::InvalidOptions,
            ),
            (
                request(id, "whatsapp-sticker", 0.0, 0.0),
                UiErrorKind::InvalidOptions,
            ),
            (
                request(id, "whatsapp-sticker", 0.0, 10.5),
                UiErrorKind::InvalidOptions,
            ),
            (
                request(id, "discord-sticker", 0.0, 6.0),
                UiErrorKind::InvalidOptions,
            ),
        ];
        let stages = TestMutex::new(Vec::new());
        let register = |stage| stages.lock().unwrap().push(stage);
        for (request, kind) in cases {
            let error = session.create(&request, &register).unwrap_err();
            assert_eq!(error.kind, kind, "{request:?}: {error:?}");
        }
        assert!(stages.lock().unwrap().is_empty());
    }

    fn generate_image(directory: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let path = directory.join(name);
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i"])
            .arg(format!("testsrc2=size={width}x{height}:rate=1"))
            .args(["-frames:v", "1"])
            .arg(&path)
            .output()
            .expect("hace falta ffmpeg para estas pruebas");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        path
    }

    fn session_with_image(directory: &Path) -> (Session, u32) {
        let session = Session::default();
        let entries = session.add_images(vec![generate_image(directory, "foto.png", 300, 200)]);
        assert_eq!(entries[0].state, EntryState::Ready);
        (session, entries[0].id)
    }

    #[test]
    fn image_enters_with_its_sides_and_several_at_once_give_several_entries() {
        let directory = tempfile::tempdir().unwrap();
        let png = generate_image(directory.path(), "a.png", 300, 200);
        let jpg = generate_image(directory.path(), "b.jpg", 64, 64);
        let fake = directory.path().join("c.png");
        fs::write(&fake, b"no soy una imagen").unwrap();
        let session = Session::default();

        let entries = session.add_images(vec![png.clone(), jpg, fake]);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].state, EntryState::Ready);
        assert_eq!(entries[0].origin, Origin::File);
        assert_eq!(
            entries[0].media,
            Some(MediaUi::Image {
                path: fs::canonicalize(&png)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                width: 300,
                height: 200,
            })
        );
        assert_eq!(entries[1].state, EntryState::Ready);
        assert!(matches!(entries[2].state, EntryState::Error { .. }));
        assert_eq!(entries[2].media, None);
    }

    #[test]
    fn create_static_returns_fixed_webp_passing_whatsapp_inspection() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_image(directory.path());
        let fit = FramingSpecUi {
            mode: FramingModeUi::Fit,
            ..FramingSpecUi::default()
        };
        for framing in [FramingSpecUi::default(), fit] {
            let result = session
                .create_static(&StaticRequest { id, framing })
                .unwrap();

            assert!(result.meets_limit, "{result:?}");
            assert_eq!(result.fps, None);
            assert_eq!(result.duration_s, None);
            let bytes = session.result(id).unwrap();
            assert_eq!(result.size_bytes, bytes.len() as u64);
            let sticker = conversor_core::inspect_whatsapp_sticker(&bytes).unwrap();
            assert_eq!(sticker.kind, conversor_core::StickerKind::Static);
            assert_eq!(session.proposal_saved(id).unwrap().name, "foto.webp");
        }
    }

    #[test]
    fn video_and_image_are_not_mixed_between_commands() {
        let directory = tempfile::tempdir().unwrap();
        let (session, image) = session_with_image(directory.path());
        let video = session.add_files(vec![pattern(directory.path(), 1.0)])[0].id;
        let static_image = |id| StaticRequest {
            id,
            framing: FramingSpecUi::default(),
        };

        let error = session.create_static(&static_image(video)).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);
        let error = session
            .create(&request(image, "whatsapp-sticker", 0.0, 1.0), &no_progress)
            .unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);
        let error = session.create_static(&static_image(99)).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);
        assert!(
            session
                .create_static(&static_image(image))
                .unwrap()
                .meets_limit
        );
    }

    fn bytes_of(directory: &Path, name: &str, entry: &str, options: &[&str]) -> Vec<u8> {
        let path = directory.join(name);
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", entry])
            .args(options)
            .arg(&path)
            .output()
            .expect("hace falta ffmpeg para estas pruebas");
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read(path).unwrap()
    }

    fn temp_files(session: &Session) -> Vec<PathBuf> {
        let state = session.state();
        let Some(temp) = &state.temp else {
            return Vec::new();
        };
        fs::read_dir(temp.path())
            .unwrap()
            .map(|file| file.unwrap().path())
            .collect()
    }

    #[test]
    fn paste_accepts_images_and_videos_by_magic_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let source = "testsrc2=size=96x64:rate=5";
        let cases = [
            ("a.png", &["-frames:v", "1"][..], true),
            ("a.jpg", &["-frames:v", "1"][..], true),
            ("a.webp", &["-frames:v", "1"][..], true),
            ("a.gif", &["-frames:v", "5"][..], false),
            ("a.mp4", &["-frames:v", "5", "-c:v", "mpeg4"][..], false),
            ("a.webm", &["-frames:v", "5", "-c:v", "libvpx"][..], false),
        ];
        let session = Session::default();
        for (name, options, is_image) in cases {
            let bytes = bytes_of(directory.path(), name, source, options);

            let entry = session.paste_bytes(&bytes).unwrap();

            assert_eq!(entry.origin, Origin::Pasted, "{name}");
            assert_eq!(entry.state, EntryState::Ready, "{name}");
            let media = entry.media.unwrap();
            assert_eq!(matches!(media, MediaUi::Image { .. }), is_image, "{name}");
            if let MediaUi::Image {
                path,
                width,
                height,
            } = media
            {
                assert!(Path::new(&path).is_file(), "{path}");
                assert_eq!((width, height), (96, 64));
            }
        }
        assert_eq!(temp_files(&session).len(), 6);
    }

    #[test]
    fn paste_rejects_non_image_non_video_even_if_claimed() {
        let session = Session::default();
        for bytes in [
            &b"hola, esto es texto"[..],
            b"file:///home/usuario/secreto.png",
            b"<svg xmlns='http://www.w3.org/2000/svg'/>",
        ] {
            let error = session.paste_bytes(bytes).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidEntry);
        }
        let mut fake_png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        fake_png.extend_from_slice(b"basura");
        let error = session.paste_bytes(&fake_png).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);
        assert!(temp_files(&session).is_empty());
        assert!(session.state().entries.is_empty());
    }

    #[test]
    fn paste_checks_size_before_writing_anything() {
        let session = Session::default();

        let empty = session.paste_bytes(&[]).unwrap_err();
        let large = session
            .paste_bytes(&vec![0x89; PASTE_LIMIT_BYTES + 1])
            .unwrap_err();

        assert_eq!(empty.kind, UiErrorKind::InvalidEntry);
        assert_eq!(large.kind, UiErrorKind::InvalidEntry);
        assert!(session.state().temp.is_none());
    }

    #[test]
    fn removing_pasted_deletes_its_file_and_closing_leaves_no_temp_files() {
        let directory = tempfile::tempdir().unwrap();
        let session = Session::default();
        let png = bytes_of(
            directory.path(),
            "a.png",
            "testsrc2=size=32x32:rate=1",
            &["-frames:v", "1"],
        );
        let first = session.paste_bytes(&png).unwrap();
        let second_one = session.paste_bytes(&png).unwrap();
        let first_path = session.pasted_image(first.id).unwrap();
        assert!(first_path.is_file());
        assert_ne!(first_path, session.pasted_image(second_one.id).unwrap());
        assert_eq!(temp_files(&session).len(), 2);

        drop(session.remove_entry(first.id).unwrap());

        assert!(!first_path.exists());
        assert_eq!(temp_files(&session).len(), 1);
        let folder = session.state().temp.as_ref().unwrap().path().to_path_buf();
        session.close();
        assert!(!folder.exists());
    }

    #[test]
    fn picked_image_does_not_count_as_pasted() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_image(directory.path());
        assert_eq!(session.pasted_image(id), None);
        drop(session.remove_entry(id).unwrap());
        assert!(directory.path().join("foto.png").is_file());
    }

    #[test]
    fn out_of_range_speed_or_too_long_output_fail_without_decoding() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 1.0);
        let base = request(id, "whatsapp-sticker", 0.0, 1.0);
        let cases = [
            CreateRequest {
                speed: 0.4,
                ..base.clone()
            },
            CreateRequest {
                speed: 3.1,
                ..base.clone()
            },
            CreateRequest {
                speed: f64::NAN,
                ..base.clone()
            },
            CreateRequest {
                seconds: 12.0,
                ..base.clone()
            },
            CreateRequest {
                seconds: 5.1,
                speed: 0.5,
                ..base.clone()
            },
        ];
        let stages = TestMutex::new(Vec::new());
        let register = |stage| stages.lock().unwrap().push(stage);
        for request in cases {
            let error = session.create(&request, &register).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidOptions, "{request:?}");
            let error = session.estimate(&request).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidOptions, "{request:?}");
        }
        assert!(stages.lock().unwrap().is_empty());
    }

    #[test]
    fn eleven_seconds_at_1_1x_are_not_rejected_and_priority_sets_fps() {
        let preset = find_preset("whatsapp-sticker").unwrap();
        let base = request(1, "whatsapp-sticker", 0.0, 11.0);
        let options = conversion_options(
            &CreateRequest {
                speed: 1.1,
                ..base.clone()
            },
            preset,
        )
        .unwrap();
        assert!((options.target_duration_s() - 10.0).abs() < 1e-6);
        assert_eq!(options.speed().map(Speed::value), Some(1.1));

        let fps = |priority| {
            let request = CreateRequest {
                priority,
                seconds: 3.0,
                ..base.clone()
            };
            let options = conversion_options(&request, preset).unwrap();
            (options.fps().value(), options.priority())
        };
        assert_eq!(fps(PriorityUi::Smoothness), (20, Priority::Smoothness));
        assert_eq!(fps(PriorityUi::Sharpness), (12, Priority::Sharpness));
        assert_eq!(fps(PriorityUi::Balance).1, Priority::Balance);
        assert_eq!(fps(PriorityUi::FitAnyway), (15, Priority::FitAnyway));
        assert_eq!(fps(PriorityUi::Fluid), (20, Priority::Fluid));
    }

    #[test]
    fn requested_fps_wins_over_the_priority_default_and_is_validated() {
        let preset = find_preset("whatsapp-sticker").unwrap();
        let with = |priority, fps| CreateRequest {
            priority,
            fps,
            seconds: 3.0,
            ..request(1, "whatsapp-sticker", 0.0, 3.0)
        };
        let options = conversion_options(&with(PriorityUi::Fluid, Some(30)), preset).unwrap();
        assert_eq!(
            (options.fps().value(), options.priority()),
            (30, Priority::Fluid)
        );
        let options = conversion_options(&with(PriorityUi::Balance, Some(24)), preset).unwrap();
        assert_eq!(options.fps().value(), 24);
        for fps in [0, 4, 31, 120] {
            let error =
                conversion_options(&with(PriorityUi::Fluid, Some(fps)), preset).unwrap_err();
            assert_eq!(
                error.message, "Esos fps no se admiten en este destino",
                "{fps}"
            );
        }
    }

    #[test]
    fn any_fps_inside_the_advertised_range_is_accepted_and_the_ends_are_enforced() {
        for preset in presets() {
            let range = preset.fps_range;
            assert_eq!(range.min, 5, "{}", preset.name);
            assert_eq!(range.max, preset.max_fps, "{}", preset.name);
            let core_preset = find_preset(&preset.name).unwrap();
            let with = |fps| CreateRequest {
                fps: Some(fps),
                ..request(1, &preset.name, 0.0, 1.0)
            };
            for fps in range.min..=range.max {
                let options = conversion_options(&with(fps), core_preset).unwrap();
                assert_eq!(options.fps().value(), fps, "{}", preset.name);
            }
            for fps in [range.min - 1, range.max + 1] {
                let error = conversion_options(&with(fps), core_preset).unwrap_err();
                assert_eq!(error.kind, UiErrorKind::InvalidOptions);
                assert_eq!(error.message, "Esos fps no se admiten en este destino");
            }
        }
    }

    #[test]
    fn free_canvas_presets_advertise_a_side_range_and_fixed_ones_do_not() {
        let presets = presets();
        let range = |name: &str| {
            presets
                .iter()
                .find(|preset| preset.name == name)
                .unwrap()
                .side_range_px
                .clone()
        };
        assert_eq!(
            range("discord-gif"),
            Some(SideRangeUi {
                min: 64,
                max: 480,
                suggested: vec![240, 320, 480]
            })
        );
        for fixed in ["whatsapp-sticker", "discord-emoji", "discord-sticker"] {
            assert_eq!(range(fixed), None, "{fixed}");
        }
    }

    #[test]
    fn side_px_is_checked_against_the_range_of_the_preset() {
        let with = |preset: &str, side_px| CreateRequest {
            side_px,
            ..request(1, preset, 0.0, 1.0)
        };
        for side in [64, 241, 480] {
            let preset = find_preset_of(&with("discord-gif", Some(side))).unwrap();
            assert_eq!(preset.canvas, Canvas::Max(side));
        }
        assert_eq!(
            find_preset_of(&with("discord-gif", None)).unwrap().canvas,
            Canvas::Max(480)
        );
        for (preset, side) in [
            ("discord-gif", 63),
            ("discord-gif", 481),
            ("discord-gif", 0),
            ("whatsapp-sticker", 512),
            ("discord-emoji", 128),
            ("discord-sticker", 100),
        ] {
            let error = find_preset_of(&with(preset, Some(side))).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidOptions, "{preset} {side}");
            assert_eq!(error.message, "Ese tamaño no se admite en este destino");
        }
    }

    #[test]
    fn gif_is_created_at_the_chosen_odd_side() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 1.0);
        let request = CreateRequest {
            side_px: Some(241),
            ..request(id, "discord-gif", 0.0, 1.0)
        };

        let result = session.create(&request, &no_progress).unwrap();

        assert!(result.meets_limit, "{result:?}");
        let gif = session.result(id).unwrap();
        // The logical screen of a GIF starts after the 6-byte header: width and height, little endian.
        assert_eq!(u16::from_le_bytes([gif[6], gif[7]]), 241);
        assert_eq!(u16::from_le_bytes([gif[8], gif[9]]), 241);
    }

    #[test]
    fn three_seconds_at_0_5x_last_six() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 3.0);
        let request = CreateRequest {
            speed: 0.5,
            ..request(id, "whatsapp-sticker", 0.0, 3.0)
        };

        let result = session.create(&request, &no_progress).unwrap();

        assert!(result.meets_limit, "{result:?}");
        let duration = result.duration_s.unwrap();
        assert!((duration - 6.0).abs() < 0.1, "{duration}");
    }

    #[test]
    fn does_not_fit_notice_returns_clip_seconds_not_output_seconds() {
        let directory = tempfile::tempdir().unwrap();
        let session = Session::default();
        let noise = generate_video(
            directory.path(),
            "nullsrc=size=128x128:rate=30,geq=r='random(1)*255':g='random(2)*255':b='random(3)*255'",
            16.0,
        );
        let id = session.add_files(vec![noise.clone()])[0].id;
        let request = CreateRequest {
            speed: 2.0,
            ..request(id, "discord-emoji", 0.0, 16.0)
        };
        let options = conversion_options(&request, find_preset("discord-emoji").unwrap()).unwrap();
        let Err(ConversionError::DoesNotFitInSize {
            suggested_seconds: Some(suggested),
            ..
        }) = convert(&FfmpegDecoder::new(&noise), &options, Tracking::none())
        else {
            panic!("el ruido tenía que no caber y dar una sugerencia");
        };

        let notice = session
            .create(&request, &no_progress)
            .unwrap()
            .notice
            .unwrap();

        let expected = format!("un tramo de {} s", decimal(suggested * 2.0));
        assert!(notice.contains(&expected), "{notice} / {expected}");
    }

    fn wait_for(condition: impl Fn() -> bool) {
        for _ in 0..400 {
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("no se cumplió a tiempo");
    }

    #[test]
    fn invalid_framing_is_rejected_before_decoding_with_spanish_message() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 1.0);
        let (image_session, image) = session_with_image(directory.path());
        let wrong_crop = |center_x, zoom| FramingSpecUi {
            center_x,
            zoom,
            ..FramingSpecUi::default()
        };
        let wrong_scale = |scale| FramingSpecUi {
            scale,
            ..FramingSpecUi::default()
        };
        let wrong_color = |rgb: &str| FramingSpecUi {
            background: BackgroundUi::Color {
                rgb: rgb.to_string(),
            },
            ..FramingSpecUi::default()
        };
        let crop_message = "El encuadre no es válido";
        let scale_message = "El tamaño del contenido tiene que estar entre 0,5 y 1";
        let color_message = "El color de fondo tiene que ser #rrggbb";
        let cases = [
            (wrong_crop(1.5, 1.0), crop_message),
            (wrong_crop(f64::NAN, 1.0), crop_message),
            (wrong_crop(0.5, 0.5), crop_message),
            (wrong_crop(0.5, 4.5), crop_message),
            (wrong_scale(0.4), scale_message),
            (wrong_scale(1.1), scale_message),
            (wrong_scale(f64::NAN), scale_message),
            (wrong_color("rojo"), color_message),
            (wrong_color("#12345"), color_message),
            (wrong_color("#12345g"), color_message),
            (wrong_color("123456"), color_message),
        ];
        for (framing, message) in cases {
            let request = CreateRequest {
                framing: framing.clone(),
                ..request(id, "whatsapp-sticker", 0.0, 1.0)
            };
            let stages = TestMutex::new(Vec::new());
            let register = |stage| stages.lock().unwrap().push(stage);
            let error = session.create(&request, &register).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidOptions);
            assert_eq!(error.message, message);
            let error = session.estimate(&request).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidOptions);
            let error = image_session
                .create_static(&StaticRequest { id: image, framing })
                .unwrap_err();
            assert_eq!(error.message, message);
            assert!(stages.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn new_estimate_cancels_previous_and_answers() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 10.0);
        let request = request(id, "whatsapp-sticker", 0.0, 10.0);
        std::thread::scope(|threads| {
            let first = threads.spawn(|| session.estimate(&request));
            wait_for(|| session.state().estimating == 1);

            let second_one = session.estimate(&request).unwrap();

            assert_eq!(
                first.join().unwrap().unwrap_err().kind,
                UiErrorKind::Cancelled
            );
            assert!(second_one.estimated_size_bytes > 0);
            assert_eq!(second_one.max_size_bytes, 500_000);
            assert!(second_one.detail_px <= 512);
        });
        assert_eq!(session.state().estimating, 0);
    }

    #[test]
    fn estimate_while_creating_returns_busy_and_the_reverse_cancels_the_estimate() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 10.0);
        let request = request(id, "whatsapp-sticker", 0.0, 10.0);
        session.state().converting = Some(id);
        let error = session.estimate(&request).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::Busy);
        session.state().converting = None;

        std::thread::scope(|threads| {
            let estimate = threads.spawn(|| session.estimate(&request));
            wait_for(|| session.state().estimating == 1);

            let short = CreateRequest {
                seconds: 1.0,
                ..request.clone()
            };
            let created = session.create(&short, &no_progress).unwrap();

            assert!(created.meets_limit);
            assert_eq!(
                estimate.join().unwrap().unwrap_err().kind,
                UiErrorKind::Cancelled
            );
        });
    }

    #[test]
    fn short_video_meets_limit_and_result_is_webp() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 3.0);
        let stages = TestMutex::new(Vec::new());
        let register = |stage| stages.lock().unwrap().push(stage);

        let result = session
            .create(&request(id, "whatsapp-sticker", 0.5, 2.0), &register)
            .unwrap();

        assert!(result.meets_limit, "{result:?}");
        assert_eq!(result.max_size_bytes, 500_000);
        assert!(result.size_bytes <= 500_000);
        let duration_s = result.duration_s.unwrap();
        assert!((duration_s - 2.0).abs() < 0.1, "{result:?}");
        assert_eq!(result.notice, None);
        assert_eq!(stages.lock().unwrap()[0], ProgressUi::Decoding);
        let bytes = session.result(id).unwrap();
        assert_eq!(bytes.len() as u64, result.size_bytes);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
        assert_eq!(
            session.proposal_saved(id).unwrap(),
            ProposalSaved {
                name: format!(
                    "{}.webp",
                    Path::new(&session.state().entry(id).unwrap().name)
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                ),
                extension: "webp",
            }
        );
    }

    #[test]
    fn only_picked_range_is_converted() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 12.0);

        let result = session
            .create(&request(id, "discord-sticker", 1.0, 4.0), &no_progress)
            .unwrap();

        let duration_s = result.duration_s.unwrap();
        assert!((duration_s - 4.0).abs() < 0.15, "{result:?}");
        assert!(result.fps.is_some());
    }

    #[test]
    fn noise_in_discord_emoji_does_not_meet_limit_and_says_so_in_notice() {
        let directory = tempfile::tempdir().unwrap();
        let session = Session::default();
        let noise = generate_video(
            directory.path(),
            "nullsrc=size=128x128:rate=30,geq=r='random(1)*255':g='random(2)*255':b='random(3)*255'",
            8.0,
        );
        let id = session.add_files(vec![noise])[0].id;

        let result = session
            .create(&request(id, "discord-emoji", 0.0, 8.0), &no_progress)
            .unwrap();

        assert!(!result.meets_limit);
        assert_eq!(result.max_size_bytes, 256_000);
        assert!(result.size_bytes > 256_000, "{result:?}");
        let notice = result.notice.unwrap();
        assert!(notice.starts_with("No cabe en 256 KB"), "{notice}");
        assert!(!notice.contains("--"), "{notice}");
        assert_eq!(
            session.result(id).unwrap_err().kind,
            UiErrorKind::InvalidEntry
        );
    }

    #[test]
    fn when_it_does_not_fit_max_range_is_shorter_than_the_range_and_when_it_fits_it_is_capped() {
        let directory = tempfile::tempdir().unwrap();
        let session = Session::default();
        let noise = generate_video(
            directory.path(),
            "nullsrc=size=128x128:rate=30,geq=r='random(1)*255':g='random(2)*255':b='random(3)*255'",
            8.0,
        );
        let noisy = session.add_files(vec![noise])[0].id;
        let calm = session.add_files(vec![pattern(directory.path(), 2.0)])[0].id;
        let priorities = [
            PriorityUi::Sharpness,
            PriorityUi::Balance,
            PriorityUi::Smoothness,
            PriorityUi::FitAnyway,
            PriorityUi::Fluid,
        ];
        let mut not_fitting = 0;
        for priority in priorities {
            for speed in [1.0, 2.0] {
                let range = 8.0;
                let request = CreateRequest {
                    priority,
                    speed,
                    ..request(noisy, "discord-emoji", 0.0, range)
                };
                let estimate = session.estimate(&request).unwrap();
                assert!(
                    estimate.max_range_s > 0.0 && estimate.max_range_s <= 10.0 * speed + 1e-9,
                    "{priority:?} {speed}: {estimate:?}"
                );
                if !estimate.fits {
                    assert!(
                        estimate.max_range_s < range,
                        "{priority:?} {speed}: {estimate:?}"
                    );
                    not_fitting += 1;
                }

                let request = CreateRequest {
                    priority,
                    speed,
                    ..request_for(calm, 2.0)
                };
                let estimate = session.estimate(&request).unwrap();
                assert!(estimate.fits, "{priority:?} {speed}: {estimate:?}");
                // The cap of 10 s of output is 10 s * speed of the source video.
                assert!(
                    estimate.max_range_s <= 10.0 * speed + 1e-9,
                    "{priority:?} {speed}: {estimate:?}"
                );
                assert!(estimate.max_range_s >= 2.0 - 1e-9, "{estimate:?}");
            }
        }
        assert!(not_fitting >= priorities.len(), "{not_fitting}");
    }

    fn request_for(id: u32, seconds: f64) -> CreateRequest {
        request(id, "whatsapp-sticker", 0.0, seconds)
    }

    #[test]
    fn second_simultaneous_conversion_returns_busy() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 2.0);
        let second_one = TestMutex::new(None);
        let try_another = |_| {
            let mut second_one = second_one.lock().unwrap();
            if second_one.is_none() {
                *second_one =
                    Some(session.create(&request(id, "discord-emoji", 0.0, 1.0), &no_progress));
            }
        };

        let first = session.create(&request(id, "discord-emoji", 0.0, 1.0), &try_another);

        assert!(first.unwrap().meets_limit);
        let second_one = second_one.into_inner().unwrap().unwrap();
        assert_eq!(second_one.unwrap_err().kind, UiErrorKind::Busy);
        assert!(
            session
                .create(&request(id, "discord-emoji", 0.0, 1.0), &no_progress)
                .is_ok()
        );
    }

    #[test]
    fn cancelling_midway_returns_cancelled_and_releases_conversion() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 6.0);
        let cancel = |_| session.cancel_creation();

        let error = session
            .create(&request(id, "whatsapp-sticker", 0.0, 6.0), &cancel)
            .unwrap_err();

        assert_eq!(error.kind, UiErrorKind::Cancelled);
        assert_eq!(session.state().converting, None);
        assert!(session.result(id).is_err());
    }

    #[test]
    fn close_waits_for_conversion_to_stop() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 6.0);
        let (running, started) = std::sync::mpsc::channel();
        let slow = |_| {
            running.send(()).unwrap();
            let start = std::time::Instant::now();
            while !session.creation_cancellation.load(Ordering::Relaxed) {
                assert!(start.elapsed() < CLOSE_DEADLINE, "nadie la canceló");
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(300));
        };

        std::thread::scope(|threads| {
            let conversion =
                threads.spawn(|| session.create(&request(id, "whatsapp-sticker", 0.0, 6.0), &slow));
            started.recv_timeout(CLOSE_DEADLINE).unwrap();

            session.close();

            assert_eq!(session.state().converting, None, "cerrar no la esperó");
            let error = conversion.join().unwrap().unwrap_err();
            assert_eq!(error.kind, UiErrorKind::Cancelled);
        });
    }

    #[test]
    fn while_closing_no_conversion_starts_and_cancellation_is_kept() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 1.0);
        session.state().closing = true;
        session.cancel_creation();

        let error = session
            .create(&request(id, "discord-emoji", 0.0, 1.0), &no_progress)
            .unwrap_err();

        assert_eq!(error.kind, UiErrorKind::Cancelled);
        assert_eq!(session.state().converting, None);
        assert!(session.creation_cancellation.load(Ordering::Relaxed));
    }

    #[test]
    fn cannot_remove_what_is_converting_but_can_remove_the_rest() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 1.0);
        let remove = TestMutex::new(None);
        let try_remove = |_| {
            let mut remove = remove.lock().unwrap();
            if remove.is_none() {
                *remove = Some(session.remove_entry(id));
            }
        };
        session
            .create(&request(id, "discord-emoji", 0.0, 1.0), &try_remove)
            .unwrap();
        let error = remove.into_inner().unwrap().unwrap().unwrap_err();
        assert_eq!(error.kind, UiErrorKind::Busy);

        session.remove_entry(id).unwrap();
        assert_eq!(
            session.remove_entry(id).unwrap_err().kind,
            UiErrorKind::InvalidEntry
        );
        assert!(session.state().entries.is_empty());
    }

    #[test]
    fn save_writes_result_with_format_extension() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 1.0);
        assert_eq!(
            session
                .save_to(id, &directory.path().join("x.gif"))
                .unwrap_err()
                .kind,
            UiErrorKind::InvalidEntry
        );
        session
            .create(&request(id, "discord-emoji", 0.0, 1.0), &no_progress)
            .unwrap();

        let name = session
            .save_to(id, &directory.path().join("gatito"))
            .unwrap();

        assert_eq!(name, "gatito.gif");
        let storage = fs::read(directory.path().join("gatito.gif")).unwrap();
        assert_eq!(storage, session.result(id).unwrap());
        assert_eq!(&storage[..3], b"GIF");
    }

    #[test]
    fn not_saved_over_original_video() {
        let directory = tempfile::tempdir().unwrap();
        let session = Session::default();
        let gif = directory.path().join("baile.gif");
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x120:rate=10",
                "-t",
                "1",
            ])
            .arg(&gif)
            .output()
            .unwrap();
        assert!(output.status.success());
        let id = session.add_files(vec![gif.clone()])[0].id;
        session
            .create(&request(id, "discord-gif", 0.0, 1.0), &no_progress)
            .unwrap();
        let original = fs::read(&gif).unwrap();

        let error = session.save_to(id, &gif).unwrap_err();

        assert_eq!(error.kind, UiErrorKind::InvalidOptions);
        assert_eq!(fs::read(&gif).unwrap(), original);
    }

    #[test]
    fn with_extension_only_adds_it_if_missing() {
        assert_eq!(
            with_extension(Path::new("/tmp/gato.webp"), "webp"),
            Path::new("/tmp/gato.webp")
        );
        assert_eq!(
            with_extension(Path::new("/tmp/gato.WEBP"), "webp"),
            Path::new("/tmp/gato.WEBP")
        );
        assert_eq!(
            with_extension(Path::new("/tmp/gato"), "gif"),
            Path::new("/tmp/gato.gif")
        );
        assert_eq!(
            with_extension(Path::new("/tmp/gato.mp4"), "png"),
            Path::new("/tmp/gato.mp4.png")
        );
    }

    #[test]
    fn does_not_fit_notice_talks_about_app_controls() {
        assert_eq!(
            does_not_fit_notice(500_000, 612_001, Some(4.5)),
            "No cabe en 500 KB: lo más ligero pesó 613 KB. Prueba con un tramo de 4,5 s como mucho."
        );
        assert_eq!(
            does_not_fit_notice(256_000, 303_000, None),
            "No cabe en 256 KB: lo más ligero pesó 303 KB. No se encontró una duración que quepa; prueba con otro tramo."
        );
    }

    #[test]
    fn every_core_error_has_its_kind() {
        use std::io;
        use std::time::Duration;

        let path = PathBuf::from("/tmp/x.mp4");
        let decoding = |error| ConversionError::Decoding(error);
        let cases: Vec<(ConversionError, UiErrorKind)> = vec![
            (
                ConversionError::Options(OptionsError::StartNotValid { value: -1.0 }),
                UiErrorKind::InvalidOptions,
            ),
            (
                ConversionError::Options(OptionsError::DoesNotFitEvenSpedUp {
                    available_s: 30.0,
                    max_s: 20.0,
                    crop_s: 10.0,
                    suggested_start_s: 10.0,
                }),
                UiErrorKind::DoesNotFitEvenSpedUp,
            ),
            (
                decoding(DecodingError::FfmpegNotFound {
                    program: "ffprobe".into(),
                }),
                UiErrorKind::FfmpegNotFound,
            ),
            (
                decoding(DecodingError::TimedOut {
                    program: "ffmpeg".into(),
                    limit: Duration::from_secs(120),
                }),
                UiErrorKind::TimedOut,
            ),
            (
                decoding(DecodingError::FileNotFound { path: path.clone() }),
                UiErrorKind::InvalidEntry,
            ),
            (
                decoding(DecodingError::NotRegularFile { path: path.clone() }),
                UiErrorKind::InvalidEntry,
            ),
            (
                decoding(DecodingError::UnreadableFile {
                    path,
                    source: io::Error::other("sin permiso"),
                }),
                UiErrorKind::InvalidEntry,
            ),
            (
                decoding(DecodingError::FileTooLarge {
                    bytes: 3,
                    max_bytes: 2,
                }),
                UiErrorKind::InvalidEntry,
            ),
            (
                decoding(DecodingError::FfmpegFailed {
                    program: "ffprobe".into(),
                    code: Some(1),
                    summarized_stderr: "Invalid data".into(),
                }),
                UiErrorKind::InvalidEntry,
            ),
            (
                decoding(DecodingError::WithoutVideoTrack),
                UiErrorKind::InvalidEntry,
            ),
            (
                decoding(DecodingError::UnreadableDuration { text: "N/A".into() }),
                UiErrorKind::InvalidEntry,
            ),
            (
                decoding(DecodingError::DurationNotValid { duration_s: 0.0 }),
                UiErrorKind::InvalidEntry,
            ),
            (
                decoding(DecodingError::DimensionsNotValid {
                    width: 0,
                    height: 0,
                    max: 16_384,
                }),
                UiErrorKind::InvalidEntry,
            ),
            (
                decoding(DecodingError::VideoWithoutFrames),
                UiErrorKind::InvalidOptions,
            ),
            (
                decoding(DecodingError::ProcessNotLaunched {
                    program: "ffmpeg".into(),
                    source: io::Error::other("x"),
                }),
                UiErrorKind::Other,
            ),
            (
                decoding(DecodingError::ThreadNotLaunched {
                    source: io::Error::other("x"),
                }),
                UiErrorKind::Other,
            ),
            (
                decoding(DecodingError::UnreadableOutput {
                    program: "ffmpeg".into(),
                    source: io::Error::other("x"),
                }),
                UiErrorKind::Other,
            ),
            (
                ConversionError::from(DecodingError::Cancelled),
                UiErrorKind::Cancelled,
            ),
            (ConversionError::Cancelled, UiErrorKind::Cancelled),
            (
                ConversionError::PreviewNotSaved(io::Error::other("x")),
                UiErrorKind::Other,
            ),
        ];
        for (error, kind) in cases {
            let text = error.to_string();
            let ui = UiError::from(error);
            assert_eq!(ui.kind, kind, "{text}");
            assert!(!ui.message.is_empty());
        }
    }

    #[test]
    fn does_not_fit_even_sped_up_is_explained_without_cli_options() {
        let ui = UiError::from(ConversionError::Options(
            OptionsError::DoesNotFitEvenSpedUp {
                available_s: 25.0,
                max_s: 20.0,
                crop_s: 5.0,
                suggested_start_s: 5.0,
            },
        ));
        assert_eq!(
            ui.message,
            "El tramo dura 25,0 s y, acelerando 2x, caben 20,0 s. Acórtalo al menos 5,0 s."
        );
    }

    #[test]
    #[cfg(unix)]
    fn session_launches_set_executables_and_not_the_path_ones() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let mark = directory.path().join("marca");
        let ffprobe = directory.path().join("libffprobe.so");
        fs::write(
            &ffprobe,
            format!("#!/bin/sh\necho lanzado > '{}'\nexit 1\n", mark.display()),
        )
        .unwrap();
        fs::set_permissions(&ffprobe, fs::Permissions::from_mode(0o755)).unwrap();
        let video = directory.path().join("video.mp4");
        fs::write(&video, b"no es un video").unwrap();

        let session = Session::default();
        session.set_executables(Executables::in_native_libraries(directory.path()));
        let entries = session.add_files(vec![video]);

        assert!(mark.exists(), "no se lanzó el ffprobe fijado");
        assert_ne!(entries[0].state, EntryState::Ready);
    }

    #[test]
    fn memory_limit_applies_before_searching_or_decoding() {
        let session = Session {
            limit_memory: true,
            ..Session::default()
        };
        let request = CreateRequest {
            priority: PriorityUi::Smoothness,
            fps: None,
            side_px: None,
            ..request(1, "discord-gif", 0.0, 30.0)
        };
        let error = session.create(&request, &no_progress).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidOptions);
        assert!(error.message.contains("caben como mucho"), "{error:?}");
        let error = session.estimate(&request).unwrap_err();
        assert!(error.message.contains("caben como mucho"), "{error:?}");

        let error = Session::default()
            .create(&request, &no_progress)
            .unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);
    }
}
