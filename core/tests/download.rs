#![allow(clippy::panic, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use conversor_core::{
    ConversionOptions, ConversionParams, DownloadError, DownloadProgress, DownloadedVideo,
    FfmpegDecoder, Link, Tracking, WHATSAPP_STICKER, YtDlpDownloader, convert,
};
use tempfile::TempDir;

const DEADLINE: Duration = Duration::from_secs(30);

const ROOMY_LIMIT: Duration = Duration::from_secs(20);

struct FalseYtDlp {
    directory: TempDir,
}

impl FalseYtDlp {
    fn new(body: &str) -> Self {
        let directory = tempfile::tempdir().expect("directorio temporal");
        let data = directory.path().display();
        let script = format!(
            "#!/bin/sh\n\
             printf '%s\\0' \"$@\" > '{data}/argumentos'\n\
             echo $$ > '{data}/pid'\n\
             DATOS='{data}'\n\
             DIR=''\n\
             previo=''\n\
             for arg in \"$@\"; do\n\
               if [ \"$previo\" = '-P' ]; then DIR=\"$arg\"; fi\n\
               previo=\"$arg\"\n\
             done\n\
             printf '%s' \"$DIR\" > '{data}/temporal'\n\
             {body}\n"
        );
        let source = directory.path().join("yt-dlp.sh");
        fs::write(&source, script).expect("se puede escribir el script");
        let state = std::process::Command::new("install")
            .args(["-m", "755"])
            .arg(&source)
            .arg(directory.path().join("yt-dlp"))
            .status()
            .expect("se puede ejecutar install");
        assert!(state.success(), "install falló");
        FalseYtDlp { directory }
    }

    fn downloader(&self) -> YtDlpDownloader {
        YtDlpDownloader::new().with_executable(self.directory.path().join("yt-dlp"))
    }

    fn arguments(&self) -> Vec<String> {
        let bytes =
            fs::read(self.directory.path().join("argumentos")).expect("el yt-dlp falso se ejecutó");
        let text = String::from_utf8(bytes).expect("argumentos en UTF-8");
        let mut arguments: Vec<String> = text.split('\0').map(str::to_string).collect();
        arguments.pop();
        arguments
    }

    fn temp(&self) -> PathBuf {
        PathBuf::from(
            fs::read_to_string(self.directory.path().join("temporal"))
                .expect("el yt-dlp falso guardó el temporal"),
        )
    }
}

fn link(text: &str) -> Link {
    Link::new(text).expect("enlace válido")
}

struct DownloadInRunning {
    cancellation: Arc<AtomicBool>,
    end: mpsc::Receiver<(
        Result<DownloadedVideo, DownloadError>,
        Vec<DownloadProgress>,
    )>,
}

impl DownloadInRunning {
    fn new(downloader: YtDlpDownloader, text: &str) -> Self {
        let link = link(text);
        let cancellation = Arc::new(AtomicBool::new(false));
        let (sending, end) = mpsc::channel();
        let flag = Arc::clone(&cancellation);
        thread::spawn(move || {
            let progress_updates = std::sync::Mutex::new(Vec::new());
            let result = downloader.download(
                &link,
                &|progress| progress_updates.lock().expect("mutex").push(progress),
                &flag,
            );
            let _ = sending.send((result, progress_updates.into_inner().expect("mutex")));
        });
        DownloadInRunning { cancellation, end }
    }

    fn cancel(&self) {
        self.cancellation.store(true, Ordering::Relaxed);
    }

    fn wait(
        self,
    ) -> (
        Result<DownloadedVideo, DownloadError>,
        Vec<DownloadProgress>,
    ) {
        self.end.recv_timeout(DEADLINE).unwrap_or_else(|error| {
            self.cancel();
            panic!("la descarga no terminó en {DEADLINE:?}: {error}")
        })
    }
}

fn download(
    fake: &FalseYtDlp,
    text: &str,
) -> (
    Result<DownloadedVideo, DownloadError>,
    Vec<DownloadProgress>,
) {
    DownloadInRunning::new(fake.downloader(), text).wait()
}

fn download_with(downloader: YtDlpDownloader) -> Result<DownloadedVideo, DownloadError> {
    DownloadInRunning::new(downloader, "https://ejemplo.org/v")
        .wait()
        .0
}

fn meets(condition: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < DEADLINE {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

fn create_e_print(name: &str) -> String {
    format!("printf 'video' > \"$DIR/{name}\"\necho \"$DIR/{name}\"")
}

#[test]
fn order_is_exactly_the_expected_without_shell_of_by_media() {
    let fake = FalseYtDlp::new(&create_e_print("v.mp4"));
    let text = "https://ejemplo.org/a;b$(touch${IFS}pwned)`id`'\"x\"&c=|d";
    let (result, _) = download(&fake, text);
    result.expect("descarga correcta");

    let temp = fake.temp();
    let expected: Vec<String> = [
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-cookies",
        "--no-cookies-from-browser",
        "--no-cache-dir",
        "--no-playlist",
        "--match-filters",
        "!is_live",
        "-S",
        "res:720",
        "--restrict-filenames",
        "--max-filesize",
        "2000000000",
        "--socket-timeout",
        "30",
        "-P",
        temp.to_str().expect("ruta UTF-8"),
        "-o",
        "%(title).60s.%(ext)s",
        "--print",
        "after_move:filepath",
        "--no-simulate",
        "--progress",
        "--newline",
        "--progress-template",
        "download:conversor-progreso %(progress.downloaded_bytes)s \
         %(progress.total_bytes)s %(progress.total_bytes_estimate)s",
        "--",
        text,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    let arguments = fake.arguments();
    assert_eq!(arguments, expected);
    assert_eq!(arguments[arguments.len() - 2], "--");
    assert!(!fake.directory.path().join("pwned").exists());
    assert!(!Path::new("pwned").exists());
}

#[test]
fn save_mode_asks_for_audio_and_video_merged_into_mp4_with_its_own_limits() {
    let fake = FalseYtDlp::new(&create_e_print("v.mp4"));
    let (result, _) =
        DownloadInRunning::new(fake.downloader().for_saving(), "https://ejemplo.org/v").wait();
    result.expect("descarga correcta");

    let arguments = fake.arguments();
    let after = |flag: &str| {
        let at = arguments.iter().position(|argument| argument == flag);
        arguments[at.expect("la opción está") + 1].clone()
    };
    assert_eq!(after("-f"), "bv*[height<=1080]+ba/b[height<=1080]/b");
    assert_eq!(after("--merge-output-format"), "mp4");
    assert_eq!(after("--match-filters"), "!is_live & duration<=?1800");
    assert_eq!(after("--max-filesize"), "524288000");
    assert!(!arguments.contains(&"-S".to_string()));
    assert!(arguments.iter().all(|argument| !argument.is_empty()));
}

#[test]
fn on_finish_returns_video_with_title_sanitized_and_deletes_it() {
    let fake = FalseYtDlp::new(&create_e_print("Gato bailando-ñ.mp4"));
    let (result, _) = download(&fake, "https://ejemplo.org/v");
    let video = result.expect("descarga correcta");

    assert!(video.path().is_file());
    assert_eq!(fs::read(video.path()).expect("se lee"), b"video");
    assert_eq!(video.name_base(), "Gato_bailando-");
    let temp = fake.temp();
    assert!(
        video
            .path()
            .starts_with(fs::canonicalize(&temp).expect("existe"))
    );
    drop(video);
    assert!(!temp.exists(), "el temporal sigue en {}", temp.display());
}

#[test]
fn secret_in_name_of_file_does_not_arrive_at_path() {
    for (printed, expected) in [
        ("SECRETO-a1b2c3.mp4", "video.mp4"),
        ("SECRETO/v.webm", "video.webm"),
        ("v.SECRETO", "video"),
        ("v.m-4", "video"),
        ("v.", "video"),
        ("SECRETO", "video"),
        ("video.mp4", "video.mp4"),
    ] {
        let fake = FalseYtDlp::new(&format!(
            "mkdir -p \"$(dirname \"$DIR/{printed}\")\"\n{}",
            create_e_print(printed)
        ));
        let (result, _) = download(&fake, "https://ejemplo.org/v/SECRETO-a1b2c3");
        let video = result.expect("descarga correcta");

        let path = video.path().to_string_lossy();
        assert!(!path.contains("SECRETO"), "{printed}: {path}");
        let temp = fs::canonicalize(fake.temp()).expect("existe");
        assert_eq!(video.path(), temp.join(expected), "{printed}");
        assert_eq!(fs::read(video.path()).expect("se lee"), b"video");
        let title = Path::new(printed).file_stem().expect("tiene nombre");
        assert_eq!(video.name_base(), title.to_string_lossy(), "{printed}");
    }
}

#[cfg(unix)]
#[test]
fn temp_only_can_read_its_owner() {
    use std::os::unix::fs::PermissionsExt;

    let fake = FalseYtDlp::new(&create_e_print("v.mp4"));
    let (result, _) = download(&fake, "https://ejemplo.org/v");
    let _video = result.expect("descarga correcta");
    let mode = fs::metadata(fake.temp())
        .expect("existe")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700, "{mode:o}");
}

#[derive(Clone, Default)]
struct Log(Arc<std::sync::Mutex<Vec<String>>>);

impl tracing::Subscriber for Log {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut text = event.metadata().level().to_string();
        event.record(
            &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                text.push_str(&format!(" {field}={value:?}"));
            },
        );
        self.0.lock().expect("mutex").push(text);
    }

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

#[test]
fn if_temp_not_can_delete_warns_without_path() {
    let fake = FalseYtDlp::new(&create_e_print("v.mp4"));
    let (result, _) = download(&fake, "https://ejemplo.org/v");
    let video = result.expect("descarga correcta");
    let temp = fake.temp();
    fs::remove_dir_all(&temp).expect("se puede borrar");

    let log = Log::default();
    tracing::subscriber::with_default(log.clone(), || drop(video));

    let events = log.0.lock().expect("mutex").join("\n");
    assert!(
        events.contains("WARN") && events.contains("no se pudo borrar el temporal"),
        "{events:?}"
    );
    let name = temp.file_name().expect("tiene nombre").to_string_lossy();
    assert!(!events.contains(&*name), "{events}");
}

#[test]
fn if_yt_dlp_fails_returns_download_failed_with_stderr_and_deletes_temp() {
    let fake = FalseYtDlp::new(
        "printf 'x' > \"$DIR/parcial.part\"\n\
         echo 'WARNING: algo' >&2\n\
         echo 'ERROR: [generic] Unable to download webpage: HTTP Error 404' >&2\n\
         exit 1",
    );
    let (result, _) = download(&fake, "https://ejemplo.org/v");
    match result {
        Err(DownloadError::FailedDownload {
            code,
            summarized_stderr,
        }) => {
            assert_eq!(code, Some(1));
            assert_eq!(
                summarized_stderr,
                "WARNING: algo | ERROR: [generic] Unable to download webpage: HTTP Error 404"
            );
        }
        other => panic!("se esperaba DescargaFallida y llegó {other:?}"),
    }
    assert!(!fake.temp().exists());
}

#[test]
fn path_outside_of_temp_or_no_returns_file_not_found() {
    for body in [
        "echo /etc/passwd".to_string(),
        "echo \"$DIR/../../../../../../etc/passwd\"".to_string(),
        "ln -s /etc/passwd \"$DIR/enlace.mp4\"\necho \"$DIR/enlace.mp4\"".to_string(),
        "mkdir \"$DIR/carpeta\"\necho \"$DIR/carpeta\"".to_string(),
        "echo \"$DIR/no-existe.mp4\"".to_string(),
        "true".to_string(),
    ] {
        let fake = FalseYtDlp::new(&body);
        let (result, _) = download(&fake, "https://ejemplo.org/v");
        assert!(
            matches!(result, Err(DownloadError::FileNotLocated { .. })),
            "{body}: {result:?}"
        );
        assert!(!fake.temp().exists(), "{body}");
    }
}

#[test]
fn without_executable_returns_yt_dlp_not_found() {
    let directory = tempfile::tempdir().expect("directorio temporal");
    let result =
        download_with(YtDlpDownloader::new().with_executable(directory.path().join("no-existe")));
    match result {
        Err(error @ DownloadError::YtDlpNotFound { .. }) => {
            assert!(error.to_string().contains("instala yt-dlp"), "{error}");
        }
        other => panic!("se esperaba YtDlpNoEncontrado y llegó {other:?}"),
    }
}

impl FalseYtDlp {
    fn pid(&self) -> String {
        fs::read_to_string(self.directory.path().join("pid"))
            .expect("el yt-dlp falso guardó su pid")
            .trim()
            .to_string()
    }
}

fn alive_process(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .expect("se puede ejecutar kill")
        .success()
}

impl FalseYtDlp {
    fn grandchild(&self) -> String {
        fs::read_to_string(self.directory.path().join("nieto"))
            .expect("el yt-dlp falso guardó el pid de su hijo")
            .trim()
            .to_string()
    }

    fn grandchild_dies(&self) -> bool {
        let grandchild = self.grandchild();
        meets(|| !alive_process(&grandchild))
    }

    fn cancel_when_this_ready(&self) -> Result<DownloadedVideo, DownloadError> {
        let download = DownloadInRunning::new(self.downloader(), "https://ejemplo.org/v");
        let mark = self.directory.path().join("listo");
        let ready = meets(|| mark.exists());
        thread::sleep(Duration::from_millis(100));
        download.cancel();
        let (result, _) = download.wait();
        assert!(ready, "el yt-dlp falso no llegó a su marca: {result:?}");
        result
    }
}

impl FalseYtDlp {
    fn exhaust_time(&self) -> Result<DownloadedVideo, DownloadError> {
        let mark = self.directory.path().join("listo");
        let mut limit = Duration::from_secs(1);
        while limit < DEADLINE {
            let result = download_with(self.downloader().with_time_limit(limit));
            if mark.exists() {
                return result;
            }
            limit *= 2;
        }
        panic!("el yt-dlp falso no llegó a su marca en {DEADLINE:?}")
    }
}

fn with_grandchild(after: &str) -> String {
    format!("sleep 120 &\necho $! > \"$DATOS/nieto\"\n{after}\nwait")
}

#[test]
fn on_timeout_also_dies_child_of_yt_dlp() {
    let fake = FalseYtDlp::new(&with_grandchild(": > \"$DATOS/listo\""));
    let result = fake.exhaust_time();
    assert!(
        matches!(result, Err(DownloadError::TimedOut { .. })),
        "{result:?}"
    );
    assert!(fake.grandchild_dies(), "el hijo de yt-dlp sigue vivo");
}

#[test]
fn on_size_overshoot_also_dies_child_of_yt_dlp() {
    let fake = FalseYtDlp::new(&with_grandchild(
        "head -c 3000000 /dev/zero > \"$DIR/v.mp4.part\"",
    ));
    let result = download_with(fake.downloader().with_max_size(1_000_000));
    assert!(
        matches!(result, Err(DownloadError::FileTooLarge { .. })),
        "{result:?}"
    );
    assert!(fake.grandchild_dies(), "el hijo de yt-dlp sigue vivo");
}

fn cancels_without_survivors(body: &str) {
    let fake = FalseYtDlp::new(body);
    let result = fake.cancel_when_this_ready();
    assert!(
        matches!(result, Err(DownloadError::Cancelled)),
        "{result:?}"
    );
    assert!(fake.grandchild_dies(), "el hijo de yt-dlp sigue vivo");
    assert!(!fake.temp().exists());
}

#[test]
fn on_cancel_also_dies_child_of_yt_dlp() {
    cancels_without_survivors(&with_grandchild(": > \"$DATOS/listo\""));
}

#[test]
fn can_cancel_although_yt_dlp_has_closed_its_output() {
    cancels_without_survivors(
        "sleep 120 >/dev/null 2>&1 &\n\
         echo $! > \"$DATOS/nieto\"\n\
         exec >/dev/null 2>&1\n\
         : > \"$DATOS/listo\"\n\
         wait",
    );
}

#[test]
fn can_cancel_although_only_child_retains_stderr() {
    cancels_without_survivors(
        "sleep 120 >/dev/null &\n\
         echo $! > \"$DATOS/nieto\"\n\
         exec >/dev/null 2>&1\n\
         : > \"$DATOS/listo\"\n\
         wait",
    );
}

#[test]
fn if_yt_dlp_dies_only_also_dies_its_child() {
    let fake = FalseYtDlp::new(
        "sleep 120 >/dev/null 2>&1 &\n\
         echo $! > \"$DATOS/nieto\"\n\
         kill -KILL $$",
    );
    let (result, _) = download(&fake, "https://ejemplo.org/v");
    assert!(
        matches!(
            result,
            Err(DownloadError::FailedDownload { code: None, .. })
        ),
        "{result:?}"
    );
    assert!(fake.grandchild_dies(), "el hijo de yt-dlp sigue vivo");
}

#[test]
fn if_yt_dlp_ends_well_also_dies_child_that_leaves() {
    let fake = FalseYtDlp::new(&format!(
        "sleep 120 >/dev/null 2>&1 &\necho $! > \"$DATOS/nieto\"\n{}",
        create_e_print("v.mp4")
    ));
    let (result, _) = download(&fake, "https://ejemplo.org/v");
    result.expect("descarga correcta");
    assert!(fake.grandchild_dies(), "el hijo de yt-dlp sigue vivo");
}

#[test]
fn child_that_retains_output_does_not_exhaust_time_of_download_correct() {
    for redirect in ["2>/dev/null", ">/dev/null", ""] {
        let fake = FalseYtDlp::new(&format!(
            "sleep 120 {redirect} &\necho $! > \"$DATOS/nieto\"\n{}",
            create_e_print("v.mp4")
        ));
        let result = download_with(fake.downloader().with_time_limit(ROOMY_LIMIT));
        assert!(result.is_ok(), "{redirect:?}: {result:?}");
        assert!(fake.grandchild_dies(), "{redirect:?}: el hijo sigue vivo");
    }
}

#[test]
fn child_that_retains_output_does_not_cover_failure_of_yt_dlp() {
    let fake = FalseYtDlp::new(
        "sleep 120 &\n\
         echo $! > \"$DATOS/nieto\"\n\
         echo 'ERROR: no se pudo unir' >&2\n\
         exit 1",
    );
    let result = download_with(fake.downloader().with_time_limit(ROOMY_LIMIT));
    match result {
        Err(DownloadError::FailedDownload {
            code,
            summarized_stderr,
        }) => {
            assert_eq!(code, Some(1));
            assert_eq!(summarized_stderr, "ERROR: no se pudo unir");
        }
        other => panic!("se esperaba DescargaFallida y llegó {other:?}"),
    }
    assert!(fake.grandchild_dies(), "el hijo de yt-dlp sigue vivo");
}

#[test]
fn if_not_prints_path_reason_mentions_direct() {
    let fake = FalseYtDlp::new("true");
    let (result, _) = download(&fake, "https://ejemplo.org/v");
    match result {
        Err(error @ DownloadError::FileNotLocated { .. }) => {
            assert!(
                error.to_string().contains("no se admiten directos"),
                "{error}"
            );
        }
        other => panic!("se esperaba ArchivoNoLocalizado y llegó {other:?}"),
    }
}

#[test]
fn if_exhausts_time_kills_yt_dlp() {
    let fake = FalseYtDlp::new(": > \"$DATOS/listo\"\nexec sleep 120");
    let result = fake.exhaust_time();
    match result {
        Err(error @ DownloadError::TimedOut { limit }) => {
            let seconds = format!(" {} s ", limit.as_secs());
            assert!(error.to_string().contains(&seconds), "{error}");
        }
        other => panic!("se esperaba TiempoAgotado y llegó {other:?}"),
    }
    assert!(!alive_process(&fake.pid()), "yt-dlp sigue vivo");
    assert!(!fake.temp().exists());
}

#[test]
fn if_temp_passes_of_cap_stops_and_deletes() {
    let fake = FalseYtDlp::new("head -c 3000000 /dev/zero > \"$DIR/v.mp4.part\"\nexec sleep 120");
    let result = download_with(fake.downloader().with_max_size(1_000_000));
    assert!(
        matches!(
            result,
            Err(DownloadError::FileTooLarge {
                max_bytes: 1_000_000
            })
        ),
        "{result:?}"
    );
    assert!(!alive_process(&fake.pid()), "yt-dlp sigue vivo");
    assert!(!fake.temp().exists());
}

#[test]
fn size_measures_although_yt_dlp_has_closed_its_output() {
    let fake = FalseYtDlp::new(
        "exec >/dev/null\n\
         sleep 120 &\n\
         echo $! > \"$DATOS/nieto\"\n\
         head -c 3000000 /dev/zero > \"$DIR/v.mp4.part\"\n\
         wait",
    );
    let result = download_with(
        fake.downloader()
            .with_max_size(1_000_000)
            .with_time_limit(ROOMY_LIMIT),
    );
    assert!(
        matches!(result, Err(DownloadError::FileTooLarge { .. })),
        "{result:?}"
    );
    assert!(fake.grandchild_dies(), "el hijo de yt-dlp sigue vivo");
    assert!(!fake.temp().exists());
}

#[test]
fn video_finished_greater_that_cap_also_rejects() {
    let fake = FalseYtDlp::new("head -c 2000 /dev/zero > \"$DIR/v.mp4\"\necho \"$DIR/v.mp4\"");
    let result = download_with(fake.downloader().with_max_size(1000));
    assert!(
        matches!(result, Err(DownloadError::FileTooLarge { .. })),
        "{result:?}"
    );
    assert!(!fake.temp().exists());
}

#[test]
fn enable_cancellation_stops_download() {
    let fake = FalseYtDlp::new(": > \"$DATOS/listo\"\nexec sleep 120");
    let result = fake.cancel_when_this_ready();
    assert!(
        matches!(result, Err(DownloadError::Cancelled)),
        "{result:?}"
    );
    assert!(!alive_process(&fake.pid()), "yt-dlp sigue vivo");
    assert!(!fake.temp().exists());
}

#[test]
fn lines_of_progress_arrive_and_garbage_does_not_shorten_download() {
    let fake = FalseYtDlp::new(&format!(
        "echo 'conversor-progreso 1024 4096 NA'\n\
         echo 'conversor-progreso basura'\n\
         echo 'conversor-progreso'\n\
         echo 'conversor-progreso 2048 NA 5000.7'\n\
         echo 'conversor-progreso NA NA NA'\n\
         echo 'conversor-progreso 3000 NA NA'\n\
         echo 'conversor-progreso -5 10 NA'\n\
         {}",
        create_e_print("v.mp4")
    ));
    let (result, progress_updates) = download(&fake, "https://ejemplo.org/v");
    result.expect("descarga correcta");
    assert_eq!(
        progress_updates,
        [
            DownloadProgress::Downloading {
                bytes: 1024,
                total: Some(4096)
            },
            DownloadProgress::Downloading {
                bytes: 2048,
                total: Some(5000)
            },
            DownloadProgress::Downloading {
                bytes: 3000,
                total: None
            },
        ]
    );
}

const PUBLIC_DOMAIN_VIDEO: &str = "https://images-assets.nasa.gov/video/\
    jsc2022m000270_Orion_First_Imagery_Timelapse_and_Original_221117/\
    jsc2022m000270_Orion_First_Imagery_Timelapse_and_Original_221117~mobile.mp4";

#[test]
#[ignore = "necesita red y yt-dlp; se ejecuta con --ignored"]
fn with_red_download_video_real_and_converts_in_sticker() {
    let progress_updates = std::sync::Mutex::new(Vec::new());
    let video = YtDlpDownloader::new()
        .download(
            &link(PUBLIC_DOMAIN_VIDEO),
            &|progress| progress_updates.lock().expect("mutex").push(progress),
            &AtomicBool::new(false),
        )
        .unwrap_or_else(|error| panic!("la descarga falló: {error}"));
    assert!(
        video.name_base().starts_with("jsc2022m000270_Orion"),
        "{}",
        video.name_base()
    );
    assert!(
        !progress_updates.into_inner().expect("mutex").is_empty(),
        "no llegó ninguna línea de progreso"
    );

    let options = ConversionOptions::new(&ConversionParams {
        preset: WHATSAPP_STICKER,
        max_seconds: Some(3.0),
        start_s: None,
        fps: None,
        automatic_fitting: false,
        max_mb: None,
        preview: false,
    })
    .expect("opciones válidas");
    let report = convert(
        &FfmpegDecoder::new(video.path()),
        &options,
        Tracking::new(&|_| {}, &AtomicBool::new(false)),
    )
    .unwrap_or_else(|error| panic!("la conversión falló: {error}"));
    assert!(report.size_bytes() <= 500_000, "{}", report.size_bytes());
    assert_eq!(&report.bytes[..4], b"RIFF");
    assert_eq!(&report.bytes[8..12], b"WEBP");
}
