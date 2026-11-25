use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::emojis::validate_emojis;
use crate::error::StickerError;
use crate::sticker_whatsapp::{
    StickerKind, TRAY_ICON_MAX_SIZE, inspect_tray_icon, inspect_whatsapp_sticker,
};

pub const INDEX_FORMAT: u32 = 1;
pub const MAX_INDEX_SIZE_BYTES: u64 = 5_000_000;
pub const MAX_PACKS: usize = 2000;
pub const MIN_STICKERS: usize = 3;
pub const MAX_STICKERS: usize = 30;
pub const MAX_TAGS: usize = 10;
pub const MAX_STICKER_SIZE_BYTES: u64 = 500_000;
pub const MAX_ICON_SIZE_BYTES: u64 = TRAY_ICON_MAX_SIZE;
pub const ALLOWED_LICENSES: [&str; 3] = ["CC0-1.0", "CC-BY-4.0", "CC-BY-SA-4.0"];
pub const ICON_FILE: &str = "icon.png";
pub const PACK_FILE: &str = "pack.json";

const ID_LONG: std::ops::RangeInclusive<usize> = 3..=64;
const TEXT_LONG: std::ops::RangeInclusive<usize> = 1..=128;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("el índice pesa {bytes} bytes y como mucho puede pesar {max}")]
    IndexTooLarge { bytes: u64, max: u64 },

    #[error("el índice no es un JSON válido: {reason}")]
    InvalidJson { reason: String },

    #[error("el índice es del formato {found} y esta versión solo entiende el {INDEX_FORMAT}")]
    FormatNotSupported { found: u64 },

    #[error("el catálogo tiene {count} paquetes y como mucho puede tener {MAX_PACKS}")]
    TooManyPacks { count: usize },

    #[error("el id {id:?} no es válido: minúsculas, números y guiones, de 3 a 64 caracteres")]
    InvalidId { id: String },

    #[error("el id {id:?} está repetido")]
    RepeatedId { id: String },

    #[error("{field}: tiene que tener de 1 a 128 caracteres, sin caracteres de control")]
    InvalidText { field: String },

    #[error("la fecha {date:?} de {field} no tiene la forma AAAA-MM-DD")]
    InvalidDate { field: String, date: String },

    #[error(
        "paquete {pack}: la licencia {license:?} no está permitida (CC0-1.0, CC-BY-4.0 o CC-BY-SA-4.0)"
    )]
    LicenseNotAllowed { pack: String, license: String },

    #[error("paquete {pack}: la categoría {category:?} no existe")]
    UnknownCategory { pack: String, category: String },

    #[error("paquete {pack}: tiene {count} etiquetas y como mucho puede tener {MAX_TAGS}")]
    TooManyTags { pack: String, count: usize },

    #[error("paquete {pack}: tiene {count} stickers y tiene que tener de 3 a 30")]
    StickersCount { pack: String, count: usize },

    #[error("paquete {pack}: el archivo {file:?} no es válido (NN.webp o icon.png)")]
    InvalidFile { pack: String, file: String },

    #[error("paquete {pack}: el archivo {file} está repetido")]
    RepeatedFile { pack: String, file: String },

    #[error("paquete {pack}: {file} tiene que tener de 1 a 3 emojis válidos")]
    InvalidEmojis { pack: String, file: String },

    #[error("paquete {pack}: {file} pesa {bytes} bytes y como mucho puede pesar {max}")]
    FileTooLarge {
        pack: String,
        file: String,
        bytes: u64,
        max: u64,
    },

    #[error("paquete {pack}: la suma SHA-256 de {file} no tiene 64 dígitos hexadecimales")]
    InvalidSum { pack: String, file: String },

    #[error("paquete {pack}: el sticker {file} no es válido: {source}")]
    InvalidSticker {
        pack: String,
        file: String,
        #[source]
        source: StickerError,
    },

    #[error("paquete {pack}: el icono no es válido: {source}")]
    InvalidIcon {
        pack: String,
        #[source]
        source: StickerError,
    },

    #[error("paquete {pack}: {file} es {real} y el paquete declara que es {declared}")]
    DifferentKind {
        pack: String,
        file: String,
        real: &'static str,
        declared: &'static str,
    },

    #[error("el paquete {id} está en la carpeta {folder:?}: tienen que llamarse igual")]
    IdDifferentFolder { id: String, folder: String },

    #[error("paquete {pack}: {file} sobra: no figura en pack.json")]
    SurplusFile { pack: String, file: String },

    #[error("paquete {pack}: falta el archivo {file}")]
    MissingFile { pack: String, file: String },

    #[error("el paquete {id} está en withdrawn.json y no puede seguir en el catálogo")]
    WithdrawnPack { id: String },

    #[error("no se pudo leer {}: {source}", path.display())]
    Reading {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{} no es un JSON válido: {reason}", path.display())]
    InvalidFileJson { path: PathBuf, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Withdrawn {
    pub id: String,
    pub date: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexFile {
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSticker {
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
    pub emojis: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexPack {
    pub id: String,
    pub version: u32,
    pub name: String,
    pub author: String,
    pub license: String,
    pub category: String,
    pub tags: Vec<String>,
    pub animated: bool,
    pub added: String,
    pub icon: IndexFile,
    pub stickers: Vec<IndexSticker>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub format: u32,
    pub generated: String,
    pub categories: Vec<Category>,
    pub packs: Vec<IndexPack>,
    pub withdrawn: Vec<Withdrawn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IconSource {
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StickerSource {
    pub file: String,
    pub emojis: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackSource {
    pub id: String,
    pub version: u32,
    pub name: String,
    pub author: String,
    pub license: String,
    pub category: String,
    pub tags: Vec<String>,
    pub animated: bool,
    pub added: String,
    pub icon: IconSource,
    pub stickers: Vec<StickerSource>,
}

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

pub fn read_index(bytes: &[u8]) -> Result<Index, CatalogError> {
    let size = bytes.len() as u64;
    if size > MAX_INDEX_SIZE_BYTES {
        return Err(CatalogError::IndexTooLarge {
            bytes: size,
            max: MAX_INDEX_SIZE_BYTES,
        });
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| CatalogError::InvalidJson {
            reason: error.to_string(),
        })?;
    let format = value
        .get("format")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| CatalogError::InvalidJson {
            reason: "falta el campo format".to_owned(),
        })?;
    if format != u64::from(INDEX_FORMAT) {
        return Err(CatalogError::FormatNotSupported { found: format });
    }
    let index: Index =
        serde_json::from_value(value).map_err(|error| CatalogError::InvalidJson {
            reason: error.to_string(),
        })?;
    validate_index(&index)?;
    Ok(index)
}

pub fn validate_index(index: &Index) -> Result<(), CatalogError> {
    if index.packs.len() > MAX_PACKS {
        return Err(CatalogError::TooManyPacks {
            count: index.packs.len(),
        });
    }
    validate_categories(&index.categories)?;
    validate_withdrawn(&index.withdrawn)?;
    let mut seen = HashSet::new();
    for pack in &index.packs {
        if !seen.insert(pack.id.as_str()) {
            return Err(CatalogError::RepeatedId {
                id: pack.id.clone(),
            });
        }
        let id = &pack.id;
        validate_header(
            id,
            &Header {
                name: &pack.name,
                author: &pack.author,
                license: &pack.license,
                category: &pack.category,
                tags: &pack.tags,
                added: &pack.added,
            },
            &index.categories,
        )?;
        check_index_file(
            id,
            &pack.icon.file,
            pack.icon.bytes,
            &pack.icon.sha256,
            MAX_ICON_SIZE_BYTES,
            true,
        )?;
        check_count(id, pack.stickers.len())?;
        let mut files = HashSet::new();
        for sticker in &pack.stickers {
            check_index_file(
                id,
                &sticker.file,
                sticker.bytes,
                &sticker.sha256,
                MAX_STICKER_SIZE_BYTES,
                false,
            )?;
            if !files.insert(sticker.file.as_str()) {
                return Err(CatalogError::RepeatedFile {
                    pack: id.clone(),
                    file: sticker.file.clone(),
                });
            }
            check_emojis(id, &sticker.file, &sticker.emojis)?;
        }
    }
    Ok(())
}

pub fn validate_source_pack(
    pack: &PackSource,
    categories: &[Category],
) -> Result<(), CatalogError> {
    let id = &pack.id;
    validate_header(
        id,
        &Header {
            name: &pack.name,
            author: &pack.author,
            license: &pack.license,
            category: &pack.category,
            tags: &pack.tags,
            added: &pack.added,
        },
        categories,
    )?;
    if pack.icon.file != ICON_FILE {
        return Err(CatalogError::InvalidFile {
            pack: id.clone(),
            file: pack.icon.file.clone(),
        });
    }
    check_count(id, pack.stickers.len())?;
    let mut files = HashSet::new();
    for sticker in &pack.stickers {
        if !is_sticker_file(&sticker.file) {
            return Err(CatalogError::InvalidFile {
                pack: id.clone(),
                file: sticker.file.clone(),
            });
        }
        if !files.insert(sticker.file.as_str()) {
            return Err(CatalogError::RepeatedFile {
                pack: id.clone(),
                file: sticker.file.clone(),
            });
        }
        check_emojis(id, &sticker.file, &sticker.emojis)?;
    }
    Ok(())
}

#[derive(Debug)]
pub struct Catalog {
    pub categories: Vec<Category>,
    pub withdrawn: Vec<Withdrawn>,
    pub packs: Vec<IndexPack>,
}

pub fn validate_catalog(directory: &Path) -> Result<Catalog, CatalogError> {
    let categories: Vec<Category> = read_json(&directory.join("categories.json"))?;
    let withdrawn: Vec<Withdrawn> = read_json(&directory.join("withdrawn.json"))?;
    validate_categories(&categories)?;
    validate_withdrawn(&withdrawn)?;

    let packs_root = directory.join("packs");
    let mut folders = Vec::new();
    for input in fs::read_dir(&packs_root).map_err(|source| reading(&packs_root, source))? {
        let input = input.map_err(|source| reading(&packs_root, source))?;
        let path = input.path();
        if path.is_dir() {
            folders.push(path);
        }
    }
    folders.sort();
    if folders.len() > MAX_PACKS {
        return Err(CatalogError::TooManyPacks {
            count: folders.len(),
        });
    }

    let mut packs = Vec::with_capacity(folders.len());
    for folder in &folders {
        let pack = validate_folder(folder, &categories)?;
        if withdrawn.iter().any(|withdrawn| withdrawn.id == pack.id) {
            return Err(CatalogError::WithdrawnPack { id: pack.id });
        }
        packs.push(pack);
    }
    packs.sort_by(|a, b| a.id.cmp(&b.id));
    for pair in packs.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(CatalogError::RepeatedId {
                id: pair[0].id.clone(),
            });
        }
    }
    Ok(Catalog {
        categories,
        withdrawn,
        packs,
    })
}

pub fn generate_index(directory: &Path) -> Result<Index, CatalogError> {
    let Catalog {
        categories,
        mut withdrawn,
        packs,
    } = validate_catalog(directory)?;
    withdrawn.sort_by(|a, b| a.id.cmp(&b.id));
    let last_date = packs
        .iter()
        .map(|pack| pack.added.as_str())
        .chain(withdrawn.iter().map(|withdrawn| withdrawn.date.as_str()))
        .max()
        .unwrap_or("1970-01-01");
    let index = Index {
        format: INDEX_FORMAT,
        generated: format!("{last_date}T00:00:00Z"),
        categories,
        packs,
        withdrawn,
    };
    validate_index(&index)?;
    Ok(index)
}

pub fn serialize_index(index: &Index) -> Result<String, CatalogError> {
    let mut text =
        serde_json::to_string_pretty(index).map_err(|error| CatalogError::InvalidJson {
            reason: error.to_string(),
        })?;
    text.push('\n');
    Ok(text)
}

fn validate_folder(folder: &Path, categories: &[Category]) -> Result<IndexPack, CatalogError> {
    let folder_name = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let source: PackSource = read_json(&folder.join(PACK_FILE))?;
    let id = source.id.clone();
    if !is_valid_id(&id) {
        return Err(CatalogError::InvalidId { id });
    }
    if folder_name != id {
        return Err(CatalogError::IdDifferentFolder {
            id,
            folder: folder_name,
        });
    }
    validate_source_pack(&source, categories)?;

    let declared = text_kind(source.animated);
    let mut stickers = Vec::with_capacity(source.stickers.len());
    for sticker in &source.stickers {
        let bytes = read_file(folder, &id, &sticker.file, MAX_STICKER_SIZE_BYTES)?;
        let info =
            inspect_whatsapp_sticker(&bytes).map_err(|source| CatalogError::InvalidSticker {
                pack: id.clone(),
                file: sticker.file.clone(),
                source,
            })?;
        let animated = info.kind == StickerKind::Animated;
        if animated != source.animated {
            return Err(CatalogError::DifferentKind {
                pack: id,
                file: sticker.file.clone(),
                real: text_kind(animated),
                declared,
            });
        }
        stickers.push(IndexSticker {
            file: sticker.file.clone(),
            bytes: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
            emojis: sticker.emojis.clone(),
        });
    }
    let icon = read_file(folder, &id, ICON_FILE, MAX_ICON_SIZE_BYTES)?;
    inspect_tray_icon(&icon).map_err(|source| CatalogError::InvalidIcon {
        pack: id.clone(),
        source,
    })?;

    check_without_surplus(folder, &id, &source)?;
    Ok(IndexPack {
        id,
        version: source.version,
        name: source.name,
        author: source.author,
        license: source.license,
        category: source.category,
        tags: source.tags,
        animated: source.animated,
        added: source.added,
        icon: IndexFile {
            file: ICON_FILE.to_owned(),
            bytes: icon.len() as u64,
            sha256: sha256_hex(&icon),
        },
        stickers,
    })
}

fn check_without_surplus(folder: &Path, id: &str, source: &PackSource) -> Result<(), CatalogError> {
    let mut surplus = None;
    for input in fs::read_dir(folder).map_err(|error| reading(folder, error))? {
        let name = input
            .map_err(|error| reading(folder, error))?
            .file_name()
            .to_string_lossy()
            .into_owned();
        let known = name == PACK_FILE
            || name == ICON_FILE
            || source.stickers.iter().any(|sticker| sticker.file == name);
        if !known && surplus.as_ref().is_none_or(|current| &name < current) {
            surplus = Some(name);
        }
    }
    match surplus {
        Some(file) => Err(CatalogError::SurplusFile {
            pack: id.to_owned(),
            file,
        }),
        None => Ok(()),
    }
}

fn read_file(folder: &Path, id: &str, file: &str, max: u64) -> Result<Vec<u8>, CatalogError> {
    let path = folder.join(file);
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(CatalogError::MissingFile {
                pack: id.to_owned(),
                file: file.to_owned(),
            });
        }
        Err(source) => return Err(reading(&path, source)),
    };
    if metadata.len() > max {
        return Err(CatalogError::FileTooLarge {
            pack: id.to_owned(),
            file: file.to_owned(),
            bytes: metadata.len(),
            max,
        });
    }
    fs::read(&path).map_err(|source| reading(&path, source))
}

fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T, CatalogError> {
    let bytes = fs::read(path).map_err(|source| reading(path, source))?;
    serde_json::from_slice(&bytes).map_err(|error| CatalogError::InvalidFileJson {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })
}

fn reading(path: &Path, source: io::Error) -> CatalogError {
    CatalogError::Reading {
        path: path.to_path_buf(),
        source,
    }
}

fn text_kind(animated: bool) -> &'static str {
    if animated { "animated" } else { "estático" }
}

struct Header<'a> {
    name: &'a str,
    author: &'a str,
    license: &'a str,
    category: &'a str,
    tags: &'a [String],
    added: &'a str,
}

fn validate_header(
    id: &str,
    header: &Header<'_>,
    categories: &[Category],
) -> Result<(), CatalogError> {
    if !is_valid_id(id) {
        return Err(CatalogError::InvalidId { id: id.to_owned() });
    }
    check_text(&format!("paquete {id}, nombre"), header.name)?;
    check_text(&format!("paquete {id}, autor"), header.author)?;
    if !ALLOWED_LICENSES.contains(&header.license) {
        return Err(CatalogError::LicenseNotAllowed {
            pack: id.to_owned(),
            license: header.license.to_owned(),
        });
    }
    if !categories.iter().any(|c| c.id == header.category) {
        return Err(CatalogError::UnknownCategory {
            pack: id.to_owned(),
            category: header.category.to_owned(),
        });
    }
    if header.tags.len() > MAX_TAGS {
        return Err(CatalogError::TooManyTags {
            pack: id.to_owned(),
            count: header.tags.len(),
        });
    }
    for tag in header.tags {
        check_text(&format!("paquete {id}, etiqueta"), tag)?;
    }
    check_date(&format!("paquete {id}, anadido"), header.added)
}

fn validate_categories(categories: &[Category]) -> Result<(), CatalogError> {
    let mut seen = HashSet::new();
    for category in categories {
        if !is_valid_id(&category.id) {
            return Err(CatalogError::InvalidId {
                id: category.id.clone(),
            });
        }
        if !seen.insert(category.id.as_str()) {
            return Err(CatalogError::RepeatedId {
                id: category.id.clone(),
            });
        }
        check_text(
            &format!("categoría {}, nombre", category.id),
            &category.name,
        )?;
    }
    Ok(())
}

fn validate_withdrawn(withdrawn: &[Withdrawn]) -> Result<(), CatalogError> {
    let mut seen = HashSet::new();
    for withdrawn in withdrawn {
        if !is_valid_id(&withdrawn.id) {
            return Err(CatalogError::InvalidId {
                id: withdrawn.id.clone(),
            });
        }
        if !seen.insert(withdrawn.id.as_str()) {
            return Err(CatalogError::RepeatedId {
                id: withdrawn.id.clone(),
            });
        }
        check_date(
            &format!("retirado {}, fecha", withdrawn.id),
            &withdrawn.date,
        )?;
        check_text(
            &format!("retirado {}, motivo", withdrawn.id),
            &withdrawn.reason,
        )?;
    }
    Ok(())
}

fn check_count(pack: &str, count: usize) -> Result<(), CatalogError> {
    if (MIN_STICKERS..=MAX_STICKERS).contains(&count) {
        Ok(())
    } else {
        Err(CatalogError::StickersCount {
            pack: pack.to_owned(),
            count,
        })
    }
}

fn check_index_file(
    pack: &str,
    file: &str,
    bytes: u64,
    sha256: &str,
    max: u64,
    is_icon: bool,
) -> Result<(), CatalogError> {
    let valid = if is_icon {
        file == ICON_FILE
    } else {
        is_sticker_file(file)
    };
    if !valid {
        return Err(CatalogError::InvalidFile {
            pack: pack.to_owned(),
            file: file.to_owned(),
        });
    }
    if bytes > max {
        return Err(CatalogError::FileTooLarge {
            pack: pack.to_owned(),
            file: file.to_owned(),
            bytes,
            max,
        });
    }
    let hexadecimal = sha256.len() == 64
        && sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if hexadecimal {
        Ok(())
    } else {
        Err(CatalogError::InvalidSum {
            pack: pack.to_owned(),
            file: file.to_owned(),
        })
    }
}

fn check_emojis(pack: &str, file: &str, emojis: &[String]) -> Result<(), CatalogError> {
    validate_emojis(emojis).map_err(|_| CatalogError::InvalidEmojis {
        pack: pack.to_owned(),
        file: file.to_owned(),
    })
}

fn check_text(field: &str, text: &str) -> Result<(), CatalogError> {
    if TEXT_LONG.contains(&text.chars().count()) && !text.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(CatalogError::InvalidText {
            field: field.to_owned(),
        })
    }
}

fn check_date(field: &str, date: &str) -> Result<(), CatalogError> {
    let invalid = || CatalogError::InvalidDate {
        field: field.to_owned(),
        date: date.to_owned(),
    };
    let parts: Vec<&str> = date.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return Err(invalid());
    };
    let number = |text: &str, digits: usize| {
        (text.len() == digits && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<u32>().ok())
            .flatten()
    };
    match (number(year, 4), number(month, 2), number(day, 2)) {
        (Some(_), Some(1..=12), Some(1..=31)) => Ok(()),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn is_valid_id(id: &str) -> bool {
    ID_LONG.contains(&id.len())
        && id.split('-').all(|range| {
            !range.is_empty()
                && range
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

#[must_use]
pub fn is_sticker_file(file: &str) -> bool {
    file.strip_suffix(".webp")
        .is_some_and(|number| number.len() == 2 && number.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use webp_animation::{Encoder, EncoderOptions, EncodingConfig};

    use super::*;
    use crate::encoding::encode_static_webp;
    use crate::sticker_whatsapp::tray_icon;

    fn static_image(tone: u8) -> Vec<u8> {
        let mut rgba = [tone, 40, 90, 255].repeat(512 * 512);
        rgba[..512 * 4 * 100].fill(0);
        encode_static_webp(&rgba, 512, 70).unwrap()
    }

    fn animated() -> Vec<u8> {
        let options = EncoderOptions {
            encoding_config: Some(EncodingConfig::new_lossy(60.0)),
            ..EncoderOptions::default()
        };
        let mut encoder = Encoder::new_with_options((512, 512), options).unwrap();
        for (index, mark_ms) in [0, 200, 500].into_iter().enumerate() {
            let frame = [(index * 100) as u8, 40, 90, 255].repeat(512 * 512);
            encoder.add_frame(&frame, mark_ms).unwrap();
        }
        encoder.finalize(750).unwrap().to_vec()
    }

    fn source(id: &str, animated: bool) -> PackSource {
        PackSource {
            id: id.to_owned(),
            version: 1,
            name: "Gatos garabato".to_owned(),
            author: "Ana".to_owned(),
            license: "CC0-1.0".to_owned(),
            category: "animales".to_owned(),
            tags: vec!["gato".to_owned()],
            animated,
            added: "2026-11-22".to_owned(),
            icon: IconSource {
                file: ICON_FILE.to_owned(),
            },
            stickers: (1..=3)
                .map(|n| StickerSource {
                    file: format!("{n:02}.webp"),
                    emojis: vec!["🐱".to_owned()],
                })
                .collect(),
        }
    }

    fn write_pack(directory: &Path, pack: &PackSource) {
        let folder = directory.join("packs").join(&pack.id);
        fs::create_dir_all(&folder).unwrap();
        let mut first = Vec::new();
        for (index, sticker) in pack.stickers.iter().enumerate() {
            let bytes = if pack.animated {
                animated()
            } else {
                static_image(index as u8 * 40 + 20)
            };
            fs::write(folder.join(&sticker.file), &bytes).unwrap();
            if first.is_empty() {
                first = bytes;
            }
        }
        fs::write(folder.join(ICON_FILE), tray_icon(&first).unwrap()).unwrap();
        fs::write(
            folder.join(PACK_FILE),
            serde_json::to_vec_pretty(pack).unwrap(),
        )
        .unwrap();
    }

    fn test_catalog() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("categories.json"),
            r#"[{"id":"animales","name":"Animales"},{"id":"otros","name":"Otros"}]"#,
        )
        .unwrap();
        fs::write(
            directory.path().join("withdrawn.json"),
            r#"[{"id":"otro-paquete","date":"2026-11-23","reason":"Retirado por una reclamación"}]"#,
        )
        .unwrap();
        write_pack(directory.path(), &source("gatos-garabato", false));
        write_pack(directory.path(), &source("bailes-ocho", true));
        directory
    }

    fn error(directory: &Path) -> String {
        validate_catalog(directory).unwrap_err().to_string()
    }

    fn edit(directory: &Path, id: &str, change: impl FnOnce(&mut PackSource)) {
        let path = directory.join("packs").join(id).join(PACK_FILE);
        let mut pack: PackSource = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        change(&mut pack);
        fs::write(path, serde_json::to_vec(&pack).unwrap()).unwrap();
    }

    #[test]
    fn catalog_valid_generates_its_index() {
        let directory = test_catalog();
        let index = generate_index(directory.path()).unwrap();
        assert_eq!(index.format, 1);
        assert_eq!(index.generated, "2026-11-23T00:00:00Z");
        let ids: Vec<_> = index.packs.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["bailes-ocho", "gatos-garabato"]);
        assert!(index.packs[0].animated);
        assert_eq!(index.packs[1].stickers.len(), 3);
        assert_eq!(index.packs[1].stickers[0].sha256.len(), 64);
        assert_eq!(index.withdrawn[0].id, "otro-paquete");
    }

    #[test]
    fn two_generations_give_same_bytes_and_index_rereads() {
        let directory = test_catalog();
        let first = serialize_index(&generate_index(directory.path()).unwrap()).unwrap();
        let second = serialize_index(&generate_index(directory.path()).unwrap()).unwrap();
        assert_eq!(first, second);
        let read = read_index(first.as_bytes()).unwrap();
        assert_eq!(read, generate_index(directory.path()).unwrap());
    }

    #[test]
    fn sum_is_of_sha256() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn rejects_id_with_uppercase() {
        let directory = test_catalog();
        edit(directory.path(), "gatos-garabato", |p| {
            p.id = "Gatos".to_owned()
        });
        assert!(error(directory.path()).contains("no es válido"));
    }

    #[test]
    fn rejects_folder_whose_name_differs_from_its_id() {
        let directory = test_catalog();
        edit(directory.path(), "gatos-garabato", |p| {
            p.id = "gatos-otro".to_owned()
        });
        assert!(error(directory.path()).contains("tienen que llamarse igual"));
    }

    #[test]
    fn rejects_licenses_outside_of_list() {
        let directory = test_catalog();
        edit(directory.path(), "gatos-garabato", |p| {
            p.license = "MIT".to_owned()
        });
        assert!(error(directory.path()).contains("licencia"));
    }

    #[test]
    fn rejects_category_that_does_not_exist() {
        let directory = test_catalog();
        edit(directory.path(), "gatos-garabato", |p| {
            p.category = "nada".to_owned()
        });
        assert!(error(directory.path()).contains("la categoría"));
    }

    #[test]
    fn rejects_texts_empty_long_or_with_control() {
        for text in ["", &"a".repeat(129), "dos\nlíneas"] {
            let directory = test_catalog();
            edit(directory.path(), "gatos-garabato", |p| {
                p.name = text.to_owned()
            });
            assert!(error(directory.path()).contains("nombre"), "{text:?}");
        }
    }

    #[test]
    fn rejects_over_ten_tags() {
        let directory = test_catalog();
        edit(directory.path(), "gatos-garabato", |p| {
            p.tags = vec!["a".to_owned(); 11];
        });
        assert!(error(directory.path()).contains("11 etiquetas"));
    }

    #[test]
    fn rejects_formed_badly_dates() {
        let directory = test_catalog();
        edit(directory.path(), "gatos-garabato", |p| {
            p.added = "22/11/2026".to_owned()
        });
        assert!(error(directory.path()).contains("AAAA-MM-DD"));
    }

    #[test]
    fn rejects_less_of_three_and_over_thirty_stickers() {
        let directory = test_catalog();
        edit(directory.path(), "gatos-garabato", |p| {
            p.stickers.pop();
        });
        assert!(error(directory.path()).contains("2 stickers"));

        let mut large = source("gatos-garabato", false);
        large.stickers = (1..=31)
            .map(|n| StickerSource {
                file: format!("{n:02}.webp"),
                emojis: vec!["🐱".to_owned()],
            })
            .collect();
        let error = validate_source_pack(
            &large,
            &[Category {
                id: "animales".to_owned(),
                name: "Animales".to_owned(),
            }],
        )
        .unwrap_err();
        assert!(error.to_string().contains("31 stickers"));
    }

    #[test]
    fn rejects_names_of_file_that_are_not_nn_webp() {
        let directory = test_catalog();
        edit(directory.path(), "gatos-garabato", |p| {
            p.stickers[0].file = "uno.webp".to_owned();
        });
        assert!(error(directory.path()).contains("uno.webp"));
    }

    #[test]
    fn rejects_emojis_missing_or_of_more() {
        for emojis in [
            vec![],
            vec!["🐱".to_owned(); 4],
            vec![String::new()],
            vec!["abc".to_owned()],
        ] {
            let directory = test_catalog();
            edit(directory.path(), "gatos-garabato", |p| {
                p.stickers[0].emojis = emojis.clone();
            });
            assert!(error(directory.path()).contains("emojis"), "{emojis:?}");
        }
    }

    #[test]
    fn rejects_mix_static_and_animated() {
        let directory = test_catalog();
        edit(directory.path(), "gatos-garabato", |p| p.animated = true);
        assert!(error(directory.path()).contains("es estático"));
    }

    #[test]
    fn rejects_sticker_that_does_not_pass_validator_of_core() {
        let directory = test_catalog();
        let path = directory.path().join("packs/gatos-garabato/02.webp");
        fs::write(path, b"no soy un webp").unwrap();
        let error = error(directory.path());
        assert!(
            error.contains("02.webp") && error.contains("WebP"),
            "{error}"
        );
    }

    #[test]
    fn rejects_sticker_more_heavy_that_cap() {
        let directory = test_catalog();
        let path = directory.path().join("packs/gatos-garabato/01.webp");
        fs::write(path, vec![0; 500_001]).unwrap();
        assert!(error(directory.path()).contains("500000"));
    }

    #[test]
    fn rejects_icon_that_is_not_96() {
        let directory = test_catalog();
        let large = directory.path().join("packs/gatos-garabato/01.webp");
        let path = directory
            .path()
            .join("packs/gatos-garabato")
            .join(ICON_FILE);
        fs::copy(large, path).unwrap();
        assert!(error(directory.path()).contains("icono"));
    }

    #[test]
    fn rejects_files_missing_and_surplus() {
        let directory = test_catalog();
        let folder = directory.path().join("packs/gatos-garabato");
        fs::remove_file(folder.join("03.webp")).unwrap();
        assert!(error(directory.path()).contains("falta el archivo 03.webp"));
        fs::write(folder.join("03.webp"), static_image(10)).unwrap();
        fs::write(folder.join("notas.txt"), "x").unwrap();
        assert!(error(directory.path()).contains("notas.txt sobra"));
    }

    #[test]
    fn rejects_pack_that_is_in_withdrawn() {
        let directory = test_catalog();
        fs::write(
            directory.path().join("withdrawn.json"),
            r#"[{"id":"bailes-ocho","date":"2026-11-23","reason":"Reclamación"}]"#,
        )
        .unwrap();
        assert!(error(directory.path()).contains("bailes-ocho"));
    }

    #[test]
    fn rejects_bytes_and_sha256_written_to_hand_in_pack_json() {
        let directory = test_catalog();
        let path = directory
            .path()
            .join("packs/gatos-garabato")
            .join(PACK_FILE);
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        json["stickers"][0]["sha256"] = "a".repeat(64).into();
        fs::write(path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(error(directory.path()).contains("sha256"));
    }

    #[test]
    fn catalog_without_packs_also_generates_index() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("categories.json"), "[]").unwrap();
        fs::write(directory.path().join("withdrawn.json"), "[]").unwrap();
        fs::create_dir(directory.path().join("packs")).unwrap();
        let index = generate_index(directory.path()).unwrap();
        assert_eq!(index.generated, "1970-01-01T00:00:00Z");
    }

    fn json_index() -> serde_json::Value {
        let directory = test_catalog();
        let index = generate_index(directory.path()).unwrap();
        serde_json::to_value(index).unwrap()
    }

    fn read(json: &serde_json::Value) -> Result<Index, CatalogError> {
        read_index(&serde_json::to_vec(json).unwrap())
    }

    #[test]
    fn index_of_format_2_does_not_read_although_changes_of_shape() {
        let error = read_index(br#"{"format":2,"things":"new"}"#).unwrap_err();
        assert!(matches!(
            error,
            CatalogError::FormatNotSupported { found: 2 }
        ));
    }

    #[test]
    fn index_broken_or_too_large_rejects() {
        assert!(matches!(
            read_index(b"{"),
            Err(CatalogError::InvalidJson { .. })
        ));
        let large = vec![b' '; 5_000_001];
        assert!(matches!(
            read_index(&large),
            Err(CatalogError::IndexTooLarge { .. })
        ));
    }

    #[test]
    fn index_with_sum_bad_or_size_excessive_rejects() {
        let mut json = json_index();
        json["packs"][0]["stickers"][0]["sha256"] = "XYZ".into();
        assert!(read(&json).unwrap_err().to_string().contains("SHA-256"));

        let mut json = json_index();
        json["packs"][0]["icon"]["bytes"] = 50_001.into();
        assert!(read(&json).unwrap_err().to_string().contains("50001"));
    }

    #[test]
    fn index_with_ids_repeated_or_over_2000_packs_rejects() {
        let mut json = json_index();
        let first = json["packs"][0].clone();
        json["packs"].as_array_mut().unwrap().push(first.clone());
        assert!(read(&json).unwrap_err().to_string().contains("repetido"));

        let packs = json["packs"].as_array_mut().unwrap();
        packs.resize(2001, first);
        assert!(
            read(&json)
                .unwrap_err()
                .to_string()
                .contains("2001 paquetes")
        );
    }

    #[test]
    fn index_with_category_unknown_rejects() {
        let mut json = json_index();
        json["packs"][0]["category"] = "nada".into();
        assert!(
            read(&json)
                .unwrap_err()
                .to_string()
                .contains("la categoría")
        );
    }
}
