#![allow(clippy::panic, clippy::expect_used)]

#[path = "../../core/tests/common/mod.rs"]
mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use conversor_core::PRESETS;
use tempfile::TempDir;

use common::{TestVideo, read_apng, read_gif, read_webp};

fn directory_with_video(video: &TestVideo, name: &str) -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("directorio temporal");
    let path = directory.path().join(name);
    fs::copy(video.path(), &path).expect("se puede copiar el video");
    (directory, path)
}

fn converter(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_conversor-stickers"));
    command.current_dir(directory);
    command
}

fn run(directory: &Path, arguments: &[&str]) -> Output {
    converter(directory)
        .args(arguments)
        .output()
        .expect("se puede ejecutar la CLI")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or_else(|| {
        panic!(
            "la CLI terminó por una señal; stderr: {}",
            text(&output.stderr)
        )
    })
}

fn files(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("se puede listar el directorio")
        .map(|input| {
            input
                .expect("entrada")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn converts_without_more_options_that_preset() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(3.0), "video.mp4");
    let output = run(
        directory.path(),
        &["video.mp4", "--preset", "whatsapp-sticker"],
    );
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));

    assert_eq!(files(directory.path()), ["video.mp4", "video.webp"]);
    let animation = read_webp(&directory.path().join("video.webp"));
    assert_eq!((animation.width, animation.height), (512, 512));
    assert!(animation.size_bytes <= 500_000);
    let stdout = text(&output.stdout);
    for expected in [
        "Creado video.webp",
        "Peso:",
        "Duración:     3,0 s",
        "fps:",
        "Lienzo:       512x512",
        "Reducciones:",
    ] {
        assert!(
            stdout.contains(expected),
            "falta {expected:?} en:\n{stdout}"
        );
    }
}

#[test]
fn discord_sticker_writes_apng_with_extension_png() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(3.0), "gato.mp4");
    let output = run(
        directory.path(),
        &["gato.mp4", "--preset", "discord-sticker"],
    );
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));

    assert_eq!(files(directory.path()), ["gato.mp4", "gato.png"]);
    let animation = read_apng(&directory.path().join("gato.png"));
    assert_eq!((animation.width, animation.height), (320, 320));
    assert!(animation.size_bytes <= 512_000);
}

#[test]
fn dry_run_writes_nothing_e_reports_of_plan() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(3.0), "gato.mp4");
    let output = run(
        directory.path(),
        &[
            "gato.mp4",
            "--preset",
            "whatsapp-sticker",
            "--fit",
            "--max-seconds",
            "2",
            "--preview",
            "--dry-run",
        ],
    );
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    assert_eq!(files(directory.path()), ["gato.mp4"]);
    let stdout = text(&output.stdout);
    for expected in [
        "no se ha escrito nada",
        "Tramo:        0,0 s → 3,0 s del video (3,0 s)",
        "Velocidad:    1,5x (--fit)",
        "Duración:     2,0 s",
        "fps:          15 (30 frames",
        "Salida:       gato.webp",
        "Vista previa: gato.preview.webp",
    ] {
        assert!(
            stdout.contains(expected),
            "falta {expected:?} en:\n{stdout}"
        );
    }
}

#[test]
fn summary_list_reductions_or_says_that_there_was_none() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(3.0), "gato.mp4");
    let output = run(
        directory.path(),
        &[
            "gato.mp4",
            "--preset",
            "discord-gif",
            "--max-mb",
            "0,5",
            "-o",
            "chico.gif",
        ],
    );
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let reductions = stdout
        .lines()
        .find(|line| line.contains("Reducciones:"))
        .unwrap_or_else(|| panic!("no hay línea de reducciones en:\n{stdout}"));
    assert!(reductions.contains("fps 15→"), "{reductions}");
    assert!(!reductions.contains("sin reducciones"), "{reductions}");
    let gif = read_gif(&directory.path().join("chico.gif"));
    assert!(gif.size_bytes <= 500_000);

    let output = run(directory.path(), &["gato.mp4", "--preset", "discord-gif"]);
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    assert!(
        text(&output.stdout).contains("Reducciones:  sin reducciones"),
        "{}",
        text(&output.stdout)
    );
}

#[test]
fn each_error_of_arguments_or_of_input_returns_its_code() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(3.0), "gato.mp4");
    let cases: [(&[&str], i32, &str); 7] = [
        (
            &["gato.mp4", "--preset", "whatsapp-sticker", "-o", "gato.gif"],
            2,
            ".webp",
        ),
        (
            &["gato.mp4", "--preset", "discord-sticker", "-o", "gato.apng"],
            2,
            ".png",
        ),
        (
            &["no-existe.mp4", "--preset", "whatsapp-sticker"],
            3,
            "no existe el video",
        ),
        (
            &["gato.mp4", "--preset", "whatsapp-sticker", "--max-mb", "1"],
            2,
            "--max-mb",
        ),
        (&["gato.mp4", "--preset", "telegram"], 2, "whatsapp-sticker"),
        (
            &["gato.mp4", "--preset", "whatsapp-sticker", "--fps", "60"],
            2,
            "--fps",
        ),
        (
            &["gato.mp4", "--preset", "whatsapp-sticker", "--start", "9"],
            2,
            "--start",
        ),
    ];
    for (arguments, expected, message) in cases {
        let output = run(directory.path(), arguments);
        let stderr = text(&output.stderr);
        assert_eq!(code(&output), expected, "{arguments:?}: {stderr}");
        assert!(stderr.contains(message), "{arguments:?}: {stderr}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
    }
    assert_eq!(files(directory.path()), ["gato.mp4"]);
}

#[test]
fn without_ffmpeg_in_path_returns_code_4() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(3.0), "gato.mp4");
    let output = converter(directory.path())
        .args(["gato.mp4", "--preset", "whatsapp-sticker"])
        .env("PATH", directory.path())
        .output()
        .expect("se puede ejecutar la CLI");
    let stderr = text(&output.stderr);
    assert_eq!(code(&output), 4, "{stderr}");
    assert!(stderr.contains("instala FFmpeg"), "{stderr}");
}

#[test]
fn does_not_fit_even_accelerated_exits_by_stderr_with_code_5() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(25.0), "largo.mp4");
    let output = run(
        directory.path(),
        &["largo.mp4", "--preset", "whatsapp-sticker", "--fit"],
    );
    let stderr = text(&output.stderr);
    assert_eq!(code(&output), 5, "{stderr}");
    assert!(stderr.contains("Recorta al menos 5,0 s"), "{stderr}");
    assert!(output.stdout.is_empty());
    assert_eq!(files(directory.path()), ["largo.mp4"]);
}

#[test]
fn help_is_in_spanish_with_examples_presets_and_codes() {
    let directory = tempfile::tempdir().expect("directorio temporal");
    let output = run(directory.path(), &["--help"]);
    assert_eq!(code(&output), 0);
    let help = text(&output.stdout);
    for expected in [
        "Uso: conversor-stickers <ENTRADA>... --preset <PRESET> [-o SALIDA | -d DIR]",
        "Argumentos:",
        "Opciones:",
        "Ejemplos:",
        "conversor-stickers gato.mp4 --preset whatsapp-sticker",
        "conversor-stickers gato.mp4 --preset discord-emoji",
        "conversor-stickers gato.mp4 --preset discord-sticker",
        "Códigos de salida:",
        "130  cancelado con Ctrl-C",
        "Muestra esta ayuda",
    ] {
        assert!(help.contains(expected), "falta {expected:?} en:\n{help}");
    }
    for preset in PRESETS {
        assert!(help.contains(preset.name), "{}", preset.name);
        assert!(help.contains(preset.description), "{}", preset.name);
    }
    for code in ["0", "1", "2", "3", "4", "5", "6", "7"] {
        assert!(
            help.lines()
                .any(|line| line.trim_start().starts_with(&format!("{code}  "))),
            "falta el código {code}"
        );
    }
    for english in ["Usage:", "Options:", "Print help", "Possible values"] {
        assert!(!help.contains(english), "queda {english:?} en la ayuda");
    }
}

fn processes_that_mention(text: &str) -> Vec<String> {
    fs::read_dir("/proc")
        .expect("se puede leer /proc")
        .filter_map(|input| {
            let path = input.ok()?.path().join("cmdline");
            let line = fs::read(path).ok()?;
            let line = String::from_utf8_lossy(&line).replace('\0', " ");
            line.contains(text).then_some(line)
        })
        .collect()
}

#[test]
fn ctrl_c_cancels_with_code_130_without_leave_ffmpeg_nor_files() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(25.0), "largo.mp4");
    let video = directory.path().join("largo.mp4");
    let child = converter(directory.path())
        .arg(&video)
        .args([
            "--preset",
            "discord-gif",
            "--max-seconds",
            "25",
            "--fps",
            "30",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("se puede lanzar la CLI");

    let start = Instant::now();
    while processes_that_mention(&video.to_string_lossy())
        .iter()
        .all(|line| !line.starts_with("ffmpeg"))
    {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "ffmpeg no llegó a arrancar"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let state = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("se puede ejecutar kill");
    assert!(state.success());

    let output = child.wait_with_output().expect("la CLI termina");
    let stderr = text(&output.stderr);
    assert_eq!(code(&output), 130, "{stderr}");
    assert!(stderr.contains("cancelada"), "{stderr}");
    assert_eq!(files(directory.path()), ["largo.mp4"]);
    assert_eq!(
        processes_that_mention(&video.to_string_lossy()),
        Vec::<String>::new()
    );
}

fn generate_bait(directory: &Path, name: &str) {
    let state = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(["-f", "lavfi", "-i", "color=c=red:size=64x48:rate=30:d=2"])
        .args(["-c:v", "mpeg2video", "-f", "mpegts"])
        .arg(directory.join(name))
        .status()
        .expect("ffmpeg está instalado");
    assert!(state.success(), "ffmpeg no pudo generar {name}");
}

#[test]
fn name_with_dash_or_protocol_reads_as_own_file() {
    let video = TestVideo::horizontal(2.0);
    let convert_with_name = |name: &str| {
        let (directory, _) = directory_with_video(&video, name);
        generate_bait(directory.path(), "a.ts");
        generate_bait(directory.path(), "b.ts");
        let output = run(
            directory.path(),
            &["--preset", "discord-gif", "-o", "salida.gif", "--", name],
        );
        let stdout = text(&output.stdout);
        assert_eq!(code(&output), 0, "{name}: {}", text(&output.stderr));
        assert!(stdout.contains("Duración:     2,0 s"), "{name}: {stdout}");
        let mut expected = vec!["a.ts", "b.ts", name, "salida.gif"];
        expected.sort_unstable();
        assert_eq!(files(directory.path()), expected, "{name}");
        fs::read(directory.path().join("salida.gif")).expect("la salida existe")
    };

    let reference = convert_with_name("normal.mp4");
    for name in ["-report", "-foo.mp4", "concat:a.ts|b.ts", "http:x.mp4"] {
        assert!(
            convert_with_name(name) == reference,
            "{name}: la salida no es la del propio video"
        );
    }
}

#[test]
fn with_preview_writes_preview_before_that_output() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(4.0), "gato.mp4");
    let output = run(
        directory.path(),
        &[
            "gato.mp4",
            "--preset",
            "whatsapp-sticker",
            "--fit",
            "--max-seconds",
            "2",
            "--preview",
        ],
    );
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    assert_eq!(
        files(directory.path()),
        ["gato.mp4", "gato.preview.webp", "gato.webp"]
    );
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains("Vista previa: gato.preview.webp"),
        "{stdout}"
    );
    let preview = read_webp(&directory.path().join("gato.preview.webp"));
    assert_eq!((preview.width, preview.height), (160, 160));
    let modified = |name: &str| {
        fs::metadata(directory.path().join(name))
            .and_then(|metadata| metadata.modified())
            .expect("fecha de modificación")
    };
    assert!(modified("gato.preview.webp") <= modified("gato.webp"));
}

#[test]
fn bad_input_returns_code_3_before_any_other_error() {
    let directory = tempfile::tempdir().expect("directorio temporal");
    fs::create_dir(directory.path().join("carpeta.mp4")).expect("se puede crear");
    let cases: [(&[&str], &str); 2] = [
        (
            &["carpeta.mp4", "--preset", "whatsapp-sticker"],
            "no es un archivo normal",
        ),
        (
            &[
                "no-existe.mp4",
                "--preset",
                "whatsapp-sticker",
                "-o",
                "x.gif",
                "--max-mb",
                "1",
            ],
            "no existe el video",
        ),
    ];
    for (arguments, message) in cases {
        let output = run(directory.path(), arguments);
        let stderr = text(&output.stderr);
        assert_eq!(code(&output), 3, "{arguments:?}: {stderr}");
        assert!(stderr.contains(message), "{arguments:?}: {stderr}");
    }
}

#[test]
fn errors_start_in_lowercase_after_error() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(25.0), "largo.mp4");
    let output = run(
        directory.path(),
        &["largo.mp4", "--preset", "whatsapp-sticker", "--fit"],
    );
    let stderr = text(&output.stderr);
    assert!(
        stderr.starts_with("error: el tramo dura 25,0 s"),
        "{stderr}"
    );
    assert!(stderr.contains("(por ejemplo, --start 5)."), "{stderr}");
}

const FAST: [&str; 4] = ["--preset", "discord-emoji", "--max-seconds", "1"];

fn with_fast<'a>(inputs: &[&'a str], extra: &[&'a str]) -> Vec<&'a str> {
    let mut arguments = inputs.to_vec();
    arguments.extend(FAST);
    arguments.extend(extra);
    arguments
}

#[test]
fn two_videos_give_two_files_and_summary() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(2.0), "a.mp4");
    fs::copy(
        directory.path().join("a.mp4"),
        directory.path().join("b.mp4"),
    )
    .expect("copia");
    let output = run(directory.path(), &with_fast(&["a.mp4", "b.mp4"], &[]));
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    assert_eq!(
        files(directory.path()),
        ["a.gif", "a.mp4", "b.gif", "b.mp4"]
    );
    let stdout = text(&output.stdout);
    assert!(stdout.contains("Creado a.gif"), "{stdout}");
    assert!(stdout.contains("Creado b.gif"), "{stdout}");
    assert!(stdout.ends_with("2 de 2 convertidas\n"), "{stdout}");
}

#[test]
fn input_that_fails_does_not_stop_queue_and_returns_its_code() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(2.0), "a.mp4");
    fs::copy(
        directory.path().join("a.mp4"),
        directory.path().join("b.mp4"),
    )
    .expect("copia");
    let output = run(
        directory.path(),
        &with_fast(&["a.mp4", "no-existe.mp4", "b.mp4"], &[]),
    );
    let stderr = text(&output.stderr);
    assert_eq!(code(&output), 3, "{stderr}");
    assert!(
        stderr.contains("error en no-existe.mp4: no existe el video"),
        "{stderr}"
    );
    assert_eq!(
        files(directory.path()),
        ["a.gif", "a.mp4", "b.gif", "b.mp4"]
    );
    assert!(
        text(&output.stdout).ends_with("2 de 3 convertidas\n"),
        "{}",
        text(&output.stdout)
    );
}

#[test]
fn output_with_several_inputs_returns_2_without_convert_nothing() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(2.0), "a.mp4");
    fs::copy(
        directory.path().join("a.mp4"),
        directory.path().join("b.mp4"),
    )
    .expect("copia");
    let output = run(
        directory.path(),
        &with_fast(&["a.mp4", "b.mp4"], &["-o", "x.gif"]),
    );
    let stderr = text(&output.stderr);
    assert_eq!(code(&output), 2, "{stderr}");
    assert!(stderr.contains("-o"), "{stderr}");
    assert!(output.stdout.is_empty());
    assert_eq!(files(directory.path()), ["a.mp4", "b.mp4"]);
}

#[test]
fn dir_sets_outputs_in_that_directory() {
    let (directory, _) = directory_with_video(&TestVideo::horizontal(2.0), "a.mp4");
    fs::create_dir(directory.path().join("videos")).expect("se puede crear");
    fs::rename(
        directory.path().join("a.mp4"),
        directory.path().join("videos").join("a.mp4"),
    )
    .expect("se puede mover");
    fs::create_dir(directory.path().join("salidas")).expect("se puede crear");
    let output = run(
        directory.path(),
        &with_fast(&["videos/a.mp4"], &["-d", "salidas"]),
    );
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    assert_eq!(files(&directory.path().join("salidas")), ["a.gif"]);
    assert_eq!(files(&directory.path().join("videos")), ["a.mp4"]);
    assert!(
        text(&output.stdout).contains("Creado salidas/a.gif"),
        "{}",
        text(&output.stdout)
    );

    let output = run(
        directory.path(),
        &with_fast(&["videos/a.mp4"], &["-d", "no-existe"]),
    );
    assert_eq!(code(&output), 2, "{}", text(&output.stderr));
    assert!(text(&output.stderr).contains("no existe el directorio de salida"));
}

struct FalseYtDlp {
    directory: TempDir,
}

impl FalseYtDlp {
    fn new(video: &TestVideo) -> Self {
        let directory = tempfile::tempdir().expect("directorio temporal");
        let data = directory.path().display();
        let script = format!(
            "#!/bin/sh\n\
             echo ejecutado >> '{data}/ejecuciones'\n\
             DIR=''\n\
             previo=''\n\
             for arg in \"$@\"; do\n\
               if [ \"$previo\" = '-P' ]; then DIR=\"$arg\"; fi\n\
               previo=\"$arg\"\n\
               ultimo=\"$arg\"\n\
             done\n\
             printf '%s' \"$DIR\" > '{data}/temporal'\n\
             case \"$ultimo\" in\n\
               *falla*) echo 'ERROR: [generic] HTTP Error 404' >&2; exit 1 ;;\n\
             esac\n\
             echo 'conversor-progreso 1000 2000 NA'\n\
             cp '{}' \"$DIR/Gato_bailando.mp4\"\n\
             echo \"$DIR/Gato_bailando.mp4\"\n",
            video.path().display()
        );
        let source = directory.path().join("yt-dlp.sh");
        fs::write(&source, script).expect("se puede escribir el script");
        fs::create_dir(directory.path().join("bin")).expect("se puede crear");
        let state = Command::new("install")
            .args(["-m", "755"])
            .arg(&source)
            .arg(directory.path().join("bin").join("yt-dlp"))
            .status()
            .expect("se puede ejecutar install");
        assert!(state.success(), "install falló");
        FalseYtDlp { directory }
    }

    fn path(&self) -> String {
        format!(
            "{}:{}",
            self.directory.path().join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        )
    }

    fn runs(&self) -> usize {
        fs::read_to_string(self.directory.path().join("ejecuciones"))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    fn temp(&self) -> PathBuf {
        PathBuf::from(
            fs::read_to_string(self.directory.path().join("temporal"))
                .expect("el yt-dlp falso guardó el temporal"),
        )
    }

    fn run(&self, directory: &Path, arguments: &[&str]) -> Output {
        converter(directory)
            .args(arguments)
            .env("PATH", self.path())
            .output()
            .expect("se puede ejecutar la CLI")
    }
}

const LINK: &str = "https://ejemplo.org/video?id=1";

#[test]
fn link_returns_title_in_current_directory_without_overwrite() {
    let fake = FalseYtDlp::new(&TestVideo::horizontal(2.0));
    let directory = tempfile::tempdir().expect("directorio temporal");
    let output = fake.run(directory.path(), &with_fast(&[LINK], &[]));
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    assert_eq!(files(directory.path()), ["Gato_bailando.gif"]);
    assert!(
        text(&output.stdout).contains("Creado Gato_bailando.gif"),
        "{}",
        text(&output.stdout)
    );
    assert!(!fake.temp().exists(), "queda el temporal de la descarga");

    let output = fake.run(directory.path(), &with_fast(&[LINK], &["--preview"]));
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    assert_eq!(
        files(directory.path()),
        [
            "Gato_bailando.gif",
            "Gato_bailando_1.gif",
            "Gato_bailando_1.preview.webp"
        ]
    );
}

#[test]
fn link_and_file_in_same_order_give_two() {
    let video = TestVideo::horizontal(2.0);
    let fake = FalseYtDlp::new(&video);
    let (directory, _) = directory_with_video(&video, "local.mp4");
    fs::create_dir(directory.path().join("salidas")).expect("se puede crear");
    let output = fake.run(
        directory.path(),
        &with_fast(&[LINK, "local.mp4"], &["-d", "salidas"]),
    );
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    assert_eq!(
        files(&directory.path().join("salidas")),
        ["Gato_bailando.gif", "local.gif"]
    );
    assert!(text(&output.stdout).ends_with("2 de 2 convertidas\n"));
}

#[test]
fn link_that_is_not_http_returns_2_without_launch_yt_dlp() {
    let video = TestVideo::horizontal(2.0);
    let fake = FalseYtDlp::new(&video);
    let (directory, _) = directory_with_video(&video, "local.mp4");
    for inputs in [&["ftp://x"][..], &["local.mp4", "file:///etc/passwd"][..]] {
        let output = fake.run(directory.path(), &with_fast(inputs, &[]));
        let stderr = text(&output.stderr);
        assert_eq!(code(&output), 2, "{inputs:?}: {stderr}");
        assert!(stderr.contains("solo se admiten enlaces http"), "{stderr}");
        assert_eq!(files(directory.path()), ["local.mp4"], "{inputs:?}");
    }
    assert_eq!(fake.runs(), 0, "se ejecutó yt-dlp");
}

#[test]
fn without_yt_dlp_in_path_returns_7_with_installation_message() {
    let directory = tempfile::tempdir().expect("directorio temporal");
    let output = converter(directory.path())
        .args(with_fast(&[LINK], &[]))
        .env("PATH", directory.path())
        .output()
        .expect("se puede ejecutar la CLI");
    let stderr = text(&output.stderr);
    assert_eq!(code(&output), 7, "{stderr}");
    assert!(stderr.contains("instala yt-dlp"), "{stderr}");
}

#[test]
fn download_failed_returns_7_and_queue_follows() {
    let video = TestVideo::horizontal(2.0);
    let fake = FalseYtDlp::new(&video);
    let (directory, _) = directory_with_video(&video, "local.mp4");
    let output = fake.run(
        directory.path(),
        &with_fast(&["https://ejemplo.org/falla", "local.mp4"], &[]),
    );
    let stderr = text(&output.stderr);
    assert_eq!(code(&output), 7, "{stderr}");
    assert!(
        stderr.contains("error en https://ejemplo.org/falla: yt-dlp no pudo descargar"),
        "{stderr}"
    );
    assert!(stderr.contains("HTTP Error 404"), "{stderr}");
    assert_eq!(files(directory.path()), ["local.gif", "local.mp4"]);
    assert!(!fake.temp().exists());
}

#[test]
fn dry_run_with_link_download_to_temp_and_leaves_nothing() {
    let fake = FalseYtDlp::new(&TestVideo::horizontal(2.0));
    let directory = tempfile::tempdir().expect("directorio temporal");
    let output = fake.run(
        directory.path(),
        &with_fast(&[LINK], &["--dry-run", "--preview"]),
    );
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    let stdout = text(&output.stdout);
    assert!(stdout.contains("Tramo:        0,0 s → 1,0 s"), "{stdout}");
    assert!(
        stdout.contains("Salida:       Gato_bailando.gif"),
        "{stdout}"
    );
    assert!(files(directory.path()).is_empty());
    assert!(!fake.temp().exists());
}

#[test]
fn preview_that_already_exists_does_not_overwrite_with_link() {
    let fake = FalseYtDlp::new(&TestVideo::horizontal(2.0));
    let directory = tempfile::tempdir().expect("directorio temporal");
    let preview = directory.path().join("Gato_bailando.preview.webp");
    fs::write(&preview, b"de antes").expect("se puede escribir");
    let output = fake.run(directory.path(), &with_fast(&[LINK], &["--preview"]));
    assert_eq!(code(&output), 0, "stderr: {}", text(&output.stderr));
    assert_eq!(fs::read(&preview).expect("existe"), b"de antes");
    assert_eq!(
        files(directory.path()),
        [
            "Gato_bailando.preview.webp",
            "Gato_bailando_1.gif",
            "Gato_bailando_1.preview.webp"
        ]
    );
}

#[test]
fn options_and_output_of_link_validate_before_of_download() {
    let fake = FalseYtDlp::new(&TestVideo::horizontal(2.0));
    let directory = tempfile::tempdir().expect("directorio temporal");
    let cases: [(&[&str], &str); 2] = [(&["--fps", "99"], "--fps"), (&["-o", "x.webp"], ".gif")];
    for (extra, message) in cases {
        let output = fake.run(directory.path(), &with_fast(&[LINK], extra));
        let stderr = text(&output.stderr);
        assert_eq!(code(&output), 2, "{extra:?}: {stderr}");
        assert!(stderr.contains(message), "{extra:?}: {stderr}");
    }
    assert_eq!(fake.runs(), 0, "se ejecutó yt-dlp");
    assert!(files(directory.path()).is_empty());
}
