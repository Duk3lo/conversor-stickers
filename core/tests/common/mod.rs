#![allow(dead_code)]
#![allow(clippy::panic, clippy::expect_used)]

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use conversor_core::{
    Canvas, ConversionError, ConversionOptions, ConversionParams, FfmpegDecoder, Format, Preset,
    Report, Tracking, convert,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Content {
    Pattern,
    Static,
    Noise,
    RandomNoise,
    CenterSquare {
        side_px: u32,
    },
    RedThenBlue,
    Halves,
    Mandelbrot,
    /// Still detailed background with a moving figure and film grain, like a typical anime clip.
    GrainyScene,
    /// Test pattern with temporal noise: the cost per frame varies, so a sample of the clip is off.
    NoisyPattern,
}

pub const COLOR_BACKGROUND: [u8; 3] = [0, 0, 255];
pub const COLOR_SQUARE: [u8; 3] = [255, 0, 0];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestVideo {
    pub content: Content,
    pub width: u32,
    pub height: u32,
    pub sar: (u32, u32),
    pub duration_s: f64,
    pub fps: u32,
}

impl TestVideo {
    pub fn new(content: Content, width: u32, height: u32, duration_s: f64) -> Self {
        TestVideo {
            content,
            width,
            height,
            sar: (1, 1),
            duration_s,
            fps: 30,
        }
    }

    pub fn horizontal(duration_s: f64) -> Self {
        Self::new(Content::Pattern, 640, 360, duration_s)
    }

    pub fn vertical(duration_s: f64) -> Self {
        Self::new(Content::Pattern, 360, 640, duration_s)
    }

    pub fn mandelbrot(width: u32, height: u32, duration_s: f64) -> Self {
        Self::new(Content::Mandelbrot, width, height, duration_s)
    }

    pub fn with_sar(self, numerator: u32, denominator: u32) -> Self {
        TestVideo {
            sar: (numerator, denominator),
            ..self
        }
    }

    pub fn file_name(&self) -> String {
        let content = match self.content {
            Content::Pattern => "patron".to_string(),
            Content::Static => "estatico".to_string(),
            Content::Noise => "ruido".to_string(),
            Content::RandomNoise => "ruido-aleatorio".to_string(),
            Content::CenterSquare { side_px } => format!("cuadrado{side_px}"),
            Content::RedThenBlue => "rojo-azul".to_string(),
            Content::Halves => "mitades".to_string(),
            Content::Mandelbrot => "mandelbrot".to_string(),
            Content::GrainyScene => "escena-con-grano-12".to_string(),
            Content::NoisyPattern => "patron-con-ruido-14".to_string(),
        };
        format!(
            "{content}-{}x{}-sar{}_{}-{}s-{}fps.mp4",
            self.width, self.height, self.sar.0, self.sar.1, self.duration_s, self.fps
        )
    }

    pub fn path(&self) -> PathBuf {
        in_cache(&self.file_name(), |destination| self.generate(destination))
    }

    fn filter_lavfi(&self) -> String {
        let (width, height, fps) = (self.width, self.height, self.fps);
        let source = match self.content {
            Content::Pattern => format!("testsrc2=size={width}x{height}:rate={fps}"),
            Content::Mandelbrot => format!("mandelbrot=size={width}x{height}:rate={fps}"),
            Content::GrainyScene => format!(
                "mandelbrot=size={width}x{height}:rate={fps},trim=end_frame=1,\
                 loop=loop=-1:size=1,setpts=N/{fps}/TB[fondo];\
                 testsrc2=size={side}x{side}:rate={fps}[figura];\
                 [fondo][figura]overlay=x='{x}+{x}*sin(t*2)':y='{y}+{y}*sin(t*3)',\
                 noise=alls=12:allf=t+u:all_seed=7",
                side = height / 2,
                x = width / 4,
                y = height / 4,
            ),
            Content::NoisyPattern => format!(
                "testsrc2=size={width}x{height}:rate={fps},noise=alls=14:allf=t+u:all_seed=3"
            ),
            Content::Static => format!("color=c=0x808080:size={width}x{height}:rate={fps}"),
            Content::Noise => format!(
                "testsrc2=size={width}x{height}:rate={fps},\
                 noise=alls=100:allf=t+u:all_seed=12345"
            ),
            Content::RandomNoise => format!(
                "nullsrc=size={width}x{height}:rate={fps},\
                 geq=r='random(1)*255':g='random(2)*255':b='random(3)*255'"
            ),
            Content::CenterSquare { side_px } => {
                let box_width = side_px * self.sar.1 / self.sar.0;
                let x = (width - box_width) / 2;
                let y = (height - side_px) / 2;
                let [fr, fg, fb] = COLOR_BACKGROUND;
                let [cr, cg, cb] = COLOR_SQUARE;
                format!(
                    "color=c=0x{fr:02x}{fg:02x}{fb:02x}:size={width}x{height}:rate={fps},\
                     drawbox=x={x}:y={y}:w={box_width}:h={side_px}:\
                     color=0x{cr:02x}{cg:02x}{cb:02x}:t=fill"
                )
            }
            Content::Halves => {
                let half = width / 2;
                format!(
                    "color=c=blue:size={width}x{height}:rate={fps},\
                     drawbox=x=0:y=0:w={half}:h={height}:color=red:t=fill"
                )
            }
            Content::RedThenBlue => {
                let half_s = self.duration_s / 2.0;
                format!(
                    "color=c=red:size={width}x{height}:rate={fps}:duration={half_s}[rojo];\
                     color=c=blue:size={width}x{height}:rate={fps}:duration={half_s}[azul];\
                     [rojo][azul]concat=n=2:v=1:a=0"
                )
            }
        };
        format!("{source},setsar={}/{}", self.sar.0, self.sar.1)
    }

    fn generate(&self, destination: &Path) {
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", &self.filter_lavfi()])
            .args(["-t", &self.duration_s.to_string()])
            .args(["-c:v", "mpeg4", "-q:v", "3", "-pix_fmt", "yuv420p"])
            .args(["-f", "mp4"])
            .arg(destination)
            .output()
            .unwrap_or_else(|e| panic!("{}", message_without_ffmpeg("ffmpeg", &e)));
        assert!(
            output.status.success(),
            "ffmpeg no pudo generar {}: {}",
            self.file_name(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageContent {
    Pattern,
    Noise,
    CenterBand,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Bmp,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestImage {
    pub content: ImageContent,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub sar: (u32, u32),
}

impl TestImage {
    pub fn png(content: ImageContent, width: u32, height: u32) -> Self {
        TestImage {
            content,
            format: ImageFormat::Png,
            width,
            height,
            sar: (1, 1),
        }
    }

    pub fn with_format(self, format: ImageFormat) -> Self {
        TestImage { format, ..self }
    }

    pub fn with_sar(self, numerator: u32, denominator: u32) -> Self {
        TestImage {
            sar: (numerator, denominator),
            ..self
        }
    }

    pub fn file_name(&self) -> String {
        let content = match self.content {
            ImageContent::Pattern => "patron",
            ImageContent::Noise => "ruido",
            ImageContent::CenterBand => "franja",
        };
        let extension = match self.format {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Webp => "webp",
            ImageFormat::Bmp => "bmp",
        };
        format!(
            "imagen-{content}-{}x{}-sar{}_{}.{extension}",
            self.width, self.height, self.sar.0, self.sar.1
        )
    }

    pub fn path(&self) -> PathBuf {
        in_cache(&self.file_name(), |destination| self.generate(destination))
    }

    fn generate(&self, destination: &Path) {
        let (width, height) = (self.width, self.height);
        let source = match self.content {
            ImageContent::Pattern => format!("testsrc2=size={width}x{height}:rate=1"),
            ImageContent::Noise => format!(
                "nullsrc=size={width}x{height}:rate=1,\
                 geq=r='random(1)*255':g='random(2)*255':b='random(3)*255',format=rgb24"
            ),
            ImageContent::CenterBand => format!(
                "color=c=black@0:size={width}x{height}:rate=1,format=rgba,\
                 drawbox=x={}:y=0:w={}:h={height}:color=red@1:t=fill:replace=1",
                width / 3,
                width / 3
            ),
        };
        let filter = format!("{source},setsar={}/{}", self.sar.0, self.sar.1);
        let encoder: &[&str] = match self.format {
            ImageFormat::Png => &["-c:v", "png", "-f", "image2pipe"],
            ImageFormat::Jpeg => &["-c:v", "mjpeg", "-pix_fmt", "yuvj420p", "-f", "image2pipe"],
            ImageFormat::Webp => &["-c:v", "libwebp", "-f", "webp"],
            ImageFormat::Bmp => &["-c:v", "bmp", "-f", "image2pipe"],
        };
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", &filter, "-frames:v", "1"])
            .args(encoder)
            .arg(destination)
            .output()
            .unwrap_or_else(|e| panic!("{}", message_without_ffmpeg("ffmpeg", &e)));
        assert!(
            output.status.success(),
            "ffmpeg no pudo generar {}: {}",
            self.file_name(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn in_cache(name: &str, generate: impl FnOnce(&Path)) -> PathBuf {
    let directory = directory_cache();
    let path = directory.join(name);
    if path.is_file() {
        return path;
    }
    fs::create_dir_all(&directory)
        .unwrap_or_else(|e| panic!("no se pudo crear {}: {e}", directory.display()));
    let temp = directory.join(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    generate(&temp);
    fs::rename(&temp, &path).unwrap_or_else(|e| {
        panic!(
            "no se pudo mover {} a {}: {e}",
            temp.display(),
            path.display()
        )
    });
    path
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn directory_cache() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
        .parent()
        .expect("CARGO_TARGET_TMPDIR está dentro del directorio target")
        .join("videos-prueba")
}

fn message_without_ffmpeg(program: &str, error: &std::io::Error) -> String {
    if error.kind() == ErrorKind::NotFound {
        format!(
            "no se encontró `{program}` en el PATH; estas pruebas lo necesitan. \
             Instálalo (por ejemplo `sudo apt-get install ffmpeg` o `sudo pacman -S ffmpeg`)"
        )
    } else {
        format!("no se pudo ejecutar `{program}`: {error}")
    }
}

pub fn ffprobe_duration(path: &Path) -> f64 {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration"])
        .args(["-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .unwrap_or_else(|e| panic!("{}", message_without_ffmpeg("ffprobe", &e)));
    assert!(
        output.status.success(),
        "ffprobe falló con {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse()
        .unwrap_or_else(|e| panic!("ffprobe devolvió una duración ilegible {text:?}: {e}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    pub width: u32,
    pub height: u32,
    pub durations_ms: Vec<u32>,
    pub size_bytes: u64,
}

impl Animation {
    pub fn frame_count(&self) -> usize {
        self.durations_ms.len()
    }

    pub fn total_duration_ms(&self) -> u64 {
        self.durations_ms.iter().map(|&ms| u64::from(ms)).sum()
    }

    pub fn min_duration_ms(&self) -> Option<u32> {
        self.durations_ms.iter().copied().min()
    }
}

pub fn read_webp(path: &Path) -> Animation {
    let bytes =
        fs::read(path).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
    let decoder = webp_animation::Decoder::new(&bytes)
        .unwrap_or_else(|e| panic!("{} no es un WebP animado válido: {e:?}", path.display()));
    let (width, height) = decoder.dimensions();
    let mut previous_end_ms = 0;
    let durations_ms = decoder
        .into_iter()
        .map(|frame| {
            let end_ms = frame.timestamp();
            let duration = u32::try_from(end_ms - previous_end_ms)
                .unwrap_or_else(|_| panic!("marcas de tiempo decrecientes en {}", path.display()));
            previous_end_ms = end_ms;
            duration
        })
        .collect();
    Animation {
        width,
        height,
        durations_ms,
        size_bytes: bytes.len() as u64,
    }
}

pub fn read_gif(path: &Path) -> Animation {
    let file =
        fs::File::open(path).unwrap_or_else(|e| panic!("no se pudo abrir {}: {e}", path.display()));
    let size_bytes = file
        .metadata()
        .unwrap_or_else(|e| panic!("no se pudo leer el tamaño de {}: {e}", path.display()))
        .len();
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = options
        .read_info(std::io::BufReader::new(file))
        .unwrap_or_else(|e| panic!("{} no es un GIF válido: {e}", path.display()));
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    let mut durations_ms = Vec::new();
    while let Some(frame) = decoder
        .read_next_frame()
        .unwrap_or_else(|e| panic!("GIF dañado en {}: {e}", path.display()))
    {
        durations_ms.push(u32::from(frame.delay) * 10);
    }
    Animation {
        width,
        height,
        durations_ms,
        size_bytes,
    }
}

pub fn read_apng(path: &Path) -> Animation {
    let bytes =
        fs::read(path).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes.as_slice()))
        .read_info()
        .unwrap_or_else(|e| panic!("{} no es un PNG válido: {e}", path.display()));
    let (width, height) = (reader.info().width, reader.info().height);
    let control = reader
        .info()
        .animation_control
        .unwrap_or_else(|| panic!("{} no es un APNG: falta acTL", path.display()));
    let mut durations_ms = Vec::new();
    for index in 0..control.num_frames {
        let frame = if index == 0 {
            reader.info().frame_control.unwrap_or_else(|| {
                panic!(
                    "{}: la imagen por defecto no es el primer frame",
                    path.display()
                )
            })
        } else {
            *reader
                .next_frame_info()
                .unwrap_or_else(|e| panic!("APNG dañado en {}: {e}", path.display()))
        };
        let denominator = match frame.delay_den {
            0 => 100,
            other => u32::from(other),
        };
        let numerator = u32::from(frame.delay_num) * 1000;
        assert!(
            numerator % denominator == 0,
            "{}: retardo {}/{} s no es un número entero de ms",
            path.display(),
            frame.delay_num,
            frame.delay_den
        );
        durations_ms.push(numerator / denominator);
    }
    Animation {
        width,
        height,
        durations_ms,
        size_bytes: bytes.len() as u64,
    }
}

pub struct Conversion {
    pub report: Report,
    pub path: PathBuf,
    _directory: tempfile::TempDir,
}

pub fn convert_video(
    video: &TestVideo,
    params: &ConversionParams,
) -> Result<Conversion, ConversionError> {
    convert_video_with(video, &ConversionOptions::new(params)?)
}

pub fn convert_video_with(
    video: &TestVideo,
    options: &ConversionOptions,
) -> Result<Conversion, ConversionError> {
    let directory = tempfile::tempdir().expect("directorio temporal");
    let path = directory
        .path()
        .join(format!("salida.{}", options.preset().format.extension()));
    let report = convert(&FfmpegDecoder::new(video.path()), options, Tracking::none())?;
    fs::write(&path, &report.bytes).expect("se puede escribir la salida");
    Ok(Conversion {
        report,
        path,
        _directory: directory,
    })
}

pub fn check_rules(
    preset: &Preset,
    max_size_bytes: u64,
    expected_s: f64,
    report: &Report,
    path: &Path,
) -> Animation {
    let animation = check_duration_except_rules(preset, max_size_bytes, report, path);
    check_requested_duration(preset, expected_s, &animation);
    animation
}

pub fn check_duration_except_rules(
    preset: &Preset,
    max_size_bytes: u64,
    report: &Report,
    path: &Path,
) -> Animation {
    let context = format!("{preset} ({})", path.display());
    let header = fs::read(path).expect("el archivo de salida existe");
    let (animation, correct_format) = match preset.format {
        Format::AnimatedWebp => (
            read_webp(path),
            header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP".as_slice()),
        ),
        Format::Gif => (read_gif(path), header.starts_with(b"GIF89a")),
        Format::Apng => (read_apng(path), header.starts_with(b"\x89PNG\r\n\x1a\n")),
    };
    assert!(correct_format, "{context}: la cabecera no es del formato");
    if preset.format == Format::AnimatedWebp {
        assert!(
            header.get(12..16) == Some(b"VP8X".as_slice())
                && header.get(20).is_some_and(|flags| flags & 0x02 != 0),
            "{context}: no es un WebP animado"
        );
    }
    match preset.canvas {
        Canvas::Exact(side) => assert_eq!(
            (animation.width, animation.height),
            (side, side),
            "{context}: dimensiones"
        ),
        Canvas::Max(side) => assert!(
            animation.width == animation.height && animation.width <= side && animation.width > 0,
            "{context}: {}x{} no es cuadrado de lado <= {side}",
            animation.width,
            animation.height
        ),
    }
    assert!(
        animation.size_bytes <= max_size_bytes,
        "{context}: pesa {} B y el límite es {max_size_bytes} B",
        animation.size_bytes
    );
    assert!(animation.frame_count() >= 1, "{context}: sin frames");
    let total_ms = animation.total_duration_ms();
    assert!(total_ms > 0, "{context}: la animación dura 0 ms");
    let cap_ms = preset.max_duration_s * 1000.0;
    assert!(
        total_ms as f64 <= cap_ms,
        "{context}: dura {total_ms} ms y el tope es {cap_ms} ms"
    );
    assert_eq!(animation.size_bytes, report.size_bytes(), "{context}");
    assert_eq!(animation.width, report.canvas_side, "{context}");
    animation
}

pub fn check_requested_duration(preset: &Preset, expected_s: f64, animation: &Animation) {
    let total_ms = animation.total_duration_ms();
    let tolerance_ms = if preset.format == Format::Gif {
        10.0
    } else {
        0.0
    };
    let expected_ms = expected_s * 1000.0;
    assert!(
        total_ms as f64 <= expected_ms + tolerance_ms,
        "{preset}: dura {total_ms} ms y se pidieron {expected_ms} ms"
    );
    assert!(
        total_ms as f64 >= expected_ms - 200.0 - tolerance_ms,
        "{preset}: dura {total_ms} ms y se pidieron {expected_ms} ms"
    );
    if let Some(min_ms) = preset.frame_min_duration_ms {
        assert!(
            animation.min_duration_ms().is_some_and(|ms| ms >= min_ms),
            "{preset}: hay un frame de menos de {min_ms} ms: {:?}",
            animation.durations_ms
        );
    }
}
