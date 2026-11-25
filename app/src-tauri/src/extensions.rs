//! Extension manifests: declarative JSON the user installs from a file. They never hold code; the
//! app only reads these fields. Everything is checked here, at the border, before the interface
//! stores it.

use serde::{Deserialize, Serialize};

use crate::contract::{UiError, UiErrorKind};

/// Larger than any real manifest; keeps a wrong file from being read whole.
pub(crate) const MAX_MANIFEST_BYTES: u64 = 16 * 1024;

const KNOWN_VERSION: u32 = 1;
const ID_LENGTH: std::ops::RangeInclusive<usize> = 3..=40;
const NAME_MAX_CHARS: usize = 60;
const DESCRIPTION_MAX_CHARS: usize = 300;
const NOTICE_MAX_CHARS: usize = 600;
const URL_MAX_CHARS: usize = 200;
const SOUND_SECONDS: std::ops::RangeInclusive<u32> = 1..=600;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    id: String,
    nombre: String,
    descripcion: String,
    aviso: String,
    url: String,
    destino: Destination,
    pie: Caption,
    limites: Option<Limits>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Destination {
    tipo: DestinationKind,
}

#[derive(Debug, Deserialize)]
enum DestinationKind {
    #[serde(rename = "whatsapp-chat")]
    WhatsappChat,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Caption {
    sonido: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    #[serde(rename = "sonidoMaxSegundos")]
    sonido_max_segundos: Option<u32>,
}

/// What the interface keeps of an installed extension. Its destination is always a WhatsApp chat,
/// whose number the user types when turning it on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct ExtensionUi {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) notice: String,
    pub(crate) url: String,
    /// Whether the caption carries `sonido=<seconds>`.
    pub(crate) sound: bool,
    pub(crate) max_sound_s: Option<u32>,
}

fn invalid(message: impl Into<String>) -> UiError {
    UiError::new(
        UiErrorKind::InvalidEntry,
        format!("extensión no válida: {}", message.into()),
    )
}

fn check_text(field: &str, text: &str, max_chars: usize) -> Result<(), UiError> {
    if text.trim().is_empty() {
        return Err(invalid(format!("«{field}» está vacío")));
    }
    if text.chars().count() > max_chars {
        return Err(invalid(format!(
            "«{field}» supera los {max_chars} caracteres"
        )));
    }
    if text.chars().any(char::is_control) {
        return Err(invalid(format!("«{field}» tiene caracteres de control")));
    }
    Ok(())
}

fn check_url(url: &str) -> Result<(), UiError> {
    check_text("url", url, URL_MAX_CHARS)?;
    let host = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split(['/', '?', '#']).next());
    if host.is_none_or(str::is_empty) || url.chars().any(char::is_whitespace) {
        return Err(invalid("«url» debe ser una dirección https"));
    }
    Ok(())
}

pub(crate) fn parse_manifest(bytes: &[u8]) -> Result<ExtensionUi, UiError> {
    let manifest: Manifest = serde_json::from_slice(bytes)
        .map_err(|error| invalid(format!("el archivo no tiene el formato esperado ({error})")))?;
    if manifest.version != KNOWN_VERSION {
        return Err(invalid(format!(
            "versión {} desconocida; esta app entiende la {KNOWN_VERSION}",
            manifest.version
        )));
    }
    let id_valid = ID_LENGTH.contains(&manifest.id.len())
        && manifest
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !id_valid {
        return Err(invalid(
            "«id» debe tener de 3 a 40 letras minúsculas, dígitos o guiones",
        ));
    }
    check_text("nombre", &manifest.nombre, NAME_MAX_CHARS)?;
    check_text("descripcion", &manifest.descripcion, DESCRIPTION_MAX_CHARS)?;
    check_text("aviso", &manifest.aviso, NOTICE_MAX_CHARS)?;
    check_url(&manifest.url)?;
    let DestinationKind::WhatsappChat = manifest.destino.tipo;
    let max_sound_s = manifest
        .limites
        .and_then(|limits| limits.sonido_max_segundos);
    if max_sound_s.is_some_and(|seconds| !SOUND_SECONDS.contains(&seconds)) {
        return Err(invalid("«sonidoMaxSegundos» debe estar entre 1 y 600"));
    }
    Ok(ExtensionUi {
        id: manifest.id,
        name: manifest.nombre,
        description: manifest.descripcion,
        notice: manifest.aviso,
        url: manifest.url,
        sound: manifest.pie.sonido,
        max_sound_s,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn manifest() -> Value {
        json!({
            "version": 1,
            "id": "bot-stickers-sonido",
            "nombre": "Bot de stickers con sonido",
            "descripcion": "Envía el clip a un bot.",
            "aviso": "Cliente no oficial de WhatsApp.",
            "url": "https://github.com/Duk3lo/bot-stickers-sonido",
            "destino": { "tipo": "whatsapp-chat" },
            "pie": { "sonido": true },
            "limites": { "sonidoMaxSegundos": 10 }
        })
    }

    fn parse(value: &Value) -> Result<ExtensionUi, UiError> {
        parse_manifest(value.to_string().as_bytes())
    }

    fn with(field: &str, value: Value) -> Value {
        let mut changed = manifest();
        changed[field] = value;
        changed
    }

    #[test]
    fn valid_manifest_is_read() {
        let extension = parse(&manifest()).unwrap();
        assert_eq!(extension.id, "bot-stickers-sonido");
        assert_eq!(extension.name, "Bot de stickers con sonido");
        assert!(extension.sound);
        assert_eq!(extension.max_sound_s, Some(10));
    }

    #[test]
    fn bundled_manifest_is_valid() {
        let bytes = include_bytes!("../../src/builtin-extensions/bot-stickers-sonido.json");
        let extension = parse_manifest(bytes).unwrap();
        assert_eq!(extension.id, "bot-stickers-sonido");
        assert!(extension.sound);
    }

    #[test]
    fn limits_are_optional() {
        let mut without = manifest();
        without.as_object_mut().unwrap().remove("limites");
        assert_eq!(parse(&without).unwrap().max_sound_s, None);
        let empty = with("limites", json!({}));
        assert_eq!(parse(&empty).unwrap().max_sound_s, None);
    }

    #[test]
    fn unknown_version_is_rejected() {
        for version in [0, 2] {
            let error = parse(&with("version", json!(version))).unwrap_err();
            assert!(error.message.contains("versión"), "{}", error.message);
        }
    }

    #[test]
    fn id_is_lowercase_digits_and_hyphens() {
        for id in [
            "ab",
            "Bot-1",
            "bot_1",
            "bot 1",
            "ñandú",
            &"a".repeat(41),
            "",
        ] {
            assert!(parse(&with("id", json!(id))).is_err(), "{id}");
        }
        for id in ["abc", "bot-1", &"a".repeat(40)] {
            assert!(parse(&with("id", json!(id))).is_ok(), "{id}");
        }
    }

    #[test]
    fn texts_have_a_length_limit_and_no_control_characters() {
        for (field, max) in [("nombre", 60), ("descripcion", 300), ("aviso", 600)] {
            assert!(parse(&with(field, json!("a".repeat(max)))).is_ok());
            assert!(parse(&with(field, json!("a".repeat(max + 1)))).is_err());
            assert!(parse(&with(field, json!("  "))).is_err());
            assert!(parse(&with(field, json!("a\u{7}b"))).is_err());
        }
    }

    #[test]
    fn url_must_be_https() {
        for url in [
            "http://example.com",
            "ftp://example.com",
            "javascript:alert(1)",
            "https://",
            "https:///ruta",
            "https://exa mple.com",
            "example.com",
        ] {
            assert!(parse(&with("url", json!(url))).is_err(), "{url}");
        }
        assert!(parse(&with("url", json!("https://example.com/a?b=1"))).is_ok());
    }

    #[test]
    fn sound_limit_is_between_1_and_600() {
        for seconds in [0, 601] {
            let changed = with("limites", json!({ "sonidoMaxSegundos": seconds }));
            assert!(parse(&changed).is_err(), "{seconds}");
        }
        for seconds in [1, 600] {
            let changed = with("limites", json!({ "sonidoMaxSegundos": seconds }));
            assert!(parse(&changed).is_ok(), "{seconds}");
        }
    }

    #[test]
    fn unknown_fields_and_destinations_are_rejected() {
        assert!(parse(&with("script", json!("rm -rf /"))).is_err());
        assert!(parse(&with("pie", json!({ "sonido": true, "extra": 1 }))).is_err());
        assert!(parse(&with("limites", json!({ "otro": 1 }))).is_err());
        assert!(parse(&with("destino", json!({ "tipo": "shell" }))).is_err());
        assert!(
            parse(&with(
                "destino",
                json!({ "tipo": "whatsapp-chat", "numero": "1" })
            ))
            .is_err()
        );
    }

    #[test]
    fn missing_fields_and_garbage_are_rejected() {
        for field in [
            "id",
            "nombre",
            "descripcion",
            "aviso",
            "url",
            "destino",
            "pie",
        ] {
            let mut incomplete = manifest();
            incomplete.as_object_mut().unwrap().remove(field);
            assert!(parse(&incomplete).is_err(), "sin {field}");
        }
        assert!(parse_manifest(b"no es json").is_err());
        assert!(parse_manifest(b"").is_err());
    }
}
