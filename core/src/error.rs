use std::io;
use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

use crate::decoding::MAX_VIDEO_SIDE_PX;
use crate::options::Speed;
use crate::preset::WHATSAPP_STICKER;
use crate::size_fitting::Attempt;
use crate::texts::{decimal, decimal_1, readable_size};

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum OptionsError {
    #[error(
        "la duración máxima (--max-seconds) de {preset} debe ser mayor que 0 y como mucho {} s; se pidió {}",
        decimal(*cap_s),
        decimal(*value)
    )]
    MaxSecondsOutsideOfRange {
        preset: &'static str,
        value: f64,
        cap_s: f64,
    },

    #[error(
        "el inicio (--start) debe ser un número de segundos mayor o igual que 0; se pidió {}",
        decimal(*value)
    )]
    StartNotValid { value: f64 },

    #[error("los fps (--fps) deben estar entre 5 y 30; se pidió {value}")]
    FpsOutsideOfRange { value: u32 },

    #[error("Esos fps no se admiten en este destino")]
    FpsNotSupported { preset: &'static str, value: u32 },

    #[error("el peso máximo (--max-mb) no se puede cambiar en {preset}")]
    MaxMbNotSupported { preset: &'static str },

    #[error(
        "el peso máximo (--max-mb) de {preset} debe estar entre {} y {} MB; se pidió {}",
        decimal(*min),
        decimal(*max),
        decimal(*value)
    )]
    MaxMbOutsideOfRange {
        preset: &'static str,
        value: f64,
        min: f64,
        max: f64,
    },

    #[error(
        "el inicio (--start {}) no es anterior al final del video ({} s)",
        decimal(*start_s),
        decimal(*video_duration_s)
    )]
    StartOutsideOfVideo { start_s: f64, video_duration_s: f64 },

    #[error(
        "el tramo dura {} s; acelerando 2x caben {} s en {} s. Recorta al menos {} s (por ejemplo, --start {}).",
        decimal_1(*available_s),
        decimal_1(*max_s),
        decimal_1(*max_s / 2.0),
        decimal_1(*crop_s),
        decimal(*suggested_start_s)
    )]
    DoesNotFitEvenSpedUp {
        available_s: f64,
        max_s: f64,
        crop_s: f64,
        suggested_start_s: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FramingError {
    #[error("El encuadre no es válido")]
    NotValid,

    #[error("El tamaño del contenido tiene que estar entre 0,5 y 1")]
    ScaleNotValid,
}

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum SpeedError {
    #[error(
        "la velocidad debe estar entre {}x y {}x; se pidió {}x",
        decimal(Speed::MIN),
        decimal(Speed::MAX),
        decimal(*value)
    )]
    OutsideRange { value: f64 },

    #[error("no se puede fijar la velocidad con el ajuste automático (--fit), que la calcula")]
    WithAutomaticFitting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ClipError {
    #[error("el clip no tiene frames")]
    WithoutFrames,

    #[error("el lado del clip ({side} px) no es válido")]
    SideNotValid { side: u32 },

    #[error("el frame {index} tiene {real} bytes RGBA y deberían ser {expected}")]
    IncorrectFrameSize {
        index: usize,
        expected: usize,
        real: usize,
    },

    #[error("el frame {index} dura 0 ms")]
    FrameWithoutDuration { index: usize },
}

#[derive(Debug, Error)]
pub enum DecodingError {
    #[error("no existe el video {}", path.display())]
    FileNotFound { path: PathBuf },

    #[error("{} no es un archivo normal; indica la ruta de un archivo de video", path.display())]
    NotRegularFile { path: PathBuf },

    #[error("no se pudo leer {}: {source}", path.display())]
    UnreadableFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error(
        "el video pesa {} MB y el máximo admitido es {} MB; recórtalo antes de convertirlo",
        bytes / 1_000_000,
        max_bytes / 1_000_000
    )]
    FileTooLarge { bytes: u64, max_bytes: u64 },

    #[error(
        "no se encontró `{program}`; instala FFmpeg (por ejemplo `sudo apt-get install ffmpeg` \
         o `sudo pacman -S ffmpeg`)"
    )]
    FfmpegNotFound { program: String },

    #[error("no se pudo ejecutar `{program}`: {source}")]
    ProcessNotLaunched {
        program: String,
        #[source]
        source: io::Error,
    },

    #[error("no se pudo crear un hilo para leer la salida de ffmpeg: {source}")]
    ThreadNotLaunched {
        #[source]
        source: io::Error,
    },

    #[error(
        "`{program}` no terminó en {} s y se detuvo; prueba con un tramo más corto",
        limit.as_secs_f64()
    )]
    TimedOut { program: String, limit: Duration },

    #[error("`{program}` falló ({}): {summarized_stderr}", describe_code(*code))]
    FfmpegFailed {
        program: String,
        code: Option<i32>,
        summarized_stderr: String,
    },

    #[error("no se pudo leer la salida de `{program}`: {source}")]
    UnreadableOutput {
        program: String,
        #[source]
        source: io::Error,
    },

    #[error("el archivo no tiene ninguna pista de video")]
    WithoutVideoTrack,

    #[error("Este video no tiene audio")]
    WithoutAudioTrack,

    #[error("Este video tiene el audio en {codec} y aún no se puede guardar")]
    AudioNotSupported { codec: String },

    #[error("Este video está en {codec} y no se puede guardar para un sticker con sonido")]
    VideoCodecNotSupported { codec: String },

    #[error("ffprobe no dio una duración legible ({text:?}); el archivo puede estar dañado")]
    UnreadableDuration { text: String },

    #[error("el video dura {} s; tiene que durar más de 0 s", decimal(*duration_s))]
    DurationNotValid { duration_s: f64 },

    #[error(
        "las dimensiones del video ({width}x{height}) no son válidas; se admiten de 1 a {max} píxeles por lado"
    )]
    DimensionsNotValid { width: u32, height: u32, max: u32 },

    #[error("el tramo elegido no tiene ningún frame; elige otro --start o una duración mayor")]
    VideoWithoutFrames,

    #[error("se canceló la decodificación")]
    Cancelled,

    #[error(transparent)]
    Clip(#[from] ClipError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ImageError {
    #[error("la imagen tiene {received} bytes RGBA y deberían ser {expected}")]
    IncorrectSize { expected: usize, received: usize },

    #[error(
        "las dimensiones de la imagen ({width}x{height}) no son válidas; se admiten de 1 a \
         {MAX_VIDEO_SIDE_PX} píxeles por lado"
    )]
    SideNotValid { width: u32, height: u32 },
}

#[derive(Debug, Error)]
pub enum StickerError {
    #[error("el sticker no es un WebP")]
    NotWebp,

    #[error("el sticker pesa {} y como mucho puede pesar {}", readable_size(*bytes), readable_size(*max))]
    TooHeavy { bytes: u64, max: u64 },

    #[error("el sticker mide {width}x{height} y tiene que medir {side}x{side}", side = WHATSAPP_STICKER.canvas.side())]
    IncorrectSide { width: u32, height: u32 },

    #[error(
        "el sticker dura {} s y como mucho puede durar {} s",
        decimal(*duration_ms as f64 / 1000.0),
        decimal(WHATSAPP_STICKER.max_duration_s)
    )]
    TooLong { duration_ms: u64 },

    #[error("el sticker tiene un frame de {duration_ms} ms, más corto de lo que admite WhatsApp")]
    FrameTooShort { duration_ms: u32 },

    #[error(
        "el sticker es un WebP animado con un solo frame; expórtalo como imagen estática \
         o con más de un frame"
    )]
    AnimatedSingleFrame,

    #[error("no se pudo leer el sticker: {reason}")]
    Unreadable { reason: String },

    #[error("el icono no es válido: {reason}")]
    IconNotValid { reason: &'static str },

    #[error("no se pudo crear el icono: {0}")]
    Encoding(#[source] EncodingError),
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("el enlace no es válido: {reason}")]
    LinkNotValid { reason: &'static str },

    #[error(
        "no se encontró `{program}`; instala yt-dlp (por ejemplo `sudo pacman -S yt-dlp`, \
         `sudo apt-get install yt-dlp` o `pipx install yt-dlp`)"
    )]
    YtDlpNotFound { program: String },

    #[error("no se pudo ejecutar `{program}`: {source}")]
    ProcessNotLaunched {
        program: String,
        #[source]
        source: io::Error,
    },

    #[error("no se pudo crear un hilo para leer la salida de yt-dlp: {source}")]
    ThreadNotLaunched {
        #[source]
        source: io::Error,
    },

    #[error("no se pudo crear el directorio temporal de la descarga: {0}")]
    TempNotCreated(#[source] io::Error),

    #[error("la descarga no terminó en {} s y se detuvo", limit.as_secs())]
    TimedOut { limit: Duration },

    #[error(
        "el video pasa de {} MB, el máximo admitido, y se detuvo la descarga",
        max_bytes / 1_000_000
    )]
    FileTooLarge { max_bytes: u64 },

    #[error(
        "yt-dlp no pudo descargar el video ({}): {summarized_stderr}. Si el sitio pide \
         iniciar sesión, descárgalo a mano y pasa el archivo",
        describe_code(*code)
    )]
    FailedDownload {
        code: Option<i32>,
        summarized_stderr: String,
    },

    #[error("no se encontró el video descargado: {reason}")]
    FileNotLocated { reason: &'static str },

    #[error("{reason}")]
    FailedOwnDownload { reason: String },

    #[error("se canceló la descarga")]
    Cancelled,
}

#[derive(Debug, Error)]
pub enum EncodingError {
    #[error("la calidad WebP debe estar entre 0 y 100; se pidió {quality}")]
    QualityOutsideOfRange { quality: u8 },

    #[error(
        "el frame {index} dura {duration_ms} ms y el destino exige al menos {min_ms} ms por frame"
    )]
    FrameTooShort {
        index: usize,
        duration_ms: u32,
        min_ms: u32,
    },

    #[error("la animación dura {duration_ms} ms, más de lo que admite el formato")]
    AnimationTooLong { duration_ms: u64 },

    #[error("libwebp devolvió un WebP fijo que no se pudo convertir en animado")]
    ContainerWebPNotValid,

    #[error("libwebp no pudo codificar la animación: {0}")]
    WebP(webp_animation::Error),

    #[error("el número de colores debe estar entre 2 y 256; se pidió {colors}")]
    ColorsOutsideOfRange { colors: u16 },

    #[error("un GIF mide como mucho 65 535 px de lado; el clip mide {side}")]
    SideGifNotValid { side: u32 },

    #[error("no se pudo escribir el GIF: {0}")]
    Gif(gif::EncodingError),

    #[error("no se pudo escribir el APNG: {0}")]
    Apng(png::EncodingError),

    #[error("no se pudo escribir el PNG: {0}")]
    Png(png::EncodingError),

    #[error("codificación cancelada")]
    Cancelled,
}

#[derive(Debug, Error)]
pub enum ScalingError {
    #[error("el frame no es una imagen RGBA cuadrada válida: {0}")]
    Image(fast_image_resize::ImageBufferError),

    #[error("no se pudo escalar el frame: {0}")]
    Resized(fast_image_resize::ResizeError),

    #[error(transparent)]
    Clip(#[from] ClipError),
}

#[derive(Debug, Error)]
pub enum SizeFittingError {
    #[error("no cabe en {limit_bytes} bytes; lo más ligero pesó {best_bytes}")]
    DoesNotFitInSize {
        limit_bytes: u64,
        best_bytes: u64,
        best_attempt: Attempt,
        suggested_seconds: Option<f64>,
    },

    #[error("la escalera del formato no tiene ninguna configuración que probar")]
    EmptyLadder,

    #[error("se canceló el ajuste de peso")]
    Cancelled,

    #[error(transparent)]
    Scaling(#[from] ScalingError),

    #[error(transparent)]
    Encoding(#[from] EncodingError),
}

#[derive(Debug, Error)]
pub enum PreviewError {
    #[error(transparent)]
    Scaling(#[from] ScalingError),

    #[error(transparent)]
    Encoding(#[from] EncodingError),
}

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error(transparent)]
    Options(#[from] OptionsError),

    #[error(transparent)]
    Decoding(DecodingError),

    #[error(
        "no cabe en {}: el intento más ligero pesó {} ({}). {}",
        readable_size(*limit_bytes),
        readable_size(*best_bytes),
        readable_attempt(best_attempt, *fixed_image),
        if *fixed_image {
            "Prueba con una imagen con menos detalle.".to_string()
        } else {
            suggestion(*suggested_seconds, *remove_fit, *speed)
        }
    )]
    DoesNotFitInSize {
        preset: &'static str,
        limit_bytes: u64,
        best_bytes: u64,
        best_attempt: Attempt,
        suggested_seconds: Option<f64>,
        remove_fit: bool,
        speed: Option<Speed>,
        fixed_image: bool,
    },

    #[error("no se pudo generar la vista previa: {0}")]
    Preview(#[from] PreviewError),

    #[error(transparent)]
    SizeFitting(SizeFittingError),

    #[error("no se pudo guardar la vista previa: {0}")]
    PreviewNotSaved(#[source] io::Error),

    #[error("conversión cancelada")]
    Cancelled,
}

impl From<DecodingError> for ConversionError {
    fn from(error: DecodingError) -> Self {
        match error {
            DecodingError::Cancelled => ConversionError::Cancelled,
            other => ConversionError::Decoding(other),
        }
    }
}

impl ConversionError {
    pub(crate) fn from_fitting(
        preset: &'static str,
        automatic_fitting: bool,
        speed: Option<Speed>,
        error: SizeFittingError,
    ) -> Self {
        match error {
            SizeFittingError::DoesNotFitInSize {
                limit_bytes,
                best_bytes,
                best_attempt,
                suggested_seconds,
            } => ConversionError::DoesNotFitInSize {
                preset,
                limit_bytes,
                best_bytes,
                best_attempt,
                suggested_seconds,
                remove_fit: automatic_fitting,
                speed,
                fixed_image: false,
            },
            SizeFittingError::Cancelled | SizeFittingError::Encoding(EncodingError::Cancelled) => {
                ConversionError::Cancelled
            }
            other => ConversionError::SizeFitting(other),
        }
    }

    pub(crate) fn from_image_fitting(preset: &'static str, error: SizeFittingError) -> Self {
        let mut error = Self::from_fitting(preset, false, None, error);
        if let ConversionError::DoesNotFitInSize { fixed_image, .. } = &mut error {
            *fixed_image = true;
        }
        error
    }
}

fn readable_attempt(attempt: &Attempt, fixed_image: bool) -> String {
    if fixed_image {
        format!("{} {}", attempt.knob, attempt.value)
    } else {
        attempt.to_string()
    }
}

fn suggestion(seconds: Option<f64>, remove_fit: bool, speed: Option<Speed>) -> String {
    match (seconds, remove_fit) {
        (Some(seconds), false) => format!(
            "Prueba con --max-seconds {}{} o elige otro tramo con --start.",
            decimal(seconds),
            speed.map_or_else(String::new, |v| format!(" a {}x", decimal(v.value())))
        ),
        (Some(seconds), true) => format!(
            "Prueba con --max-seconds {} sin --fit o elige otro tramo con --start.",
            decimal(seconds)
        ),
        (None, _) => {
            "No se encontró una duración que quepa; elige otro tramo con --start.".to_string()
        }
    }
}

fn describe_code(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("código {code}"),
        None => "terminado por una señal".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestion_says_remove_fit_or_not_returns_digit() {
        assert_eq!(
            suggestion(Some(4.5), false, None),
            "Prueba con --max-seconds 4,5 o elige otro tramo con --start."
        );
        assert_eq!(
            suggestion(Some(4.5), true, None),
            "Prueba con --max-seconds 4,5 sin --fit o elige otro tramo con --start."
        );
        for remove_fit in [false, true] {
            assert_eq!(
                suggestion(None, remove_fit, None),
                "No se encontró una duración que quepa; elige otro tramo con --start."
            );
        }
    }

    #[test]
    fn suggestion_with_speed_fixed_says_to_that_speed_fits() {
        let speed = Speed::new(1.5).expect("velocidad válida");
        assert_eq!(
            suggestion(Some(4.5), false, Some(speed)),
            "Prueba con --max-seconds 4,5 a 1,5x o elige otro tramo con --start."
        );
    }
}
