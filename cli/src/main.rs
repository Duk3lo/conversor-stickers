mod arguments;
mod catalog;
mod codes;
mod summary;
mod writing;

use std::ffi::OsStr;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use conversor_core::{
    ConversionError, ConversionOptions, ConversionParams, DownloadError, DownloadProgress,
    DownloadedVideo, FfmpegDecoder, Link, OptionsError, Progress, Tracking, YtDlpDownloader,
    convert, looks_like_link, plan, readable_size,
};
use indicatif::{ProgressBar, ProgressStyle};
use thiserror::Error;
use tracing::warn;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

use crate::arguments::Arguments;
use crate::writing::{preview_path, write_atomic, write_without_overwrite};

static CANCELLATION: AtomicBool = AtomicBool::new(false);

const BAR_TEMPLATE: &str = "{spinner} {msg} [{elapsed}]";

#[derive(Debug, Error)]
enum CliError {
    #[error(
        "la salida {} tiene que tener la extensión .{expected} del preset {preset}",
        path.display()
    )]
    ExtensionIncorrect {
        path: PathBuf,
        expected: &'static str,
        preset: &'static str,
    },

    #[error(
        "la salida {} es el propio video de entrada; elige otra con -o",
        path.display()
    )]
    OutputEqualThatInput { path: PathBuf },

    #[error("no existe el directorio de salida {}", path.display())]
    DirectoryOutputNotExists { path: PathBuf },

    #[error("-o solo se admite con una entrada; con varias, usa -d para elegir el directorio")]
    OutputWithSeveralInputs,

    #[error(transparent)]
    Options(#[from] OptionsError),

    #[error(transparent)]
    Conversion(#[from] ConversionError),

    #[error(transparent)]
    Download(#[from] DownloadError),

    #[error(
        "{} apareció mientras se convertía y no se sobrescribe; vuelve a ejecutar la orden",
        path.display()
    )]
    OutputAlreadyExists { path: PathBuf },

    #[error("no se pudo escribir {}: {source}", path.display())]
    Writing {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("no se pudo preparar la cancelación con Ctrl-C: {0}")]
    CtrlC(#[from] ctrlc::Error),

    #[error("no se pudo escribir en la salida estándar: {0}")]
    OutputStandard(#[source] io::Error),
}

impl CliError {
    fn code(&self) -> u8 {
        match self {
            CliError::ExtensionIncorrect { .. }
            | CliError::OutputEqualThatInput { .. }
            | CliError::DirectoryOutputNotExists { .. }
            | CliError::OutputWithSeveralInputs => codes::INVALID_ARGUMENTS,
            CliError::Options(error) => codes::options(error),
            CliError::Conversion(error) => codes::conversion(error),
            CliError::Download(error) => codes::download(error),
            CliError::OutputAlreadyExists { .. }
            | CliError::Writing { .. }
            | CliError::CtrlC(_)
            | CliError::OutputStandard(_) => codes::OTHER_ERROR,
        }
    }
}

fn main() -> ExitCode {
    let mut process_arguments = std::env::args_os().skip(1);
    if process_arguments
        .next()
        .is_some_and(|first| first == "catalog")
    {
        return ExitCode::from(catalog::run(process_arguments));
    }
    let arguments = Arguments::read();
    let bar = create_bar();
    start_logs(&bar);
    ExitCode::from(run(&arguments, &bar))
}

fn run(arguments: &Arguments, bar: &ProgressBar) -> u8 {
    if let Err(error) = prepare(arguments) {
        eprintln!("error: {error}");
        return error.code();
    }
    let total = arguments.inputs.len();
    let mut inputs = Vec::with_capacity(total);
    for text in &arguments.inputs {
        match classify(text) {
            Ok(input) => inputs.push((text, input)),
            Err(error) => {
                let error = CliError::from(error);
                report_error(total, text, &error);
                return error.code();
            }
        }
    }
    let mut converted = 0;
    let mut first_failure = None;
    for (text, input) in &inputs {
        bar.reset();
        match convert_input(arguments, input, bar) {
            Ok(()) => converted += 1,
            Err(error) => {
                bar.finish_and_clear();
                report_error(total, text, &error);
                let code = error.code();
                first_failure.get_or_insert(code);
                if code == codes::CANCELLED {
                    break;
                }
            }
        }
    }
    if total > 1
        && let Err(error) = write_stdout(&format!("{converted} de {total} convertidas\n"))
    {
        eprintln!("error: {error}");
        first_failure.get_or_insert(error.code());
    }
    first_failure.unwrap_or(codes::SUCCESS)
}

fn report_error(total: usize, input: &Path, error: &CliError) {
    if total == 1 {
        eprintln!("error: {error}");
    } else {
        eprintln!("error en {}: {error}", input.display());
    }
}

enum Input<'a> {
    File(&'a Path),
    Link(Link),
}

fn classify(text: &Path) -> Result<Input<'_>, DownloadError> {
    match text.to_str() {
        Some(link) if looks_like_link(link) => Ok(Input::Link(Link::new(link)?)),
        _ => Ok(Input::File(text)),
    }
}

fn prepare(arguments: &Arguments) -> Result<(), CliError> {
    if arguments.output.is_some() && arguments.inputs.len() > 1 {
        return Err(CliError::OutputWithSeveralInputs);
    }
    if let Some(directory) = &arguments.directory
        && !directory.is_dir()
    {
        return Err(CliError::DirectoryOutputNotExists {
            path: directory.clone(),
        });
    }
    ctrlc::set_handler(|| CANCELLATION.store(true, Ordering::Relaxed))?;
    Ok(())
}

fn convert_input(arguments: &Arguments, input: &Input, bar: &ProgressBar) -> Result<(), CliError> {
    let preset = arguments.preset;
    let extension = preset.format.extension();
    match input {
        Input::File(video) => {
            FfmpegDecoder::new(video)
                .validate_file()
                .map_err(ConversionError::from)?;
            let output = output_path(
                video,
                arguments.output.as_deref(),
                arguments.directory.as_deref(),
                extension,
            );
            validate_output(&output, video, extension, preset.name)?;
            let options = options(arguments)?;
            convert_video(arguments, &options, video, &output, Overwrite::Allowed, bar)
        }
        Input::Link(link) => {
            let options = options(arguments)?;
            if let Some(output) = &arguments.output {
                validate_destination(output, extension, preset.name)?;
            }
            let downloaded = download(link, bar)?;
            let (output, overwrite) = match &arguments.output {
                Some(output) => (output.clone(), Overwrite::Allowed),
                None => (
                    free_path(
                        arguments.directory.as_deref().unwrap_or(Path::new("")),
                        downloaded.name_base(),
                        extension,
                    ),
                    Overwrite::Forbidden,
                ),
            };
            validate_output(&output, downloaded.path(), extension, preset.name)?;
            convert_video(
                arguments,
                &options,
                downloaded.path(),
                &output,
                overwrite,
                bar,
            )
        }
    }
}

fn download(link: &Link, bar: &ProgressBar) -> Result<DownloadedVideo, CliError> {
    bar.set_message(download_message(link.host(), 0, None));
    bar.enable_steady_tick(Duration::from_millis(100));
    let progress = |advance: DownloadProgress| match advance {
        DownloadProgress::Downloading { bytes, total } => {
            bar.set_message(download_message(link.host(), bytes, total));
        }
    };
    Ok(YtDlpDownloader::new().download(link, &progress, &CANCELLATION)?)
}

fn download_message(host: &str, bytes: u64, total: Option<u64>) -> String {
    match total {
        Some(total) if total > 0 => format!(
            "Descargando de {host} ({} %)",
            bytes.saturating_mul(100) / total
        ),
        _ if bytes > 0 => format!("Descargando de {host} ({})", readable_size(bytes)),
        _ => format!("Descargando de {host}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overwrite {
    Allowed,
    Forbidden,
}

fn free_path(directory: &Path, name: &str, extension: &str) -> PathBuf {
    let taken = |path: &Path| {
        fs::symlink_metadata(path).is_ok() || fs::symlink_metadata(preview_path(path)).is_ok()
    };
    let mut path = directory.join(format!("{name}.{extension}"));
    let mut number = 1u64;
    while taken(&path) {
        path = directory.join(format!("{name}_{number}.{extension}"));
        number += 1;
    }
    path
}

fn options(arguments: &Arguments) -> Result<ConversionOptions, OptionsError> {
    ConversionOptions::new(&ConversionParams {
        preset: *arguments.preset,
        max_seconds: arguments.max_seconds,
        start_s: arguments.start,
        fps: arguments.fps,
        automatic_fitting: arguments.fit,
        max_mb: arguments.max_mb,
        preview: arguments.preview,
    })
}

fn convert_video(
    arguments: &Arguments,
    options: &ConversionOptions,
    video: &Path,
    output: &Path,
    overwrite: Overwrite,
    bar: &ProgressBar,
) -> Result<(), CliError> {
    let write = match overwrite {
        Overwrite::Allowed => write_atomic,
        Overwrite::Forbidden => write_without_overwrite,
    };
    let decoder = FfmpegDecoder::new(video);
    let preview = arguments.preview.then(|| preview_path(output));
    if let Some(path) = &preview {
        check_not_input(path, video)?;
    }

    if arguments.dry_run {
        let plan = plan(&decoder, options).map_err(|error| {
            if CANCELLATION.load(Ordering::Relaxed) {
                ConversionError::Cancelled
            } else {
                error
            }
        })?;
        return write_stdout(&summary::plan(&plan, options, output, preview.as_deref()));
    }

    bar.set_message("Inspeccionando el video");
    bar.enable_steady_tick(Duration::from_millis(100));
    let progress = |stage: Progress| bar.set_message(stage_message(stage));
    let save_preview = |bytes: &[u8]| match &preview {
        Some(path) => write(path, bytes).map_err(|source| {
            io::Error::new(source.kind(), format!("{}: {source}", path.display()))
        }),
        None => Ok(()),
    };
    let report = convert(
        &decoder,
        options,
        Tracking::new(&progress, &CANCELLATION).with_preview(&save_preview),
    )?;
    if CANCELLATION.load(Ordering::Relaxed) {
        return Err(ConversionError::Cancelled.into());
    }
    bar.set_message("Escribiendo el archivo");
    write(output, &report.bytes).map_err(|source| {
        if source.kind() == io::ErrorKind::AlreadyExists {
            CliError::OutputAlreadyExists {
                path: output.to_path_buf(),
            }
        } else {
            CliError::Writing {
                path: output.to_path_buf(),
                source,
            }
        }
    })?;
    bar.finish_and_clear();
    write_stdout(&summary::report(&report, output, preview.as_deref()))
}

fn output_path(
    video: &Path,
    output: Option<&Path>,
    directory: Option<&Path>,
    extension: &str,
) -> PathBuf {
    match (output, directory) {
        (Some(output), _) => output.to_path_buf(),
        (None, Some(directory)) => directory
            .join(Path::new(video.file_name().unwrap_or_default()).with_extension(extension)),
        (None, None) => video.with_extension(extension),
    }
}

fn stage_message(stage: Progress) -> String {
    match stage {
        Progress::Decoding => "Decodificando el video".to_string(),
        Progress::Preview => "Creando la vista previa".to_string(),
        Progress::FittingSize { attempt, max } => {
            format!("Ajustando el peso (intento {attempt}/{max})")
        }
        Progress::SearchingDuration { attempt, max } => {
            format!("No cabe; buscando una duración que quepa (intento {attempt}/{max})")
        }
    }
}

fn validate_output(
    output: &Path,
    video: &Path,
    extension: &'static str,
    preset: &'static str,
) -> Result<(), CliError> {
    validate_destination(output, extension, preset)?;
    check_not_input(output, video)
}

fn validate_destination(
    output: &Path,
    extension: &'static str,
    preset: &'static str,
) -> Result<(), CliError> {
    let extension_correct = output
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|current| current.eq_ignore_ascii_case(extension));
    if !extension_correct {
        return Err(CliError::ExtensionIncorrect {
            path: output.to_path_buf(),
            expected: extension,
            preset,
        });
    }
    let directory = match output.parent() {
        Some(directory) if !directory.as_os_str().is_empty() => directory,
        _ => Path::new("."),
    };
    if !directory.is_dir() {
        return Err(CliError::DirectoryOutputNotExists {
            path: directory.to_path_buf(),
        });
    }
    Ok(())
}

fn check_not_input(path: &Path, video: &Path) -> Result<(), CliError> {
    let same = match (fs::canonicalize(path), fs::canonicalize(video)) {
        (Ok(path), Ok(video)) => path == video,
        _ => false,
    };
    if same {
        Err(CliError::OutputEqualThatInput {
            path: path.to_path_buf(),
        })
    } else {
        Ok(())
    }
}

fn write_stdout(text: &str) -> Result<(), CliError> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(CliError::OutputStandard)
}

fn create_bar() -> ProgressBar {
    let bar = ProgressBar::new_spinner();
    if let Ok(style) = ProgressStyle::with_template(BAR_TEMPLATE) {
        bar.set_style(style);
    }
    bar
}

fn start_logs(bar: &ProgressBar) {
    let (filter, filter_error) = match EnvFilter::builder()
        .with_default_directive(LevelFilter::WARN.into())
        .from_env()
    {
        Ok(filter) => (filter, None),
        Err(error) => (EnvFilter::new("warn"), Some(error)),
    };
    let bar = bar.clone();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .with_ansi(io::stderr().is_terminal())
        .with_writer(move || WriterLogs(bar.clone()))
        .init();
    if let Some(error) = filter_error {
        warn!(%error, "RUST_LOG no es válido; se muestran solo los avisos");
    }
}

struct WriterLogs(ProgressBar);

impl Write for WriterLogs {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.suspend(|| io::stderr().write(bytes))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.suspend(|| io::stderr().flush())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_of_bar_is_valid() {
        assert!(ProgressStyle::with_template(BAR_TEMPLATE).is_ok());
    }

    #[test]
    fn path_of_output_is_or_or_video_with_extension_in_its_directory_or_in_d() {
        let video = Path::new("videos/gato.mp4");
        assert_eq!(
            output_path(video, Some(Path::new("x.webp")), None, "webp"),
            Path::new("x.webp")
        );
        assert_eq!(
            output_path(video, None, None, "webp"),
            Path::new("videos/gato.webp")
        );
        assert_eq!(
            output_path(video, None, Some(Path::new("salidas")), "gif"),
            Path::new("salidas/gato.gif")
        );
    }

    #[test]
    fn name_taken_of_title_does_not_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let dir = directory.path();
        assert_eq!(free_path(dir, "gato", "gif"), dir.join("gato.gif"));
        fs::write(dir.join("gato.gif"), b"").unwrap();
        assert_eq!(free_path(dir, "gato", "gif"), dir.join("gato_1.gif"));
        fs::write(dir.join("gato_1.gif"), b"").unwrap();
        std::os::unix::fs::symlink(dir.join("no-existe"), dir.join("gato_2.gif")).unwrap();
        assert_eq!(free_path(dir, "gato", "gif"), dir.join("gato_3.gif"));
        assert_eq!(free_path(dir, "gato", "webp"), dir.join("gato.webp"));
        fs::write(dir.join("perro.preview.webp"), b"").unwrap();
        assert_eq!(free_path(dir, "perro", "gif"), dir.join("perro_1.gif"));
    }

    #[test]
    fn message_of_download_returns_percent_or_downloaded() {
        assert_eq!(
            download_message("x.org", 500, Some(2000)),
            "Descargando de x.org (25 %)"
        );
        assert_eq!(
            download_message("x.org", 1_500_000, None),
            "Descargando de x.org (1,5 MB)"
        );
        assert_eq!(download_message("x.org", 0, None), "Descargando de x.org");
        assert_eq!(
            download_message("x.org", 0, Some(0)),
            "Descargando de x.org"
        );
    }

    #[test]
    fn input_with_shape_of_link_has_to_be_valid_link() {
        assert!(matches!(
            classify(Path::new("https://x.org/v")),
            Ok(Input::Link(_))
        ));
        assert!(matches!(
            classify(Path::new("gato.mp4")),
            Ok(Input::File(_))
        ));
        assert!(matches!(
            classify(Path::new("./ftp://x")),
            Ok(Input::File(_))
        ));
        assert!(matches!(
            classify(Path::new("ftp://x")),
            Err(DownloadError::LinkNotValid { .. })
        ));
    }

    #[test]
    fn output_must_have_extension_of_preset() {
        let directory = tempfile::tempdir().unwrap();
        let video = directory.path().join("gato.mp4");
        fs::write(&video, b"").unwrap();
        let output = |name: &str| directory.path().join(name);
        assert!(validate_output(&output("gato.webp"), &video, "webp", "w").is_ok());
        assert!(validate_output(&output("gato.WEBP"), &video, "webp", "w").is_ok());
        for name in ["gato.gif", "gato", "gato.webp.tmp"] {
            assert!(
                matches!(
                    validate_output(&output(name), &video, "webp", "w"),
                    Err(CliError::ExtensionIncorrect { .. })
                ),
                "{name}"
            );
        }
    }

    #[test]
    fn output_not_can_be_input_nor_be_in_directory_nonexistent() {
        let directory = tempfile::tempdir().unwrap();
        let video = directory.path().join("gato.webp");
        fs::write(&video, b"").unwrap();
        assert!(matches!(
            validate_output(&video, &video, "webp", "w"),
            Err(CliError::OutputEqualThatInput { .. })
        ));
        let other_path_al_video = directory.path().join(".").join("gato.webp");
        assert!(matches!(
            validate_output(&other_path_al_video, &video, "webp", "w"),
            Err(CliError::OutputEqualThatInput { .. })
        ));
        assert!(matches!(
            validate_output(
                &directory.path().join("no").join("x.webp"),
                &video,
                "webp",
                "w"
            ),
            Err(CliError::DirectoryOutputNotExists { .. })
        ));
    }
}
