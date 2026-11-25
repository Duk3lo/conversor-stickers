use std::fmt::Write as _;
use std::path::PathBuf;

use clap::builder::{PossibleValue, PossibleValuesParser, StyledStr, TypedValueParser};
use clap::error::{ContextKind, ErrorFormatter, ErrorKind};
use clap::{ArgAction, Parser};
use conversor_core::{Fps, PRESETS, Preset, decimal, find_preset};

use crate::codes::CODES;

const USE: &str =
    "conversor-stickers <ENTRADA>... --preset <PRESET> [-o SALIDA | -d DIR] [OPCIONES]";

const HELP_TEMPLATE: &str = "\
{about-with-newline}
Uso: {usage}

{all-args}{after-help}";

const EXAMPLES: &str = "\
Ejemplos:
  # Sticker de WhatsApp con los 6 primeros segundos (gato.webp junto al video)
  conversor-stickers gato.mp4 --preset whatsapp-sticker

  # Desde el segundo 12,5 y como mucho 4 s, a 12 fps
  conversor-stickers gato.mp4 --preset whatsapp-sticker --start 12.5 --max-seconds 4 --fps 12

  # Acelera hasta 2x para que un video de 18 s entre en los 10 s de WhatsApp
  conversor-stickers gato.mp4 --preset whatsapp-sticker --fit --preview

  # GIF para Discord de hasta 20 MB, desde el minuto 1:05
  conversor-stickers gato.mp4 --preset discord-gif --max-mb 20 --start 1:05 -o gato.gif

  # Emoji animado de Discord (GIF 128x128, hasta 256 KB)
  conversor-stickers gato.mp4 --preset discord-emoji --start 3

  # Sticker de Discord (APNG 320x320, hasta 5 s; sale gato.png)
  conversor-stickers gato.mp4 --preset discord-sticker --fit

  # Solo muestra qué tramo, velocidad y fps se usarían, sin escribir nada
  conversor-stickers gato.mp4 --preset discord-gif --fit --dry-run

  # Descarga un video con yt-dlp y lo guarda con su título (<título>.webp)
  conversor-stickers 'https://www.tiktok.com/@usuario/video/123' --preset whatsapp-sticker

  # Varios enlaces y archivos de una vez, con las salidas en ~/stickers
  conversor-stickers 'https://youtu.be/xxxx' gato.mp4 perro.mov --preset discord-gif -d ~/stickers";

#[derive(Debug, Parser)]
#[command(
    name = "conversor-stickers",
    version,
    about = "Convierte videos locales o enlaces en stickers animados de WhatsApp o GIF para \
             Discord.",
    long_about = "Convierte videos locales o enlaces en stickers animados de WhatsApp o GIF \
                  para Discord, con un recorte cuadrado centrado y sin deformar. Si el archivo \
                  supera el peso del destino, baja calidad, fps, detalle, resolución y \
                  colores hasta que quepa; si no cabe, falla con un mensaje que dice qué \
                  probar. KB y MB son de base 10 (500 KB = 500 000 bytes).",
    override_usage = USE,
    help_template = HELP_TEMPLATE,
    after_help = help_final(),
    disable_help_flag = true,
    disable_version_flag = true,
    next_help_heading = "Opciones"
)]
pub(crate) struct Arguments {
    #[arg(
        value_name = "ENTRADA",
        required = true,
        num_args = 1..,
        help_heading = "Argumentos",
        help = "Uno o varios videos: rutas de archivo (cualquier formato que lea FFmpeg) o enlaces \
                http/https, que se descargan con yt-dlp. Se convierten en orden; si uno falla, se \
                sigue con el siguiente. Un archivo cuyo nombre empieza por algo:// se pasa como \
                ./algo://…"
    )]
    pub(crate) inputs: Vec<PathBuf>,

    #[arg(
        long,
        value_name = "PRESET",
        value_parser = parser_preset(),
        hide_possible_values = true,
        help = preset_help()
    )]
    pub(crate) preset: &'static Preset,

    #[arg(
        short = 'o',
        long = "output",
        value_name = "SALIDA",
        help = "Archivo de salida, solo con una entrada. Por defecto, el video con la extensión \
                del preset (gato.mp4 → gato.webp); con un enlace, el título del video en el \
                directorio actual, sin sobrescribir (_1, _2…)."
    )]
    pub(crate) output: Option<PathBuf>,

    #[arg(
        short = 'd',
        long = "dir",
        value_name = "DIR",
        conflicts_with = "output",
        help = "Directorio de todas las salidas, que tiene que existir. Por defecto, el del \
                video, o el actual con un enlace."
    )]
    pub(crate) directory: Option<PathBuf>,

    #[arg(
        long,
        value_name = "SEGUNDOS",
        value_parser = parse_decimal,
        help = "Duración máxima del resultado, en segundos. Por defecto 6 s, o el tope del \
                preset con --fit; nunca más que ese tope."
    )]
    pub(crate) max_seconds: Option<f64>,

    #[arg(
        long,
        value_name = "INICIO",
        value_parser = parse_start,
        help = "Inicio del tramo: segundos (12.5 o 12,5) o mm:ss (1:05.5). Por defecto 0."
    )]
    pub(crate) start: Option<f64>,

    #[arg(long, value_name = "FPS", value_parser = parse_whole, help = help_fps())]
    pub(crate) fps: Option<u32>,

    #[arg(
        long,
        help = "Si el tramo dura más que --max-seconds, lo acelera hasta 2x para que entre; si \
                ni así entra, dice cuánto recortar."
    )]
    pub(crate) fit: bool,

    #[arg(long, value_name = "MB", value_parser = parse_decimal, help = help_max_mb())]
    pub(crate) max_mb: Option<f64>,

    #[arg(
        long,
        help = "Antes de ajustar el peso, escribe <salida>.preview.webp (160x160) para ver el \
                efecto de acelerar o bajar fps."
    )]
    pub(crate) preview: bool,

    #[arg(
        long,
        help = "Muestra el tramo, la velocidad y los fps que se usarían, sin decodificar ni \
                escribir nada. Con un enlace, lo descarga a un temporal que se borra."
    )]
    pub(crate) dry_run: bool,

    #[arg(short = 'h', long = "help", action = ArgAction::HelpLong, help = "Muestra esta ayuda.")]
    help: Option<bool>,

    #[arg(
        short = 'V',
        long = "version",
        action = ArgAction::Version,
        help = "Muestra la versión."
    )]
    version: Option<bool>,
}

impl Arguments {
    pub(crate) fn read() -> Self {
        Self::try_parse().unwrap_or_else(|error| error.apply::<SpanishFormat>().exit())
    }
}

fn parser_preset() -> impl TypedValueParser<Value = &'static Preset> {
    PossibleValuesParser::new(PRESETS.iter().map(|preset| PossibleValue::new(preset.name))).try_map(
        |name: String| find_preset(&name).ok_or_else(|| format!("no hay ningún preset {name:?}")),
    )
}

fn preset_help() -> String {
    let mut help = String::from("Destino del archivo (obligatorio):");
    for preset in PRESETS {
        let _ = write!(help, "\n  {}: {}", preset.name, preset.description);
    }
    help
}

fn help_fps() -> String {
    format!(
        "Fotogramas por segundo del resultado, de {} a {}. Por defecto {}; si no cabe en \
         el peso, se bajan",
        Fps::MIN,
        Fps::MAX,
        Fps::DEFAULT.value()
    )
}

fn help_max_mb() -> String {
    let mut help = String::from("Peso máximo en MB de base 10. Solo en");
    for preset in PRESETS {
        if let Some(range) = preset.range_max_mb {
            let _ = write!(
                help,
                " {} (de {} a {}, por defecto {})",
                preset.name,
                decimal(range.min),
                decimal(range.max),
                decimal(preset.max_size.bytes() as f64 / 1_000_000.0)
            );
        }
    }
    help
}

fn help_final() -> String {
    let mut help = format!("{EXAMPLES}\n\nCódigos de salida:");
    for (code, meaning) in CODES {
        let _ = write!(help, "\n  {code:>3}  {meaning}");
    }
    help.push_str(
        "\n\nRequiere ffmpeg y ffprobe en el PATH, y yt-dlp para los enlaces. Descarga solo \
         videos que tengas derecho a descargar.",
    );
    help
}

fn parse_decimal(text: &str) -> Result<f64, String> {
    text.trim()
        .replace(',', ".")
        .parse()
        .map_err(|_| format!("{text:?} no es un número (por ejemplo 4.5 o 4,5)"))
}

fn parse_whole(text: &str) -> Result<u32, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("{text:?} no es un número entero positivo"))
}

fn parse_start(text: &str) -> Result<f64, String> {
    let not_valid =
        || format!("{text:?} no es un inicio válido: usa segundos (12.5) o mm:ss (1:05.5)");
    match text.split_once(':') {
        None => parse_decimal(text).map_err(|_| not_valid()),
        Some((minutes, seconds)) => {
            let minutes: u32 = minutes.trim().parse().map_err(|_| not_valid())?;
            let seconds = parse_decimal(seconds).map_err(|_| not_valid())?;
            if !(0.0..60.0).contains(&seconds) {
                return Err(not_valid());
            }
            Ok(f64::from(minutes) * 60.0 + seconds)
        }
    }
}

pub(crate) struct SpanishFormat;

impl ErrorFormatter for SpanishFormat {
    fn format_error(error: &clap::error::Error<Self>) -> StyledStr {
        let context = |key| error.get(key).map(ToString::to_string).unwrap_or_default();
        let argument = context(ContextKind::InvalidArg);
        let value = context(ContextKind::InvalidValue);
        let cause = std::error::Error::source(error)
            .map(ToString::to_string)
            .unwrap_or_default();
        let message = match error.kind() {
            ErrorKind::InvalidValue => {
                let valid = context(ContextKind::ValidValue);
                if value.is_empty() {
                    format!("falta el valor de {argument}")
                } else if valid.is_empty() {
                    format!("valor no válido {value:?} para {argument}")
                } else {
                    format!("valor no válido {value:?} para {argument}; se admite: {valid}")
                }
            }
            ErrorKind::ValueValidation => {
                format!("valor no válido {value:?} para {argument}: {cause}")
            }
            ErrorKind::UnknownArgument => format!("argumento desconocido: {argument}"),
            ErrorKind::MissingRequiredArgument => {
                format!("falta un argumento obligatorio: {argument}")
            }
            ErrorKind::ArgumentConflict => {
                format!(
                    "{argument} no se puede usar con {}",
                    context(ContextKind::PriorArg)
                )
            }
            ErrorKind::TooManyValues | ErrorKind::WrongNumberOfValues => {
                format!("número de valores incorrecto para {argument}")
            }
            ErrorKind::NoEquals => format!("{argument} necesita un valor con ="),
            _ if !cause.is_empty() => format!("argumentos no válidos: {cause}"),
            _ => "argumentos no válidos".to_string(),
        };
        let mut text = StyledStr::new();
        let _ = write!(
            text,
            "error: {message}\n\nUso: {USE}\n\nPara más información, prueba con '--help'.\n"
        );
        text
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn definition_of_clap_is_valid() {
        Arguments::command().debug_assert();
    }

    #[test]
    fn decimals_support_point_and_comma() {
        assert_eq!(parse_decimal("4.5"), Ok(4.5));
        assert_eq!(parse_decimal("4,5"), Ok(4.5));
        assert_eq!(parse_decimal("10"), Ok(10.0));
        assert!(parse_decimal("cuatro").is_err());
        assert_eq!(parse_whole("12"), Ok(12));
        assert!(parse_whole("12.5").is_err());
        assert!(parse_whole("-1").is_err());
    }

    #[test]
    fn start_supports_seconds_and_minutes_with_seconds() {
        assert_eq!(parse_start("12.5"), Ok(12.5));
        assert_eq!(parse_start("12,5"), Ok(12.5));
        assert_eq!(parse_start("1:05"), Ok(65.0));
        assert_eq!(parse_start("01:05.5"), Ok(65.5));
        assert_eq!(parse_start("0:00"), Ok(0.0));
        for text in ["1:60", "1:-1", "a:05", "1:", ":5", "1:2:3", "doce"] {
            assert!(parse_start(text).is_err(), "{text}");
        }
    }

    #[test]
    fn preset_searches_in_table() {
        for preset in PRESETS {
            let arguments =
                Arguments::try_parse_from(["conversor-stickers", "v.mp4", "--preset", preset.name])
                    .unwrap();
            assert_eq!(arguments.preset, preset);
        }
        let error =
            Arguments::try_parse_from(["conversor-stickers", "v.mp4", "--preset", "telegram"])
                .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn help_list_each_preset_with_its_description() {
        let help = preset_help();
        for preset in PRESETS {
            assert!(help.contains(preset.name));
            assert!(help.contains(preset.description));
        }
    }
}
