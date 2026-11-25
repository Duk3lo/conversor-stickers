use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::decoding::MAX_VIDEO_SIZE_BYTES;
use crate::error::DownloadError;
use crate::process::{
    STDERR_MAX_BYTES, WatchedProcess, launch_thread, read_final_bounded, summarize_stderr,
};

pub const DOWNLOAD_TIME_LIMIT: Duration = Duration::from_secs(600);

/// Limits of «save video»: the whole file is kept, with sound, so it can be bigger and longer than
/// what a sticker needs.
pub const SAVE_TIME_LIMIT: Duration = Duration::from_secs(30 * 60);
pub const SAVE_MAX_SIZE_BYTES: u64 = 500 * 1024 * 1024;
const SAVE_MAX_DURATION_S: u32 = 30 * 60;

const LINK_MAX_LENGTH: usize = 2048;

const NAME_MAX_LENGTH: usize = 60;

const DEFAULT_NAME: &str = "video";

const NAME_IN_TEMP: &str = "video";

const LENGTH_MAX_EXTENSION: usize = 5;

const CANCELLATION_INTERVAL: Duration = Duration::from_millis(50);

const INTERVAL_FINAL: Duration = Duration::from_millis(5);

const OUTPUT_GRACE: Duration = Duration::from_millis(500);

const SIZE_INTERVAL: Duration = Duration::from_millis(250);

const LINE_MAX_BYTES: u64 = 64 * 1024;

const PROGRESS_MARK: &str = "conversor-progreso";

#[derive(Clone, PartialEq, Eq)]
pub struct Link {
    text: String,
    host: std::ops::Range<usize>,
}

impl Link {
    pub fn new(text: &str) -> Result<Link, DownloadError> {
        let not_valid = |reason| DownloadError::LinkNotValid { reason };
        let text = text.trim();
        if text.is_empty() {
            return Err(not_valid("está vacío"));
        }
        if text.len() > LINK_MAX_LENGTH {
            return Err(not_valid("pasa de 2048 bytes"));
        }
        if text
            .chars()
            .any(|char| char.is_whitespace() || char.is_control())
        {
            return Err(not_valid("tiene espacios o caracteres de control en medio"));
        }
        let (schema, rest) = text
            .split_once("://")
            .ok_or_else(|| not_valid("no empieza por http:// ni https://"))?;
        let schema = schema.to_ascii_lowercase();
        if schema != "http" && schema != "https" {
            return Err(not_valid("solo se admiten enlaces http:// y https://"));
        }
        let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        if authority.contains('@') {
            return Err(not_valid("no puede llevar usuario ni contraseña"));
        }
        let long_host = if authority.starts_with('[') {
            authority
                .find(']')
                .map_or(authority.len(), |closure| closure + 1)
        } else {
            authority.find(':').unwrap_or(authority.len())
        };
        if long_host == 0 {
            return Err(not_valid("no tiene host"));
        }

        let start_host = schema.len() + "://".len();
        Ok(Link {
            text: format!("{schema}://{rest}"),
            host: start_host..start_host + long_host,
        })
    }

    pub fn host(&self) -> &str {
        &self.text[self.host.clone()]
    }

    pub fn as_text(&self) -> &str {
        &self.text
    }
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Enlace")
            .field("host", &self.host())
            .finish_non_exhaustive()
    }
}

pub fn looks_like_link(text: &str) -> bool {
    let Some((schema, _)) = text.trim_start().split_once("://") else {
        return false;
    };
    let mut chars = schema.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|char| char.is_ascii_alphanumeric() || "+.-".contains(char))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadProgress {
    Downloading { bytes: u64, total: Option<u64> },
}

pub struct DownloadedVideo {
    path: PathBuf,
    name_base: String,
    _temp: Temp,
}

impl fmt::Debug for DownloadedVideo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VideoDescargado")
            .field("ruta", &self.path)
            .finish_non_exhaustive()
    }
}

impl DownloadedVideo {
    pub fn from_own_download(
        max_size_bytes: u64,
        download: impl FnOnce(&Path) -> Result<PathBuf, DownloadError>,
    ) -> Result<DownloadedVideo, DownloadError> {
        let temp = Temp::create().map_err(DownloadError::TempNotCreated)?;
        let path = download(temp.path())?;
        DownloadedVideo::adopt(temp, &path, max_size_bytes)
    }

    fn adopt(
        temp: Temp,
        downloaded_path: &Path,
        max_size_bytes: u64,
    ) -> Result<DownloadedVideo, DownloadError> {
        let path = locate_in_temp(downloaded_path, temp.path())?;
        if bytes_in(temp.path()) > max_size_bytes {
            return Err(DownloadError::FileTooLarge {
                max_bytes: max_size_bytes,
            });
        }
        let name_base = sanitize_name_base(
            &path
                .file_stem()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default(),
        );
        let path = rename_without_title(&path, temp.path())?;
        Ok(DownloadedVideo {
            path,
            name_base,
            _temp: temp,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name_base(&self) -> &str {
        &self.name_base
    }
}

/// What the download is for: a sticker only needs the best video up to 720p; saving keeps video
/// and audio, up to 1080p, merged into an mp4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DownloadMode {
    #[default]
    Sticker,
    Save,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YtDlpDownloader {
    executable: PathBuf,
    time_limit: Duration,
    max_size_bytes: u64,
    mode: DownloadMode,
}

impl Default for YtDlpDownloader {
    fn default() -> Self {
        YtDlpDownloader::new()
    }
}

impl YtDlpDownloader {
    pub fn new() -> Self {
        YtDlpDownloader {
            executable: PathBuf::from("yt-dlp"),
            time_limit: DOWNLOAD_TIME_LIMIT,
            max_size_bytes: MAX_VIDEO_SIZE_BYTES,
            mode: DownloadMode::Sticker,
        }
    }

    /// The same downloader (same executable) in save mode, with its own size and time limits.
    pub fn for_saving(self) -> Self {
        YtDlpDownloader {
            time_limit: SAVE_TIME_LIMIT,
            max_size_bytes: SAVE_MAX_SIZE_BYTES,
            mode: DownloadMode::Save,
            ..self
        }
    }

    pub fn with_executable(self, path: impl Into<PathBuf>) -> Self {
        YtDlpDownloader {
            executable: path.into(),
            ..self
        }
    }

    pub fn with_time_limit(self, limit: Duration) -> Self {
        YtDlpDownloader {
            time_limit: limit,
            ..self
        }
    }

    pub fn with_max_size(self, bytes: u64) -> Self {
        YtDlpDownloader {
            max_size_bytes: bytes,
            ..self
        }
    }

    pub fn download(
        &self,
        link: &Link,
        progress: &(dyn Fn(DownloadProgress) + Sync),
        cancellation: &AtomicBool,
    ) -> Result<DownloadedVideo, DownloadError> {
        let temp = Temp::create().map_err(DownloadError::TempNotCreated)?;
        info!(host = link.host(), "descargando video");

        let program = self.executable.display().to_string();
        let start = Instant::now();
        let child = self
            .order(link, temp.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| {
                if source.kind() == ErrorKind::NotFound {
                    DownloadError::YtDlpNotFound {
                        program: program.clone(),
                    }
                } else {
                    DownloadError::ProcessNotLaunched {
                        program: program.clone(),
                        source,
                    }
                }
            })?;
        let mut process = WatchedProcess::with_own_group(child, program);
        let without_pipe = |program: &str| DownloadError::ProcessNotLaunched {
            program: program.to_string(),
            source: io::Error::other("no se pudo abrir el pipe"),
        };
        let stdout = process
            .child
            .stdout
            .take()
            .ok_or_else(|| without_pipe(&process.program))?;
        let mut stderr = process
            .child
            .stderr
            .take()
            .ok_or_else(|| without_pipe(&process.program))?;

        let thread_not_launched = |source| DownloadError::ThreadNotLaunched { source };
        let stderr_reader = launch_thread("yt-dlp-stderr".to_string(), move || {
            read_final_bounded(&mut stderr, STDERR_MAX_BYTES)
        })
        .map_err(thread_not_launched)?;
        let (sending, reception) = mpsc::channel();
        launch_thread("yt-dlp-stdout".to_string(), move || {
            read_stdout(stdout, &sending);
        })
        .map_err(thread_not_launched)?;

        let too_large = DownloadError::FileTooLarge {
            max_bytes: self.max_size_bytes,
        };
        let cancelled = || cancellation.load(Ordering::Relaxed);
        let mut last_measure = start;
        let mut reading = None;
        let mut end_of_leader = None;
        let mut state = None;
        let without_state = |source| DownloadError::ProcessNotLaunched {
            program: self.executable.display().to_string(),
            source,
        };
        let (state, reading) = loop {
            if cancelled() {
                return Err(DownloadError::Cancelled);
            }
            if last_measure.elapsed() >= SIZE_INTERVAL {
                last_measure = Instant::now();
                if bytes_in(temp.path()) > self.max_size_bytes {
                    return Err(too_large);
                }
            }
            let remaining = self.time_limit.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(DownloadError::TimedOut {
                    limit: self.time_limit,
                });
            }
            if reading.is_none() {
                match reception.recv_timeout(remaining.min(CANCELLATION_INTERVAL)) {
                    Ok(YtDlpOutput::Progress(advance)) => progress(advance),
                    Err(RecvTimeoutError::Timeout) => {}
                    Ok(YtDlpOutput::End(read)) => reading = Some(read),
                    Err(RecvTimeoutError::Disconnected) => {
                        reading = Some(Err(io::Error::other(
                            "el hilo que leía la salida terminó sin resultado",
                        )));
                    }
                }
            } else {
                std::thread::sleep(remaining.min(INTERVAL_FINAL));
            }
            let closed_outputs = reading.is_some() && stderr_reader.is_finished();
            if state.is_none() && process.leader_finished().map_err(without_state)? {
                let end = *end_of_leader.get_or_insert_with(Instant::now);
                if closed_outputs || end.elapsed() >= OUTPUT_GRACE {
                    state = Some(process.kill_group_and_wait().map_err(without_state)?);
                }
            }
            if closed_outputs
                && let Some(state) = state
                && let Some(read) = reading.take()
            {
                break (state, read);
            }
        };
        if cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let stderr = stderr_reader
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("el hilo que leía stderr falló")))
            .unwrap_or_else(|error| {
                warn!(%error, "no se pudo leer el stderr de yt-dlp");
                Vec::new()
            });
        if !state.success() {
            return Err(DownloadError::FailedDownload {
                code: state.code(),
                summarized_stderr: summarize_stderr(&String::from_utf8_lossy(&stderr)),
            });
        }
        let printed_path = reading
            .map_err(|error| {
                warn!(%error, "no se pudo leer la salida de yt-dlp");
                DownloadError::FileNotLocated {
                    reason: "no se pudo leer la salida de yt-dlp",
                }
            })?
            .ok_or(DownloadError::FileNotLocated {
                reason: "yt-dlp no imprimió la ruta del video: puede ser un directo (no se admiten \
                         directos) o pasar del tamaño máximo",
            })?;
        let video = DownloadedVideo::adopt(temp, Path::new(&printed_path), self.max_size_bytes)?;
        info!(host = link.host(), "video descargado");
        Ok(video)
    }

    fn order(&self, link: &Link, temp: &Path) -> Command {
        let mut command = Command::new(&self.executable);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command
            .args(["--ignore-config", "--no-plugin-dirs"])
            .args([
                "--no-cookies",
                "--no-cookies-from-browser",
                "--no-cache-dir",
            ])
            .arg("--no-playlist")
            .args(["--match-filters", &self.filter()])
            .args(self.quality())
            .arg("--restrict-filenames")
            .args(["--max-filesize", &self.max_size_bytes.to_string()])
            .args(["--socket-timeout", "30"])
            .arg("-P")
            .arg(temp)
            .args(["-o", "%(title).60s.%(ext)s"])
            .args(["--print", "after_move:filepath", "--no-simulate"])
            .args(["--progress", "--newline", "--progress-template"])
            .arg(format!(
                "download:{PROGRESS_MARK} %(progress.downloaded_bytes)s \
                 %(progress.total_bytes)s %(progress.total_bytes_estimate)s"
            ))
            .arg("--")
            .arg(link.as_text());
        command
    }
}

impl YtDlpDownloader {
    fn filter(&self) -> String {
        match self.mode {
            DownloadMode::Sticker => "!is_live".to_string(),
            // `<=?` also lets through the sites that do not tell the duration.
            DownloadMode::Save => format!("!is_live & duration<=?{SAVE_MAX_DURATION_S}"),
        }
    }

    fn quality(&self) -> &'static [&'static str] {
        match self.mode {
            DownloadMode::Sticker => &["-S", "res:720"],
            DownloadMode::Save => &[
                "-f",
                "bv*[height<=1080]+ba/b[height<=1080]/b",
                "--merge-output-format",
                "mp4",
            ],
        }
    }
}

#[derive(Debug)]
struct Temp(PathBuf);

impl Temp {
    fn create() -> io::Result<Self> {
        let mut builder = tempfile::Builder::new();
        builder.prefix("conversor-descarga-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o700));
        }
        Ok(Temp(builder.tempdir()?.keep()))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            warn!(%error, "no se pudo borrar el temporal de la descarga");
        }
    }
}

enum YtDlpOutput {
    Progress(DownloadProgress),
    End(io::Result<Option<String>>),
}

fn read_stdout(stdout: impl Read, sending: &mpsc::Sender<YtDlpOutput>) {
    let mut reader = BufReader::new(stdout);
    let mut last: Option<String> = None;
    let mut line = Vec::new();
    let mut discarding = false;
    let result = loop {
        line.clear();
        match (&mut reader)
            .take(LINE_MAX_BYTES)
            .read_until(b'\n', &mut line)
        {
            Ok(0) => break Ok(last),
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => break Err(error),
        }
        let complete = line.ends_with(b"\n");
        if discarding {
            discarding = !complete;
            continue;
        }
        if !complete && line.len() as u64 >= LINE_MAX_BYTES {
            discarding = true;
            continue;
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\n', '\r']);
        if let Some(data) = text.strip_prefix(PROGRESS_MARK) {
            if let Some(progress) = interpret_progress(data) {
                let _ = sending.send(YtDlpOutput::Progress(progress));
            }
        } else if !text.trim().is_empty() {
            last = Some(text.to_string());
        }
    };
    let _ = sending.send(YtDlpOutput::End(result));
}

fn interpret_progress(data: &str) -> Option<DownloadProgress> {
    let mut fields = data.split_whitespace();
    let bytes = fields.next()?.parse().ok()?;
    let mut total = || fields.next().and_then(bytes_of_yt_dlp);
    let announced = total();
    let estimated = total();
    Some(DownloadProgress::Downloading {
        bytes,
        total: announced.or(estimated),
    })
}

fn bytes_of_yt_dlp(text: &str) -> Option<u64> {
    if let Ok(bytes) = text.parse() {
        return Some(bytes);
    }
    let bytes: f64 = text.parse().ok()?;
    (bytes.is_finite() && bytes >= 0.0).then_some(bytes as u64)
}

fn bytes_in(directory: &Path) -> u64 {
    let Ok(inputs) = fs::read_dir(directory) else {
        return 0;
    };
    inputs
        .filter_map(|input| {
            let path = input.ok()?.path();
            let metadata = fs::symlink_metadata(&path).ok()?;
            if metadata.is_dir() {
                Some(bytes_in(&path))
            } else {
                Some(metadata.len())
            }
        })
        .fold(0u64, u64::saturating_add)
}

fn locate_in_temp(path: &Path, temp: &Path) -> Result<PathBuf, DownloadError> {
    let not_located = |reason| DownloadError::FileNotLocated { reason };
    let canonical =
        fs::canonicalize(path).map_err(|_| not_located("la ruta que imprimió yt-dlp no existe"))?;
    let temp = fs::canonicalize(temp)
        .map_err(|_| not_located("no se pudo resolver el directorio temporal"))?;
    if !canonical.starts_with(&temp) {
        return Err(not_located(
            "la ruta que imprimió yt-dlp está fuera del directorio temporal",
        ));
    }
    let metadata =
        fs::metadata(&canonical).map_err(|_| not_located("no se pudo leer el video descargado"))?;
    if !metadata.is_file() {
        return Err(not_located(
            "la ruta que imprimió yt-dlp no es un archivo normal",
        ));
    }
    Ok(canonical)
}

fn rename_without_title(path: &Path, temp: &Path) -> Result<PathBuf, DownloadError> {
    let not_renamed = |error: io::Error| {
        warn!(%error, "no se pudo renombrar el video descargado");
        DownloadError::FileNotLocated {
            reason: "no se pudo renombrar el video descargado",
        }
    };
    let name = match extension_safe(path) {
        Some(extension) => format!("{NAME_IN_TEMP}.{extension}"),
        None => NAME_IN_TEMP.to_string(),
    };
    let destination = fs::canonicalize(temp).map_err(not_renamed)?.join(name);
    fs::rename(path, &destination).map_err(not_renamed)?;
    Ok(destination)
}

fn extension_safe(path: &Path) -> Option<&str> {
    let extension = path.extension()?.to_str()?;
    let valid = (1..=LENGTH_MAX_EXTENSION).contains(&extension.len())
        && extension.chars().all(|char| char.is_ascii_alphanumeric());
    valid.then_some(extension)
}

pub(crate) fn sanitize_name_base(title: &str) -> String {
    let filtered: String = title
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
    let name = &without_prefix[..without_prefix.len().min(NAME_MAX_LENGTH)];
    if name.is_empty() {
        DEFAULT_NAME.to_string()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn reason(text: &str) -> &'static str {
        match Link::new(text) {
            Err(DownloadError::LinkNotValid { reason }) => reason,
            other => panic!("{text:?} debería ser inválido y dio {other:?}"),
        }
    }

    #[test]
    fn accepts_http_and_https_in_uppercase_or_lowercase_and_with_spaces_around() {
        for (text, expected, host) in [
            (
                "https://www.tiktok.com/@user/video/123",
                "https://www.tiktok.com/@user/video/123",
                "www.tiktok.com",
            ),
            ("http://ejemplo.org", "http://ejemplo.org", "ejemplo.org"),
            (
                "HTTPS://Ejemplo.org/V?id=1#t",
                "https://Ejemplo.org/V?id=1#t",
                "Ejemplo.org",
            ),
            (
                "  Http://ejemplo.org:8080/x \n",
                "http://ejemplo.org:8080/x",
                "ejemplo.org",
            ),
            (
                "http://[::1]:8000/v.mp4",
                "http://[::1]:8000/v.mp4",
                "[::1]",
            ),
            ("https://youtu.be?v=x", "https://youtu.be?v=x", "youtu.be"),
        ] {
            let link = Link::new(text).unwrap();
            assert_eq!(link.as_text(), expected, "{text:?}");
            assert_eq!(link.host(), host, "{text:?}");
        }
    }

    #[test]
    fn rejects_others_schemas_and_links_badly_formed() {
        let long = format!("https://ejemplo.org/{}", "a".repeat(2048));
        for (text, expected) in [
            ("ftp://ejemplo.org/v.mp4", "solo se admiten"),
            ("file:///etc/passwd", "solo se admiten"),
            ("javascript://alert(1)", "solo se admiten"),
            ("javascript:alert(1)", "no empieza por"),
            ("ejemplo.org/v.mp4", "no empieza por"),
            ("https://", "no tiene host"),
            ("https:///ruta", "no tiene host"),
            ("https://:8080/x", "no tiene host"),
            ("https://usuario@ejemplo.org/", "usuario ni contraseña"),
            ("https://u:clave@ejemplo.org/", "usuario ni contraseña"),
            ("https://ejemplo.org/a b", "espacios"),
            ("https://ejemplo.org/a\tb", "espacios"),
            ("https://ejemplo.org/a\u{7}b", "caracteres de control"),
            ("https://ejemplo.org/\u{0}", "caracteres de control"),
            (long.as_str(), "2048"),
            ("", "vacío"),
            ("   ", "vacío"),
        ] {
            let reason = reason(text);
            assert!(reason.contains(expected), "{text:?}: {reason}");
        }
    }

    #[test]
    fn debug_of_link_only_sample_host() {
        let link = Link::new("https://ejemplo.org/u/ana?token=secreto").unwrap();
        let text = format!("{link:?}");
        assert_eq!(text, "Enlace { host: \"ejemplo.org\", .. }");
        assert!(!text.contains("secreto"));
    }

    #[test]
    fn link_of_2048_bytes_just_is_valid() {
        let base = "https://ejemplo.org/";
        let link = format!("{base}{}", "a".repeat(2048 - base.len()));
        assert!(Link::new(&link).is_ok());
    }

    #[test]
    fn looks_link_follows_rule_of_togif() {
        for text in [
            "https://x",
            "ftp://x",
            "file:///etc/passwd",
            "a+b.c-d://x",
            "HTTP://x",
            "  https://x",
            "x1://",
        ] {
            assert!(looks_like_link(text), "{text:?}");
        }
        for text in [
            "gato.mp4",
            "./ftp://x",
            "1http://x",
            "://x",
            "ht_tp://x",
            "javascript:alert(1)",
            "https:/x",
            "",
        ] {
            assert!(!looks_like_link(text), "{text:?}");
        }
    }

    fn read(output: &[u8]) -> (Vec<DownloadProgress>, Option<String>) {
        let (sending, reception) = mpsc::channel();
        read_stdout(output, &sending);
        drop(sending);
        let mut progress_updates = Vec::new();
        let mut path = None;
        for message in reception {
            match message {
                YtDlpOutput::Progress(progress) => progress_updates.push(progress),
                YtDlpOutput::End(reading) => path = reading.unwrap(),
            }
        }
        (progress_updates, path)
    }

    #[test]
    fn line_over_64_kib_discards_whole() {
        let huge = "a".repeat(200_000);
        let (_, path) = read(format!("/tmp/v.mp4\n{huge}").as_bytes());
        assert_eq!(path.as_deref(), Some("/tmp/v.mp4"));
        let (progress_updates, path) =
            read(format!("{huge}\nconversor-progreso 5 NA NA\n{huge}\n/tmp/w.mp4\n").as_bytes());
        assert_eq!(path.as_deref(), Some("/tmp/w.mp4"));
        assert_eq!(
            progress_updates,
            [DownloadProgress::Downloading {
                bytes: 5,
                total: None
            }]
        );
    }

    #[test]
    fn debug_of_video_downloaded_does_not_show_title() {
        let temp = Temp::create().unwrap();
        let video = DownloadedVideo {
            path: temp.path().join("video.mp4"),
            name_base: "Mi_cumpleanos_con_Ana".to_string(),
            _temp: temp,
        };
        let text = format!("{video:?}");
        assert!(!text.contains("Mi_cumpleanos_con_Ana"), "{text}");
        assert!(text.contains("video.mp4"), "{text}");
    }

    #[test]
    fn download_own_remains_as_video_in_its_temp() {
        let video = DownloadedVideo::from_own_download(1024, |temp| {
            let path = temp.join("Mi cumple.mp4");
            fs::write(&path, b"video").unwrap();
            Ok(path)
        })
        .unwrap();
        assert_eq!(video.path().file_name().unwrap(), "video.mp4");
        assert_eq!(video.name_base(), "Mi_cumple");
        assert_eq!(fs::read(video.path()).unwrap(), b"video");
        let temp = video.path().parent().unwrap().to_path_buf();
        drop(video);
        assert!(!temp.exists());
    }

    #[test]
    fn download_own_that_fails_not_leaves_temp() {
        let mut used = None;
        let error = DownloadedVideo::from_own_download(1024, |temp| {
            used = Some(temp.to_path_buf());
            fs::write(temp.join("a.part"), b"a medias").unwrap();
            Err(DownloadError::Cancelled)
        })
        .unwrap_err();
        assert!(matches!(error, DownloadError::Cancelled));
        assert!(!used.unwrap().exists());
    }

    #[test]
    fn download_own_outside_of_temp_or_too_large_rejects() {
        let outside = tempfile::tempdir().unwrap();
        let foreign = outside.path().join("ajeno.mp4");
        fs::write(&foreign, b"video").unwrap();
        let error = DownloadedVideo::from_own_download(1024, |_| Ok(foreign.clone())).unwrap_err();
        assert!(matches!(error, DownloadError::FileNotLocated { .. }));
        assert!(foreign.exists(), "no toca lo que no es suyo");

        let error = DownloadedVideo::from_own_download(4, |temp| {
            let path = temp.join("grande.mp4");
            fs::write(&path, b"demasiado").unwrap();
            Ok(path)
        })
        .unwrap_err();
        assert!(matches!(
            error,
            DownloadError::FileTooLarge { max_bytes: 4 }
        ));
    }

    #[test]
    fn name_base_sanitizes() {
        for (title, expected) in [
            ("Gato_bailando-2024.v2", "Gato_bailando-2024.v2"),
            ("Canción del año", "Cancin_del_ao"),
            ("../../etc/passwd", "etcpasswd"),
            ("a/b\\c", "abc"),
            (".oculto", "oculto"),
            ("-rf", "rf"),
            ("-.-.x", "x"),
            ("", "video"),
            ("日本語", "video"),
            ("...", "video"),
        ] {
            assert_eq!(sanitize_name_base(title), expected, "{title:?}");
        }
        assert_eq!(sanitize_name_base(&"a".repeat(100)), "a".repeat(60));
    }

    fn is_valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= NAME_MAX_LENGTH
            && !name.starts_with(['.', '-'])
            && name
                .chars()
                .all(|char| char.is_ascii_alphanumeric() || "._-".contains(char))
    }

    proptest! {
        #[test]
        fn name_base_always_is_valid(title in any::<String>()) {
            let name = sanitize_name_base(&title);
            prop_assert!(is_valid_name(&name), "{title:?} → {name:?}");
        }

        #[test]
        fn link_valid_has_no_spaces_and_its_host_not_is_empty(text in any::<String>()) {
            if let Ok(link) = Link::new(&text) {
                prop_assert!(!link.host().is_empty());
                prop_assert!(link.as_text().starts_with("http"));
                prop_assert!(!link.as_text().contains(char::is_whitespace));
            }
        }
    }
}
