//! The session link queue: validates pasted text, downloads pending entries one at a time with
//! `yt-dlp` and stores each video in its entry together with its temp file.
//!
//! Neither links nor `yt-dlp` output are logged: both can carry personal data.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use conversor_core::{
    ConversionError, DownloadError, DownloadMode, DownloadProgress, DownloadedVideo, Link,
    SAVE_MAX_SIZE_BYTES, SAVE_TIME_LIMIT,
};

use super::{Entry, Session, inspect};
use crate::contract::{EntryState, EntryUi, MediaOrigin, Origin, QueueEvent, UiError, UiErrorKind};
use crate::direct_link::{self, DirectLinkError, MAX_SIZE_BYTES as MAX_DIRECT_SIZE_BYTES};

const MAX_LINKS_AT_ONCE: usize = 20;

const MAX_LINKS_IN_QUEUE: usize = 100;

const ANDROID_MAX_SIZE_BYTES: u64 = 200 * 1024 * 1024;

const ANDROID_TIME_LIMIT: Duration = Duration::from_secs(600);

pub(crate) type BridgeDownload = Box<
    dyn Fn(
            &Link,
            DownloadMode,
            &Path,
            &(dyn Fn(DownloadProgress) + Sync),
            &AtomicBool,
            Duration,
        ) -> Result<PathBuf, DownloadError>
        + Send
        + Sync,
>;

const MAX_LINKS_TEXT_BYTES: usize = 64 * 1024;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

impl Session {
    pub(crate) fn add_links(&self, text: &str) -> Result<Vec<EntryUi>, UiError> {
        let links = read_links(text)?;
        let mut state = self.state();
        let queued = state
            .entries
            .iter()
            .filter(|entry| entry.origin == Origin::Link)
            .count();
        if queued + links.len() > MAX_LINKS_IN_QUEUE {
            return Err(UiError::new(
                UiErrorKind::LinkNotValid,
                format!(
                    "caben como mucho {MAX_LINKS_IN_QUEUE} enlaces en la cola; quita alguno antes de añadir más"
                ),
            ));
        }
        Ok(links
            .into_iter()
            .map(|link| {
                let entry = Entry {
                    id: state.new_id(),
                    name: link.host().to_string(),
                    name_base: String::new(),
                    origin: Origin::Link,
                    state: EntryState::Pending,
                    source: Some(link.clone()),
                    link: Some(link),
                    downloaded: None,
                    video: None,
                    image: None,
                    pasted: None,
                    created: None,
                    media_origin: MediaOrigin::Own,
                };
                let ui = entry.ui();
                state.entries.push(entry);
                ui
            })
            .collect())
    }

    pub(crate) fn download_queue(&self, emit: &(dyn Fn(QueueEvent) + Sync)) {
        {
            let mut state = self.state();
            if state.downloading || state.closing {
                return;
            }
            state.downloading = true;
            self.downloads_cancellation.store(false, Ordering::Relaxed);
        }
        let _on_interrupt = OnInterrupt(self);
        while let Some((entry, link)) = self.start_next() {
            let id = entry.id;
            emit(QueueEvent::Change { entry });
            if let Some(entry) = self.download_one(id, &link, emit) {
                emit(QueueEvent::Change { entry });
            }
        }
        emit(QueueEvent::Finished);
    }

    pub(crate) fn cancel_downloads(&self) {
        self.downloads_cancellation.store(true, Ordering::Relaxed);
    }

    fn start_next(&self) -> Option<(EntryUi, Link)> {
        let mut state = self.state();
        let cancelled = self.downloads_cancellation.load(Ordering::Relaxed);
        if !cancelled {
            for entry in &mut state.entries {
                if entry.state == EntryState::Pending
                    && let Some(link) = entry.link.take()
                {
                    entry.state = EntryState::Downloading {
                        bytes: 0,
                        total: None,
                    };
                    return Some((entry.ui(), link));
                }
            }
        }
        state.downloading = false;
        self.stopped_jobs.notify_all();
        None
    }

    fn download_one(
        &self,
        id: u32,
        link: &Link,
        emit: &(dyn Fn(QueueEvent) + Sync),
    ) -> Option<EntryUi> {
        let last_notice = Mutex::new(None::<Instant>);
        let progress = |advance: DownloadProgress| {
            let DownloadProgress::Downloading { bytes, total } = advance;
            {
                let mut last = last_notice.lock().unwrap_or_else(PoisonError::into_inner);
                if last.is_some_and(|moment| moment.elapsed() < PROGRESS_INTERVAL) {
                    return;
                }
                *last = Some(Instant::now());
            }
            let change = self.apply_change(id, |entry| {
                entry.state = EntryState::Downloading { bytes, total };
            });
            if let Some(entry) = change {
                emit(QueueEvent::Change { entry });
            }
        };
        let downloaded = match self.download_link(link, DownloadMode::Sticker, &progress) {
            Ok(downloaded) => downloaded,
            Err(error) => return self.mark_error(id, UiError::from(error)),
        };

        let path = downloaded.path().to_path_buf();
        let name = downloaded.name_base().to_string();
        self.apply_change(id, move |entry| {
            entry.name.clone_from(&name);
            entry.name_base = name;
            entry.downloaded = Some(downloaded);
        })?;
        match inspect(&path, &self.inspection_cancellation, &self.executables()) {
            Ok(video) => self.apply_change(id, |entry| {
                entry.state = EntryState::Ready;
                entry.video = Some(video);
            }),
            Err(error) => self.mark_error(id, UiError::from(ConversionError::from(error))),
        }
    }

    fn download_link(
        &self,
        link: &Link,
        mode: DownloadMode,
        progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> Result<DownloadedVideo, DownloadError> {
        if mode == DownloadMode::Sticker && direct_link::is_direct_link(link.as_text()) {
            return DownloadedVideo::from_own_download(MAX_DIRECT_SIZE_BYTES, |temp| {
                direct_link::download_direct(link.as_text(), temp, &self.downloads_cancellation)
                    .map_err(direct_link_error)
            });
        }
        let Some(bridge) = self.bridge_download.get() else {
            return match mode {
                DownloadMode::Sticker => {
                    self.downloader
                        .download(link, progress, &self.downloads_cancellation)
                }
                DownloadMode::Save => self.downloader.clone().for_saving().download(
                    link,
                    progress,
                    &self.downloads_cancellation,
                ),
            };
        };
        let (max_size_bytes, time_limit) = match mode {
            DownloadMode::Sticker => (ANDROID_MAX_SIZE_BYTES, ANDROID_TIME_LIMIT),
            DownloadMode::Save => (SAVE_MAX_SIZE_BYTES, SAVE_TIME_LIMIT),
        };
        DownloadedVideo::from_own_download(max_size_bytes, |temp| {
            bridge(
                link,
                mode,
                temp,
                progress,
                &self.downloads_cancellation,
                time_limit,
            )
        })
    }

    /// The video of entry `id` with its audio, ready to be saved. A video that came from a link
    /// is downloaded again in save mode (the one used for stickers has no audio and is smaller);
    /// a direct link, a gallery file or a pasted one is already whole.
    pub(crate) fn video_to_save(
        &self,
        id: u32,
        progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> Result<VideoToSave, UiError> {
        let busy = |message: &str| UiError::new(UiErrorKind::InvalidEntry, message);
        let link = {
            let mut state = self.state();
            let entry = state.entry(id)?;
            if entry.state != EntryState::Ready {
                return Err(busy("el video todavía no está listo para guardarse"));
            }
            let Some(video) = &entry.video else {
                return Err(busy("solo se pueden guardar videos"));
            };
            let again = entry
                .source
                .clone()
                .filter(|link| !direct_link::is_direct_link(link.as_text()));
            let Some(link) = again else {
                return Ok(VideoToSave::existing(&video.path, entry));
            };
            if state.closing {
                return Err(busy("la app se está cerrando"));
            }
            if state.downloading {
                return Err(busy("espera a que terminen las descargas para guardar"));
            }
            state.downloading = true;
            self.downloads_cancellation.store(false, Ordering::Relaxed);
            link
        };
        let _saving = Saving(self);
        // Some sites (TikTok) answer HTTP 403 the first time and work on the second: retry once,
        // but never after a cancellation.
        let progress = throttled(progress);
        let downloaded = match self.download_link(&link, DownloadMode::Save, &progress) {
            Err(error) if !matches!(error, DownloadError::Cancelled) => {
                tracing::warn!(%error, "the download with audio failed, retrying once");
                self.download_link(&link, DownloadMode::Save, &progress)
            }
            result => result,
        }
        .map_err(UiError::from)?;
        Ok(VideoToSave::downloaded(downloaded))
    }

    /// The audio of `[start_s, start_s + seconds)` of entry `id`, copied without re-encoding into
    /// a temp file ready to be saved (`seconds` is capped to the longest sound a sticker takes).
    pub(crate) fn sound_to_save(
        &self,
        id: u32,
        start_s: f64,
        seconds: f64,
        progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> Result<CutToSave, UiError> {
        check_cut(start_s, seconds, "el tramo del sonido no es válido")?;
        let video = self.video_to_save(id, progress)?;
        let temp = tempfile::Builder::new()
            .prefix("sonido-")
            .tempdir()
            .map_err(|error| {
                UiError::new(
                    UiErrorKind::Other,
                    format!("no se pudo preparar la carpeta del sonido: {error}"),
                )
            })?;
        let path = self
            .executables()
            .decoder(video.path())
            .cut_audio(
                start_s,
                seconds,
                &temp.path().join("sonido"),
                &AtomicBool::new(false),
            )
            .map_err(|error| UiError::from(ConversionError::from(error)))?;
        let base = Path::new(video.name())
            .file_stem()
            .map_or_else(|| "video".into(), |stem| stem.to_string_lossy());
        let extension = path
            .extension()
            .map_or_else(|| "m4a".into(), |extension| extension.to_string_lossy());
        Ok(CutToSave {
            name: format!("{base}-sonido.{extension}"),
            path,
            _temp: temp,
        })
    }

    /// The clip `[start_s, start_s + seconds)` of entry `id` with its audio, copied without
    /// re-encoding into a temp MP4 ready to be saved (`seconds` is capped to 10 s).
    pub(crate) fn clip_video_to_save(
        &self,
        id: u32,
        start_s: f64,
        seconds: f64,
        progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> Result<CutToSave, UiError> {
        check_cut(start_s, seconds, "el tramo del vídeo no es válido")?;
        let video = self.video_to_save(id, progress)?;
        let temp = tempfile::Builder::new()
            .prefix("recorte-")
            .tempdir()
            .map_err(|error| {
                UiError::new(
                    UiErrorKind::Other,
                    format!("no se pudo preparar la carpeta del recorte: {error}"),
                )
            })?;
        let base = Path::new(video.name())
            .file_stem()
            .map_or_else(|| "video".into(), |stem| stem.to_string_lossy());
        let name = format!("{base}-sticker.mp4");
        let path = temp.path().join("recorte.mp4");
        self.executables()
            .decoder(video.path())
            .cut_video(start_s, seconds, &path, &AtomicBool::new(false))
            .map_err(|error| UiError::from(ConversionError::from(error)))?;
        Ok(CutToSave {
            name,
            path,
            _temp: temp,
        })
    }

    fn mark_error(&self, id: u32, error: UiError) -> Option<EntryUi> {
        self.apply_change(id, |entry| {
            entry.state = EntryState::Error { error };
            entry.downloaded = None;
        })
    }

    fn apply_change(&self, id: u32, change: impl FnOnce(&mut Entry)) -> Option<EntryUi> {
        let mut state = self.state();
        let entry = state.entries.iter_mut().find(|entry| entry.id == id)?;
        change(entry);
        Some(entry.ui())
    }
}

/// Marks that a save-mode download is using the downloader, so the queue does not start another.
struct Saving<'a>(&'a Session);

impl Drop for Saving<'_> {
    fn drop(&mut self) {
        self.0.state().downloading = false;
        self.0.stopped_jobs.notify_all();
    }
}

/// A video that is ready to save and, if it was downloaded for that, the temp that keeps it.
pub(crate) struct VideoToSave {
    path: PathBuf,
    name: String,
    _downloaded: Option<DownloadedVideo>,
}

impl VideoToSave {
    fn existing(path: &Path, entry: &Entry) -> Self {
        let base = if entry.name_base.is_empty() {
            Path::new(&entry.name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            entry.name_base.clone()
        };
        VideoToSave {
            path: path.to_path_buf(),
            name: file_name(&base, path),
            _downloaded: None,
        }
    }

    fn downloaded(video: DownloadedVideo) -> Self {
        VideoToSave {
            path: video.path().to_path_buf(),
            name: file_name(video.name_base(), video.path()),
            _downloaded: Some(video),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

fn check_cut(start_s: f64, seconds: f64, message: &str) -> Result<(), UiError> {
    if start_s.is_finite() && start_s >= 0.0 && seconds.is_finite() && seconds > 0.0 {
        return Ok(());
    }
    Err(UiError::new(UiErrorKind::InvalidOptions, message))
}

/// A cut of audio or of video in a temp folder that goes away with this value.
pub(crate) struct CutToSave {
    path: PathBuf,
    name: String,
    _temp: tempfile::TempDir,
}

impl CutToSave {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

fn file_name(base: &str, path: &Path) -> String {
    let base = if base.is_empty() { "video" } else { base };
    let extension = path
        .extension()
        .map_or_else(|| "mp4".into(), |extension| extension.to_string_lossy());
    format!("{base}.{extension}")
}

/// Lets through at most one progress notice every `PROGRESS_INTERVAL`.
fn throttled<'a>(
    progress: &'a (dyn Fn(DownloadProgress) + Sync),
) -> impl Fn(DownloadProgress) + Sync + 'a {
    let last_notice = Mutex::new(None::<Instant>);
    move |advance| {
        let mut last = last_notice.lock().unwrap_or_else(PoisonError::into_inner);
        if last.is_some_and(|moment| moment.elapsed() < PROGRESS_INTERVAL) {
            return;
        }
        *last = Some(Instant::now());
        drop(last);
        progress(advance);
    }
}

struct OnInterrupt<'a>(&'a Session);

impl Drop for OnInterrupt<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let mut state = self.0.state();
            for entry in &mut state.entries {
                if matches!(entry.state, EntryState::Downloading { .. }) {
                    entry.state = EntryState::Error {
                        error: UiError::new(
                            UiErrorKind::Other,
                            "la descarga se interrumpió por un fallo de la app; quítala y vuelve a pegar el enlace",
                        ),
                    };
                }
            }
            state.downloading = false;
            drop(state);
            self.0.stopped_jobs.notify_all();
        }
    }
}

fn direct_link_error(error: DirectLinkError) -> DownloadError {
    match error {
        DirectLinkError::Cancelled => DownloadError::Cancelled,
        DirectLinkError::TooLarge => DownloadError::FileTooLarge {
            max_bytes: MAX_DIRECT_SIZE_BYTES,
        },
        other => DownloadError::FailedOwnDownload {
            reason: other.to_string(),
        },
    }
}

fn read_links(text: &str) -> Result<Vec<Link>, UiError> {
    let not_valid = |message: String| UiError::new(UiErrorKind::LinkNotValid, message);
    if text.len() > MAX_LINKS_TEXT_BYTES {
        return Err(not_valid(
            "el texto pasa de 64 KB; pega menos enlaces de una vez".to_string(),
        ));
    }
    let chunks = || {
        text.lines()
            .enumerate()
            .flat_map(|(index, line)| line.split_whitespace().map(move |chunk| (index + 1, chunk)))
    };
    let how_many = chunks().take(MAX_LINKS_AT_ONCE + 1).count();
    if how_many == 0 {
        return Err(not_valid("pega al menos un enlace".to_string()));
    }
    if how_many > MAX_LINKS_AT_ONCE {
        return Err(not_valid(format!(
            "se admiten como mucho {MAX_LINKS_AT_ONCE} enlaces de una vez"
        )));
    }
    chunks()
        .map(|(line, chunk)| {
            Link::new(chunk).map_err(|error| not_valid(format!("línea {line}: {error}")))
        })
        .collect()
}

impl From<DownloadError> for UiError {
    fn from(error: DownloadError) -> Self {
        let kind = match &error {
            DownloadError::FailedDownload {
                code,
                summarized_stderr,
            } => {
                let process_end = code.map_or_else(
                    || "terminado por una señal".to_string(),
                    |code| format!("código {code}"),
                );
                return UiError::new(
                    UiErrorKind::FailedDownload,
                    format!(
                        "yt-dlp no pudo descargar el video ({process_end}): {summarized_stderr}"
                    ),
                );
            }
            DownloadError::LinkNotValid { .. } => UiErrorKind::LinkNotValid,
            DownloadError::YtDlpNotFound { .. } => UiErrorKind::YtDlpNotFound,
            DownloadError::FileNotLocated { .. } | DownloadError::FailedOwnDownload { .. } => {
                UiErrorKind::FailedDownload
            }
            DownloadError::TimedOut { .. } => UiErrorKind::TimedOut,
            DownloadError::FileTooLarge { .. } => UiErrorKind::InvalidEntry,
            DownloadError::Cancelled => UiErrorKind::Cancelled,
            DownloadError::ProcessNotLaunched { .. }
            | DownloadError::ThreadNotLaunched { .. }
            | DownloadError::TempNotCreated(_) => UiErrorKind::Other,
        };
        UiError::new(kind, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use conversor_core::{DownloadError, YtDlpDownloader};
    use tempfile::TempDir;

    use super::*;
    use crate::contract::{
        CreateRequest, EntryState, FramingSpecUi, MediaUi, Origin, PriorityUi, ProgressUi,
        QueueEvent, UiErrorKind,
    };

    const DEADLINE: Duration = Duration::from_secs(5);

    struct FalseYtDlp {
        directory: TempDir,
    }

    impl FalseYtDlp {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let data = directory.path();
            let output = Command::new("ffmpeg")
                .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
                .args([
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=160x120:rate=15",
                    "-t",
                    "2",
                ])
                .args(["-c:v", "mpeg4", "-pix_fmt", "yuv420p"])
                .arg(data.join("video.mp4"))
                .output()
                .expect("hace falta ffmpeg para estas pruebas");
            assert!(output.status.success());
            let script = format!(
                r#"#!/bin/sh
DATOS='{data}'
DIR=''
previo=''
for arg in "$@"; do
  if [ "$previo" = '-P' ]; then DIR="$arg"; fi
  previo="$arg"
done
echo "$previo" >> "$DATOS/enlaces"
echo "$DIR" >> "$DATOS/temporales"
echo 'conversor-progreso 10 100 NA'
case "$previo" in
  *falla*)
    echo "ERROR: [generic] no se pudo abrir $previo" >&2
    exit 1 ;;
  *espera*)
    sleep 30 &
    echo $! > "$DATOS/hijo"
    echo $$ > "$DATOS/padre"
    wait ;;
  *no-es-video*)
    echo texto > "$DIR/pagina.mp4"
    echo "$DIR/pagina.mp4" ;;
  *con-titulo*)
    cp "$DATOS/video.mp4" "$DIR/Titulo_secreto.mp4"
    echo "$DIR/Titulo_secreto.mp4" ;;
  *)
    cp "$DATOS/video.mp4" "$DIR/Gato_bailando.mp4"
    echo 'conversor-progreso 100 100 NA'
    echo "$DIR/Gato_bailando.mp4" ;;
esac
"#,
                data = data.display()
            );
            fs::write(data.join("yt-dlp.sh"), script).unwrap();
            let state = Command::new("install")
                .args(["-m", "755"])
                .arg(data.join("yt-dlp.sh"))
                .arg(data.join("yt-dlp"))
                .status()
                .unwrap();
            assert!(state.success());
            FalseYtDlp { directory }
        }

        fn session(&self) -> Session {
            Session::with_downloader(
                YtDlpDownloader::new().with_executable(self.directory.path().join("yt-dlp")),
            )
        }

        fn lines(&self, name: &str) -> Vec<String> {
            fs::read_to_string(self.directory.path().join(name))
                .map(|text| text.lines().map(str::to_string).collect())
                .unwrap_or_default()
        }

        fn temp_files_left(&self) -> Vec<String> {
            let mut temp_files = self.lines("temporales");
            temp_files.retain(|temp| Path::new(temp).exists());
            temp_files
        }

        fn processes(&self) -> [String; 2] {
            let start = Instant::now();
            loop {
                if let ([parent], [child]) = (&self.lines("padre")[..], &self.lines("hijo")[..]) {
                    return [parent.clone(), child.clone()];
                }
                assert!(start.elapsed() < DEADLINE, "the fake yt-dlp did not start");
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn alive_process(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    fn die(pids: &[String]) -> bool {
        let start = Instant::now();
        while pids.iter().any(|pid| alive_process(pid)) {
            if start.elapsed() >= DEADLINE {
                return false;
            }
            thread::sleep(Duration::from_millis(20));
        }
        true
    }

    fn download(session: &Session) -> Vec<QueueEvent> {
        let events = Mutex::new(Vec::new());
        session.download_queue(&|event| events.lock().unwrap().push(event));
        events.into_inner().unwrap()
    }

    fn download_separately(session: &Arc<Session>) -> mpsc::Receiver<Vec<QueueEvent>> {
        let (end, finished) = mpsc::channel();
        let session = Arc::clone(session);
        thread::spawn(move || end.send(download(&session)).ok());
        finished
    }

    fn on_finish(finished: &mpsc::Receiver<Vec<QueueEvent>>) -> Vec<QueueEvent> {
        finished
            .recv_timeout(DEADLINE)
            .expect("la cola no terminó dentro del plazo")
    }

    fn states(events: &[QueueEvent], id: u32) -> Vec<EntryState> {
        events
            .iter()
            .filter_map(|event| match event {
                QueueEvent::Change { entry } if entry.id == id => Some(entry.state.clone()),
                _ => None,
            })
            .collect()
    }

    fn error_kind(state: &EntryState) -> Option<UiErrorKind> {
        match state {
            EntryState::Error { error } => Some(error.kind),
            _ => None,
        }
    }

    fn request(id: u32) -> CreateRequest {
        CreateRequest {
            id,
            preset: "discord-emoji".to_string(),
            start_s: 0.0,
            seconds: 1.0,
            speed: 1.0,
            priority: PriorityUi::Balance,
            fps: None,
            side_px: None,
            framing: FramingSpecUi::default(),
        }
    }

    #[test]
    fn queue_downloads_in_order_and_emits_downloading_and_ready() {
        let fake = FalseYtDlp::new();
        let session = fake.session();
        session
            .add_links("https://a.example/1 https://b.example/2")
            .unwrap();

        let events = download(&session);

        assert_eq!(
            fake.lines("enlaces"),
            ["https://a.example/1", "https://b.example/2"]
        );
        for id in [1, 2] {
            let states = states(&events, id);
            assert_eq!(
                states[0],
                EntryState::Downloading {
                    bytes: 0,
                    total: None
                }
            );
            assert_eq!(
                states[1],
                EntryState::Downloading {
                    bytes: 10,
                    total: Some(100)
                }
            );
            assert_eq!(states.last(), Some(&EntryState::Ready));
        }
        let order: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                QueueEvent::Change { entry } => Some(entry.id),
                QueueEvent::Finished => None,
            })
            .collect();
        assert!(order.is_sorted(), "{order:?}");
        assert_eq!(events.last(), Some(&QueueEvent::Finished));

        let state = session.state();
        let entry = state.entry(1).unwrap();
        assert_eq!(entry.name, "Gato_bailando");
        assert_eq!(entry.name_base, "Gato_bailando");
        assert!(entry.video.as_ref().unwrap().path.is_file());
        let Some(MediaUi::Video { width, height, .. }) = entry.ui().media else {
            panic!("{:?}", entry.ui());
        };
        assert_eq!((width, height), (160, 120));
        assert!(entry.link.is_none(), "el enlace ya no hace falta");
    }

    #[test]
    fn failure_leaves_that_entry_with_error_and_continues_with_next() {
        let fake = FalseYtDlp::new();
        let session = fake.session();
        session
            .add_links("https://a.example/falla https://b.example/no-es-video https://c.example/3")
            .unwrap();

        let events = download(&session);

        let last = |id| states(&events, id).pop().unwrap();
        assert_eq!(error_kind(&last(1)), Some(UiErrorKind::FailedDownload));
        assert_eq!(error_kind(&last(2)), Some(UiErrorKind::InvalidEntry));
        assert_eq!(last(3), EntryState::Ready);
        assert_eq!(fake.temp_files_left().len(), 1);
    }

    fn bridge_video(temp: &Path) -> PathBuf {
        let path = temp.join("Mi cumple.mp4");
        let state = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc=size=64x64:rate=10:duration=1"])
            .args(["-pix_fmt", "yuv420p"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(state.success());
        path
    }

    #[test]
    fn on_android_the_bridge_downloads_not_the_program() {
        let directory = tempfile::tempdir().unwrap();
        let session = Session::with_downloader(
            YtDlpDownloader::new().with_executable(directory.path().join("no-existe")),
        );
        session.set_bridge_download(Box::new(|_, _, temp, progress, _, limit| {
            assert_eq!(limit, ANDROID_TIME_LIMIT);
            progress(DownloadProgress::Downloading {
                bytes: 5,
                total: Some(10),
            });
            Ok(bridge_video(temp))
        }));
        session.add_links("https://a.example/1").unwrap();

        let events = download(&session);

        let last = states(&events, 1).pop().unwrap();
        assert_eq!(last, EntryState::Ready, "{events:?}");
        assert!(states(&events, 1).contains(&EntryState::Downloading {
            bytes: 5,
            total: Some(10)
        }));
        let Some(entry) = events.iter().rev().find_map(|event| match event {
            QueueEvent::Change { entry } => Some(entry),
            QueueEvent::Finished => None,
        }) else {
            panic!("{events:?}");
        };
        assert_eq!(entry.name, "Mi_cumple");
    }

    fn session_with_downloaded_entry(
        modes: &Arc<Mutex<Vec<DownloadMode>>>,
        result: fn(&Path) -> Result<PathBuf, DownloadError>,
    ) -> Session {
        let session = Session::default();
        let modes = Arc::clone(modes);
        session.set_bridge_download(Box::new(move |_, mode, temp, _, _, _| {
            modes.lock().unwrap().push(mode);
            result(temp)
        }));
        session.add_links("https://a.example/1").unwrap();
        let events = download(&session);
        assert_eq!(states(&events, 1).pop(), Some(EntryState::Ready));
        session
    }

    #[test]
    fn saving_a_downloaded_video_downloads_it_again_with_audio() {
        let modes = Arc::new(Mutex::new(Vec::new()));
        let session = session_with_downloaded_entry(&modes, |temp| Ok(bridge_video(temp)));
        let notices = Mutex::new(Vec::new());

        let video = session
            .video_to_save(1, &|progress| notices.lock().unwrap().push(progress))
            .unwrap();

        assert_eq!(
            *modes.lock().unwrap(),
            vec![DownloadMode::Sticker, DownloadMode::Save]
        );
        assert_eq!(video.name(), "Mi_cumple.mp4");
        assert!(video.path().is_file());
        assert!(!session.state().downloading, "la cola queda libre");
        assert_eq!(session.state().entry(1).unwrap().state, EntryState::Ready);
    }

    #[test]
    fn a_sound_needs_a_valid_range_before_anything_is_downloaded() {
        let modes = Arc::new(Mutex::new(Vec::new()));
        let session = session_with_downloaded_entry(&modes, |temp| Ok(bridge_video(temp)));

        for (start_s, seconds) in [
            (-1.0, 2.0),
            (0.0, 0.0),
            (f64::NAN, 2.0),
            (0.0, f64::INFINITY),
        ] {
            let Err(error) = session.sound_to_save(1, start_s, seconds, &|_| {}) else {
                panic!("el tramo {start_s} + {seconds} no es válido");
            };
            assert_eq!(error.kind, UiErrorKind::InvalidOptions);
        }
        assert_eq!(modes.lock().unwrap().len(), 1, "no descargó nada más");
    }

    #[test]
    fn a_failed_save_download_gives_its_error_and_frees_the_queue() {
        let modes = Arc::new(Mutex::new(Vec::new()));
        let session = Session::default();
        let calls = Arc::clone(&modes);
        session.set_bridge_download(Box::new(move |_, mode, temp, _, _, _| {
            let mut calls = calls.lock().unwrap();
            calls.push(mode);
            if mode == DownloadMode::Save {
                return Err(DownloadError::Cancelled);
            }
            Ok(bridge_video(temp))
        }));
        session.add_links("https://a.example/1").unwrap();
        download(&session);

        let Err(error) = session.video_to_save(1, &|_| {}) else {
            panic!("tenía que fallar");
        };

        assert_eq!(error.kind, UiErrorKind::Cancelled);
        assert!(!session.state().downloading);
    }

    #[test]
    fn a_failed_save_download_is_retried_once() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let session = Session::default();
        let seen = Arc::clone(&calls);
        session.set_bridge_download(Box::new(move |_, mode, temp, _, _, _| {
            let mut seen = seen.lock().unwrap();
            seen.push(mode);
            if mode == DownloadMode::Save && seen.len() == 2 {
                return Err(DownloadError::FailedOwnDownload {
                    reason: "HTTP 403".to_string(),
                });
            }
            Ok(bridge_video(temp))
        }));
        session.add_links("https://a.example/1").unwrap();
        download(&session);

        assert!(session.video_to_save(1, &|_| {}).is_ok());
        assert_eq!(calls.lock().unwrap().len(), 3, "sticker, fallo y reintento");
    }

    #[test]
    fn a_save_download_that_fails_twice_shows_the_error_after_one_retry() {
        let calls = Arc::new(Mutex::new(0));
        let session = Session::default();
        let seen = Arc::clone(&calls);
        session.set_bridge_download(Box::new(move |_, mode, temp, _, _, _| {
            if mode == DownloadMode::Save {
                *seen.lock().unwrap() += 1;
                return Err(DownloadError::FailedOwnDownload {
                    reason: "HTTP 403".to_string(),
                });
            }
            Ok(bridge_video(temp))
        }));
        session.add_links("https://a.example/1").unwrap();
        download(&session);

        let Err(error) = session.video_to_save(1, &|_| {}) else {
            panic!("tenía que fallar");
        };

        assert!(error.message.contains("403"));
        assert_eq!(*calls.lock().unwrap(), 2);
    }

    #[test]
    fn saving_waits_for_the_queue_and_needs_a_ready_video() {
        let modes = Arc::new(Mutex::new(Vec::new()));
        let session = session_with_downloaded_entry(&modes, |temp| Ok(bridge_video(temp)));
        session.state().downloading = true;
        let Err(busy) = session.video_to_save(1, &|_| {}) else {
            panic!("tenía que esperar");
        };
        assert_eq!(busy.kind, UiErrorKind::InvalidEntry);
        assert!(busy.message.contains("espera"));
        session.state().downloading = false;

        session.add_links("https://b.example/2").unwrap();
        let Err(pending) = session.video_to_save(2, &|_| {}) else {
            panic!("no estaba listo");
        };
        assert!(pending.message.contains("no está listo"));
        assert!(session.video_to_save(99, &|_| {}).is_err());
        assert_eq!(modes.lock().unwrap().len(), 1, "no descargó nada más");
    }

    #[test]
    fn on_android_bridge_failure_leaves_entry_with_its_error() {
        let session = Session::default();
        session.set_bridge_download(Box::new(|_, _, temp, _, _, _| {
            std::fs::write(temp.join("a.part"), b"a medias").unwrap();
            Err(DownloadError::FailedOwnDownload {
                reason: "Este sitio no se pudo descargar: Unsupported URL".to_string(),
            })
        }));
        session.add_links("https://a.example/1").unwrap();

        let events = download(&session);

        let Some(EntryState::Error { error }) = states(&events, 1).pop() else {
            panic!("{events:?}");
        };
        assert_eq!(error.kind, UiErrorKind::FailedDownload);
        assert!(error.message.starts_with("Este sitio no se pudo descargar"));
    }

    #[test]
    fn direct_link_errors_arrive_as_download_errors() {
        assert!(matches!(
            direct_link_error(DirectLinkError::Cancelled),
            DownloadError::Cancelled
        ));
        assert!(matches!(
            direct_link_error(DirectLinkError::TooLarge),
            DownloadError::FileTooLarge { .. }
        ));
        let other = direct_link_error(DirectLinkError::DisallowedContent);
        assert_eq!(UiError::from(other).kind, UiErrorKind::FailedDownload);
    }

    #[test]
    fn without_yt_dlp_entry_says_how_to_install_it() {
        let directory = tempfile::tempdir().unwrap();
        let session = Session::with_downloader(
            YtDlpDownloader::new().with_executable(directory.path().join("no-existe")),
        );
        session.add_links("https://a.example/1").unwrap();

        let events = download(&session);

        let Some(EntryState::Error { error }) = states(&events, 1).pop() else {
            panic!("{events:?}");
        };
        assert_eq!(error.kind, UiErrorKind::YtDlpNotFound);
        assert!(
            error.message.contains("instala yt-dlp"),
            "{}",
            error.message
        );
    }

    #[test]
    fn cancel_leaves_current_cancelled_rest_pending_and_can_resume() {
        let fake = FalseYtDlp::new();
        let session = Arc::new(fake.session());
        session
            .add_links("https://a.example/espera https://b.example/2")
            .unwrap();

        let queue = download_separately(&session);
        let pids = fake.processes();
        session.cancel_downloads();
        let events = on_finish(&queue);
        assert!(die(&pids), "sigue vivo yt-dlp o su hijo");

        let last = states(&events, 1).pop().unwrap();
        assert_eq!(error_kind(&last), Some(UiErrorKind::Cancelled));
        assert!(states(&events, 2).is_empty());
        assert_eq!(session.state().entry(2).unwrap().state, EntryState::Pending);
        assert_eq!(events.last(), Some(&QueueEvent::Finished));
        assert!(fake.temp_files_left().is_empty());

        let events = download(&session);
        assert_eq!(states(&events, 2).pop(), Some(EntryState::Ready));
        assert!(states(&events, 1).is_empty());
    }

    #[test]
    fn with_queue_running_another_call_does_nothing() {
        let fake = FalseYtDlp::new();
        let session = Arc::new(fake.session());
        session.add_links("https://a.example/espera").unwrap();

        let queue = download_separately(&session);
        fake.processes();
        assert!(on_finish(&download_separately(&session)).is_empty());
        session.cancel_downloads();
        on_finish(&queue);
        assert_eq!(fake.lines("enlaces").len(), 1);
    }

    #[test]
    fn what_is_added_while_queue_runs_is_also_downloaded() {
        let fake = FalseYtDlp::new();
        let session = fake.session();
        session.add_links("https://a.example/1").unwrap();
        let added = Mutex::new(false);

        session.download_queue(&|_| {
            let mut added = added.lock().unwrap();
            if !*added {
                *added = true;
                session.add_links("https://b.example/2").unwrap();
            }
        });

        assert_eq!(fake.lines("enlaces").len(), 2);
        assert_eq!(session.state().entry(2).unwrap().state, EntryState::Ready);
    }

    #[test]
    fn panic_midway_leaves_entry_with_error_and_queue_usable() {
        let fake = FalseYtDlp::new();
        let session = fake.session();
        session
            .add_links("https://a.example/1 https://b.example/2")
            .unwrap();

        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            session.download_queue(&|event| {
                if matches!(event, QueueEvent::Change { .. }) {
                    panic!("fallo a propósito");
                }
            });
        }));

        assert!(interrupted.is_err());
        let state = session.state().entry(1).unwrap().state.clone();
        assert_eq!(error_kind(&state), Some(UiErrorKind::Other), "{state:?}");
        assert!(session.remove_entry(1).is_ok(), "no se puede quitar");
        let events = download(&session);
        assert_eq!(states(&events, 2).pop(), Some(EntryState::Ready));
    }

    #[test]
    fn removing_downloaded_returns_its_video_and_dropping_deletes_it() {
        let fake = FalseYtDlp::new();
        let session = fake.session();
        session.add_links("https://a.example/1").unwrap();
        download(&session);
        let path = session.state().entries[0]
            .video
            .as_ref()
            .unwrap()
            .path
            .clone();

        let deleted = session.remove_entry(1).unwrap().expect("tenía temporal");

        assert!(path.exists(), "se borró con el candado cogido");
        drop(deleted);
        assert!(!path.exists());
        assert!(fake.temp_files_left().is_empty());
    }

    #[test]
    fn close_waits_for_deletions_in_progress() {
        let fake = FalseYtDlp::new();
        let session = fake.session();
        session.add_links("https://a.example/1").unwrap();
        download(&session);
        let deleted = session.remove_entry(1).unwrap().expect("tenía temporal");
        let (closed, on_close) = mpsc::channel();

        thread::scope(|threads| {
            let session = &session;
            threads.spawn(move || {
                session.close();
                closed.send(()).unwrap();
            });
            let before_delete = on_close.recv_timeout(Duration::from_millis(300));
            drop(deleted);
            assert!(before_delete.is_err(), "cerrar no esperó al borrado");
            on_close.recv_timeout(DEADLINE).unwrap();
        });
        assert!(fake.temp_files_left().is_empty());
    }

    #[test]
    fn pending_one_can_be_removed_and_is_not_downloaded() {
        let fake = FalseYtDlp::new();
        let session = Arc::new(fake.session());
        session
            .add_links("https://a.example/espera https://b.example/2")
            .unwrap();

        let queue = download_separately(&session);
        fake.processes();
        let taken = session.remove_entry(1).unwrap_err();
        assert_eq!(taken.kind, UiErrorKind::Busy);
        assert!(session.remove_entry(2).unwrap().is_none());
        session.cancel_downloads();
        on_finish(&queue);
        assert!(
            download(&session)
                .iter()
                .all(|e| *e == QueueEvent::Finished)
        );
        assert_eq!(fake.lines("enlaces").len(), 1);
    }

    #[test]
    fn download_and_convert_at_once_with_separate_cancellations() {
        let fake = FalseYtDlp::new();
        let session = Arc::new(fake.session());
        let file = fake.directory.path().join("video.mp4");
        let local = session.add_files(vec![file])[0].id;
        session.add_links("https://a.example/espera").unwrap();

        let (end, finished) = mpsc::channel();
        let queue = Arc::clone(&session);
        thread::spawn(move || end.send(download(&queue)).unwrap());
        let pids = fake.processes();

        let created = session
            .create(&request(local), &|_: ProgressUi| {})
            .unwrap();
        assert!(created.meets_limit);
        session.cancel_creation();
        assert!(
            alive_process(&pids[0]),
            "cancelar la creación paró la descarga"
        );

        session.cancel_downloads();
        finished.recv_timeout(DEADLINE).unwrap();
        assert!(die(&pids));
    }

    #[test]
    fn three_links_give_three_pending_with_host_as_name() {
        let session = Session::default();
        let entries = session
            .add_links(
                " https://a.example/v?token=secreto\n\nhttp://b.example/x  https://c.example/y\r\n",
            )
            .unwrap();

        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a.example", "b.example", "c.example"]);
        let ids: Vec<_> = entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, [1, 2, 3]);
        for entry in &entries {
            assert_eq!(entry.origin, Origin::Link);
            assert_eq!(entry.state, EntryState::Pending);
            assert_eq!(entry.media, None);
        }
        assert_eq!(session.state().entries.len(), 3);
    }

    #[test]
    fn invalid_link_adds_none_and_names_the_line_without_repeating_it() {
        let session = Session::default();
        for (text, line) in [
            (
                "https://a.example/1\nftp://b.example/secreto\nhttps://c.example/3",
                2,
            ),
            ("https://a.example/1 ftp://b.example/secreto", 1),
            ("\n\n  hola", 3),
            ("https://usuario:secreto@a.example/", 1),
        ] {
            let error = session.add_links(text).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::LinkNotValid, "{text:?}");
            assert!(
                error.message.starts_with(&format!("línea {line}: ")),
                "{}",
                error.message
            );
            assert!(!error.message.contains("secreto"), "{}", error.message);
        }
        assert!(session.state().entries.is_empty());
    }

    #[test]
    fn accepts_exactly_20_links_and_64_kb_and_rejects_one_more() {
        let links = |how_many: usize| "https://a.example/v\n".repeat(how_many);
        let session = Session::default();
        assert_eq!(session.add_links(&links(20)).unwrap().len(), 20);
        let error = session.add_links(&links(21)).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::LinkNotValid);
        assert!(error.message.contains("20"), "{}", error.message);

        let exact = format!("https://a.example/v{}", " ".repeat(65_536 - 19));
        assert_eq!(exact.len(), 65_536);
        assert_eq!(session.add_links(&exact).unwrap().len(), 1);
        let error = session.add_links(&format!("{exact} ")).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::LinkNotValid);
        assert!(error.message.contains("64 KB"), "{}", error.message);
        assert_eq!(session.state().entries.len(), 21);
    }

    #[test]
    fn accepts_100_links_in_queue_and_rejects_one_more() {
        let session = Session::default();
        let ten = "https://a.example/v\n".repeat(10);
        for _ in 0..9 {
            session.add_links(&ten).unwrap();
        }
        let nine = "https://a.example/v\n".repeat(9);
        assert_eq!(session.add_links(&nine).unwrap().len(), 9);

        let error = session
            .add_links("https://a.example/a https://a.example/b")
            .unwrap_err();
        assert_eq!(error.kind, UiErrorKind::LinkNotValid);
        assert!(error.message.contains("100"), "{}", error.message);
        assert_eq!(session.state().entries.len(), 99, "entró alguno");

        assert_eq!(session.add_links("https://a.example/a").unwrap().len(), 1);
        assert!(session.add_links("https://a.example/b").is_err());
        session.remove_entry(1).unwrap();
        assert!(session.add_links("https://a.example/b").is_ok());
    }

    #[test]
    fn without_any_link_returns_error() {
        for text in ["", " \n\t "] {
            let error = Session::default().add_links(text).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::LinkNotValid);
        }
    }

    #[test]
    fn every_download_error_has_its_kind() {
        let source = || std::io::Error::other("x");
        let program = || PathBuf::from("yt-dlp").display().to_string();
        let cases = [
            (
                DownloadError::LinkNotValid {
                    reason: "está vacío",
                },
                UiErrorKind::LinkNotValid,
            ),
            (
                DownloadError::YtDlpNotFound { program: program() },
                UiErrorKind::YtDlpNotFound,
            ),
            (
                DownloadError::ProcessNotLaunched {
                    program: program(),
                    source: source(),
                },
                UiErrorKind::Other,
            ),
            (
                DownloadError::ThreadNotLaunched { source: source() },
                UiErrorKind::Other,
            ),
            (DownloadError::TempNotCreated(source()), UiErrorKind::Other),
            (
                DownloadError::TimedOut {
                    limit: Duration::from_secs(1),
                },
                UiErrorKind::TimedOut,
            ),
            (
                DownloadError::FileTooLarge { max_bytes: 1 },
                UiErrorKind::InvalidEntry,
            ),
            (
                DownloadError::FailedDownload {
                    code: Some(1),
                    summarized_stderr: "404".into(),
                },
                UiErrorKind::FailedDownload,
            ),
            (
                DownloadError::FileNotLocated { reason: "x" },
                UiErrorKind::FailedDownload,
            ),
            (DownloadError::Cancelled, UiErrorKind::Cancelled),
        ];
        for (error, kind) in cases {
            let text = error.to_string();
            assert_eq!(UiError::from(error).kind, kind, "{text}");
        }
    }

    #[test]
    fn failed_download_has_no_cli_login_advice() {
        let ui = UiError::from(DownloadError::FailedDownload {
            code: Some(1),
            summarized_stderr: "HTTP Error 404".into(),
        });
        assert_eq!(
            ui.message,
            "yt-dlp no pudo descargar el video (código 1): HTTP Error 404"
        );
        let ui = UiError::from(DownloadError::FailedDownload {
            code: None,
            summarized_stderr: "x".into(),
        });
        assert_eq!(
            ui.message,
            "yt-dlp no pudo descargar el video (terminado por una señal): x"
        );
    }

    #[test]
    fn close_waits_for_yt_dlp_and_child_to_die_and_deletes_everything() {
        let fake = FalseYtDlp::new();
        let session = Arc::new(fake.session());
        session
            .add_links("https://a.example/1 https://b.example/espera https://c.example/3")
            .unwrap();

        let queue = download_separately(&session);
        let pids = fake.processes();
        assert_eq!(fake.temp_files_left().len(), 2);

        let start = Instant::now();
        session.close();

        assert!(start.elapsed() < DEADLINE, "agotó el plazo de cierre");
        assert!(fake.temp_files_left().is_empty());
        assert!(!alive_process(&pids[0]), "yt-dlp sigue vivo");
        assert!(die(&pids), "sigue vivo el hijo de yt-dlp");
        on_finish(&queue);
        assert_eq!(fake.lines("enlaces").len(), 2, "empezó otra al cerrar");
        assert!(session.state().entries.is_empty());
    }

    #[test]
    fn after_closing_no_download_starts() {
        let fake = FalseYtDlp::new();
        let session = fake.session();
        session.close();
        session.add_links("https://a.example/1").unwrap();

        download(&session);

        assert!(fake.lines("enlaces").is_empty());
    }

    #[test]
    fn end_to_end_add_download_create_save_and_remove() {
        let fake = FalseYtDlp::new();
        let session = fake.session();
        let destination = tempfile::tempdir().unwrap();

        let entries = session
            .add_links("https://a.example/falla\nhttps://b.example/v")
            .unwrap();
        let (failed, id) = (entries[0].id, entries[1].id);
        let events = download(&session);
        assert_eq!(states(&events, id).pop(), Some(EntryState::Ready));

        let created = session.create(&request(id), &|_: ProgressUi| {}).unwrap();
        assert!(created.meets_limit, "{created:?}");
        let proposal = session.proposal_saved(id).unwrap();
        assert_eq!(proposal.name, "Gato_bailando.gif");
        let name = session
            .save_to(id, &destination.path().join(&proposal.name))
            .unwrap();
        assert_eq!(name, "Gato_bailando.gif");
        let storage = fs::read(destination.path().join(&name)).unwrap();
        assert_eq!(storage, session.result(id).unwrap());
        assert!(storage.len() as u64 <= created.max_size_bytes);
        assert_eq!(&storage[..3], b"GIF");

        drop(session.remove_entry(id).unwrap());
        assert!(session.remove_entry(failed).unwrap().is_none());

        assert!(session.state().entries.is_empty());
        assert_eq!(fake.lines("temporales").len(), 2);
        assert!(fake.temp_files_left().is_empty());
        assert!(destination.path().join(&name).is_file());
    }

    const LOGS_VARIABLE: &str = "CONVERSOR_PRUEBA_LOGS";

    #[test]
    fn logs_have_host_and_never_the_link_or_stderr() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "session::downloads::tests::logs_in_separate_process",
                "--ignored",
                "--nocapture",
            ])
            .env(LOGS_VARIABLE, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let summary = String::from_utf8_lossy(&output.stdout);
        assert!(
            summary.contains("1 passed"),
            "the test did not run: {summary}"
        );
    }

    #[test]
    #[ignore = "launched by logs_have_host_and_never_the_link_or_stderr"]
    fn logs_in_separate_process() {
        if std::env::var(LOGS_VARIABLE).is_err() {
            return;
        }
        let fake = FalseYtDlp::new();
        let session = fake.session();
        session
            .add_links(
                "https://a.example/falla?clave=secreto https://b.example/con-titulo?clave=secreto",
            )
            .unwrap();
        let log = fake.directory.path().join("registro.log");
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(Mutex::new(fs::File::create(&log).unwrap()))
            .finish();

        let events = tracing::subscriber::with_default(subscriber, || {
            let events = download(&session);
            session.preview(2, 0.0).unwrap();
            session.create(&request(2), &|_: ProgressUi| {}).unwrap();
            events
        });
        let visible = session.state().entry(2).unwrap().name.clone();
        assert_eq!(visible, "Titulo_secreto", "el usuario sí ve el título");

        let log = fs::read_to_string(log).unwrap();
        assert!(log.contains("a.example"), "{log}");
        assert!(log.contains("video inspeccionado"), "{log}");
        assert!(log.contains("video decodificado"), "{log}");
        assert!(!log.contains("secreto"), "{log}");
        assert!(!log.contains("no se pudo abrir"), "{log}");
        let Some(EntryState::Error { error }) = states(&events, 1).pop() else {
            panic!("{events:?}");
        };
        assert!(error.message.contains("no se pudo abrir"));
    }

    const FAKE_VARIABLE: &str = "CONVERSOR_PRUEBA_YT_DLP";
    const LINK_VARIABLE: &str = "CONVERSOR_PRUEBA_ENLACE";

    #[test]
    #[ignore = "launched by nothing_survives_when_process_exits_without_releasing"]
    fn process_that_closes_and_exits_without_releasing() {
        let (Ok(executable), Ok(link)) =
            (std::env::var(FAKE_VARIABLE), std::env::var(LINK_VARIABLE))
        else {
            return;
        };
        let session = Arc::new(Session::with_downloader(
            YtDlpDownloader::new().with_executable(&executable),
        ));
        session.add_links(&link).unwrap();
        let queue = Arc::clone(&session);
        thread::spawn(move || download(&queue));
        let mark = Path::new(&executable).with_file_name("hijo");
        let start = Instant::now();
        while fs::read_to_string(&mark).unwrap_or_default().is_empty() {
            assert!(start.elapsed() < DEADLINE);
            thread::sleep(Duration::from_millis(10));
        }
        session.close();
        std::process::exit(0);
    }

    #[test]
    fn nothing_survives_when_process_exits_without_releasing() {
        let fake = FalseYtDlp::new();
        let state = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "session::downloads::tests::process_that_closes_and_exits_without_releasing",
                "--ignored",
            ])
            .env(FAKE_VARIABLE, fake.directory.path().join("yt-dlp"))
            .env(LINK_VARIABLE, "https://a.example/espera")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();

        assert!(state.success());
        let pids = fake.processes();
        let died = die(&pids);
        if !died {
            Command::new("kill")
                .args(["-KILL"])
                .args(&pids)
                .status()
                .unwrap();
        }
        assert!(died, "yt-dlp or its child survived");
        assert!(fake.temp_files_left().is_empty());
    }
}
