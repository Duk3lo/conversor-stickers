//! Import and export of `.wastickers`: a ZIP with `title.txt`, `author.txt`, the icon and the
//! stickers.
//!
//! An imported `.wastickers` is untrusted: the file size, the number of entries and the
//! decompressed size are limited before reserving memory, and everything is processed in memory.
//! Only the last part of an entry name is used to classify it; it is never written to disk with it.

use std::collections::BTreeMap;
use std::io::{self, Cursor, Read, Write};
use std::sync::atomic::AtomicBool;

use conversor_core::{
    DEFAULT_EMOJI, FramingSpec, Tracking, WHATSAPP_STATIC_STICKER, convert_image,
    inspect_tray_icon, validate_emojis,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::contract::StickerKind;
use crate::library::{MAX_STICKERS, MIN_STICKERS_TO_EXPORT, ValidSticker};
use crate::mobile::Executables;

pub(crate) const MAX_SIZE_BYTES: u64 = 20_000_000;
const MAX_ENTRIES: usize = 64;
const MAX_PER_ENTRY: u64 = 600_000;
const MAX_DECOMPRESSED: u64 = 20_000_000;
const MAX_TEXT_BYTES: usize = 1_000;
const MAX_EMOJIS_BYTES: usize = 8_000;
const EMOJIS_FILE: &str = "emojis.json";
const MAX_TEXT_LENGTH: usize = 128;
const STATIC_PNG_SIDE: u32 = 512;
const STATIC_PNG_MAX_SIZE: usize = 100_000;
const DEFAULT_AUTHOR: &str = "Conversor de stickers";
const DEFAULT_TITLE: &str = "Paquete importado";
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

#[derive(Debug, thiserror::Error)]
pub(crate) enum WastickersError {
    #[error("el archivo pesa más de 20 MB; un paquete de stickers no llega a tanto")]
    TooLarge,

    #[error("no es un archivo .wastickers (ZIP) válido: {0}")]
    NotZip(#[source] zip::result::ZipError),

    #[error("el archivo tiene {entries} entradas y un paquete tiene como mucho {MAX_ENTRIES}")]
    TooManyEntries { entries: usize },

    #[error("el archivo está cifrado con contraseña y no se puede importar")]
    Encrypted,

    #[error("el archivo descomprime más de 20 MB; no es un paquete de stickers")]
    DecompressesTooMuch,

    #[error("el archivo no tiene ningún sticker válido para WhatsApp")]
    NoStickers,

    #[error("no se pudo escribir el .wastickers: {0}")]
    Writing(#[source] zip::result::ZipError),

    #[error("no se pudo preparar la conversión de un PNG: {0}")]
    Temp(#[source] io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackToExport {
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) icon: Vec<u8>,
    pub(crate) stickers: Vec<Vec<u8>>,
    pub(crate) emojis: Vec<Vec<String>>,
}

#[derive(Debug)]
pub(crate) struct Imported {
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) icon: Option<Vec<u8>>,
    pub(crate) stickers: Vec<ValidSticker>,
    pub(crate) emojis: Vec<Vec<String>>,
    pub(crate) notices: Vec<String>,
}

pub(crate) fn export(pack: &PackToExport) -> Result<Vec<u8>, WastickersError> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let author = if pack.author.is_empty() {
        DEFAULT_AUTHOR
    } else {
        &pack.author
    };
    let mut entries: Vec<(String, &[u8])> = vec![
        ("title.txt".to_string(), pack.title.as_bytes()),
        ("author.txt".to_string(), author.as_bytes()),
        ("cover.png".to_string(), &pack.icon),
    ];
    for (number, sticker) in pack.stickers.iter().enumerate() {
        entries.push((format!("{:02}.webp", number + 1), sticker));
    }
    let emojis: BTreeMap<String, &Vec<String>> = pack
        .emojis
        .iter()
        .enumerate()
        .map(|(number, emojis)| (format!("{:02}.webp", number + 1), emojis))
        .collect();
    let emojis = serde_json::to_vec_pretty(&emojis)
        .map_err(|error| WastickersError::Writing(io::Error::from(error).into()))?;
    entries.push((EMOJIS_FILE.to_string(), &emojis));
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(name, options)
            .map_err(WastickersError::Writing)?;
        zip.write_all(bytes)
            .map_err(|error| WastickersError::Writing(error.into()))?;
    }
    Ok(zip.finish().map_err(WastickersError::Writing)?.into_inner())
}

enum Class {
    Title,
    Author,
    Icon,
    Emojis,
    Webp,
    Png,
}

fn classify(final_name: &str) -> Option<Class> {
    let lowercase = final_name.to_lowercase();
    let (base, extension) = lowercase.rsplit_once('.')?;
    match (base, extension) {
        ("title", "txt") => Some(Class::Title),
        ("author", "txt") => Some(Class::Author),
        ("cover" | "tray", _) => Some(Class::Icon),
        ("emojis", "json") => Some(Class::Emojis),
        (_, "webp") => Some(Class::Webp),
        (_, "png") => Some(Class::Png),
        _ => None,
    }
}

fn final_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn clean_text(text: &str) -> String {
    let clean: String = text
        .trim_start_matches('\u{feff}')
        .chars()
        .map(|char| if char.is_control() { ' ' } else { char })
        .collect();
    let cropped: String = clean.trim().chars().take(MAX_TEXT_LENGTH).collect();
    cropped.trim_end().to_string()
}

fn for_notice(name: &str) -> String {
    clean_text(name).chars().take(60).collect()
}

pub(crate) fn import(
    bytes: &[u8],
    file_name: &str,
    executables: &Executables,
) -> Result<Imported, WastickersError> {
    if bytes.len() as u64 > MAX_SIZE_BYTES {
        return Err(WastickersError::TooLarge);
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(WastickersError::NotZip)?;
    if zip.len() > MAX_ENTRIES {
        return Err(WastickersError::TooManyEntries { entries: zip.len() });
    }
    let mut notices = Vec::new();
    let mut title = None;
    let mut author = None;
    let mut icon = None;
    let mut stickers: Vec<ValidSticker> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut emojis_by_name = BTreeMap::new();
    let mut valid = 0;
    let mut decompressed = 0;
    let mut temp = None;

    for index in 0..zip.len() {
        let entry = zip.by_index_data(index).map_err(WastickersError::NotZip)?;
        if entry.encrypted() {
            return Err(WastickersError::Encrypted);
        }
        if entry.is_dir() {
            continue;
        }
        let name = match entry.name() {
            Ok(name) => final_name(&name).to_string(),
            Err(_) => continue,
        };
        let Some(class) = classify(&name) else {
            continue;
        };
        let content = match read_entry(&mut zip, index) {
            Ok(content) => content,
            Err(reason) => {
                notices.push(format!("Se omitió «{}»: {reason}.", for_notice(&name)));
                continue;
            }
        };
        decompressed += content.len() as u64;
        if decompressed > MAX_DECOMPRESSED {
            return Err(WastickersError::DecompressesTooMuch);
        }
        let skip_it = |reason: &dyn std::fmt::Display| {
            format!("Se omitió «{}»: {reason}.", for_notice(&name))
        };
        let sticker = match class {
            Class::Title => {
                title = read_text(&content);
                continue;
            }
            Class::Author => {
                author = read_text(&content);
                continue;
            }
            Class::Emojis => {
                match read_emojis(&content) {
                    Ok(read) => emojis_by_name = read,
                    Err(reason) => notices.push(format!(
                        "Se omitió «{}»: {reason}; los stickers llevan el emoji por defecto.",
                        for_notice(&name)
                    )),
                }
                continue;
            }
            Class::Icon => {
                if icon.is_none() {
                    match inspect_tray_icon(&content) {
                        Ok(()) => icon = Some(content),
                        Err(error) => notices.push(format!(
                            "El icono «{}» no vale ({error}); se genera del primer sticker.",
                            for_notice(&name)
                        )),
                    }
                }
                continue;
            }
            Class::Webp => ValidSticker::new(content).map_err(|error| skip_it(&error)),
            Class::Png => {
                convert_png(&content, &mut temp, executables)?.map_err(|reason| skip_it(&reason))
            }
        };
        let sticker = match sticker {
            Ok(sticker) => sticker,
            Err(notice) => {
                notices.push(notice);
                continue;
            }
        };
        if let Some(first) = stickers.first()
            && first.kind() != sticker.kind()
        {
            notices.push(skip_it(&format!(
                "es {} y el paquete es de stickers {}",
                if sticker.kind() == StickerKind::Animated {
                    "animado"
                } else {
                    "estático"
                },
                if first.kind() == StickerKind::Animated {
                    "animados"
                } else {
                    "estáticos"
                },
            )));
            continue;
        }
        valid += 1;
        if stickers.len() < MAX_STICKERS {
            stickers.push(sticker);
            names.push(name.to_lowercase());
        }
    }
    if let Some(temp) = temp
        && let Err(error) = temp.close()
    {
        tracing::warn!(%error, "no se pudo borrar el temporal de la importación");
    }

    if stickers.is_empty() {
        return Err(WastickersError::NoStickers);
    }
    if valid > MAX_STICKERS {
        notices.push(format!(
            "El archivo trae {valid} stickers válidos y un paquete admite {MAX_STICKERS}: \
             se importan los {MAX_STICKERS} primeros."
        ));
    }
    if stickers.len() < MIN_STICKERS_TO_EXPORT {
        notices.push(format!(
            "El paquete tiene {} {}: hasta tener {MIN_STICKERS_TO_EXPORT} no se podrá exportar.",
            stickers.len(),
            if stickers.len() == 1 {
                "sticker"
            } else {
                "stickers"
            }
        ));
    }
    let title = title
        .filter(|title| !title.is_empty())
        .or_else(|| {
            let base = file_name
                .rsplit_once('.')
                .map_or(file_name, |(base, _)| base);
            Some(clean_text(base)).filter(|base| !base.is_empty())
        })
        .unwrap_or_else(|| DEFAULT_TITLE.to_string());
    let emojis = names
        .iter()
        .map(|name| {
            emojis_by_name
                .get(name)
                .filter(|emojis| validate_emojis(emojis).is_ok())
                .cloned()
                .unwrap_or_else(|| vec![DEFAULT_EMOJI.to_string()])
        })
        .collect();
    Ok(Imported {
        title,
        author: author.unwrap_or_default(),
        icon,
        stickers,
        emojis,
        notices,
    })
}

fn read_entry(zip: &mut ZipArchive<Cursor<&[u8]>>, index: usize) -> Result<Vec<u8>, String> {
    let entry = zip
        .by_index(index)
        .map_err(|error| format!("no se pudo leer ({error})"))?;
    let mut content = Vec::new();
    entry
        .take(MAX_PER_ENTRY + 1)
        .read_to_end(&mut content)
        .map_err(|error| format!("no se pudo leer ({error})"))?;
    if content.len() as u64 > MAX_PER_ENTRY {
        return Err("pasa de 600 KB".to_string());
    }
    Ok(content)
}

fn read_emojis(content: &[u8]) -> Result<BTreeMap<String, Vec<String>>, String> {
    if content.len() > MAX_EMOJIS_BYTES {
        return Err("es demasiado grande".to_string());
    }
    let read: BTreeMap<String, Vec<String>> =
        serde_json::from_slice(content).map_err(|error| format!("no se entiende ({error})"))?;
    Ok(read
        .into_iter()
        .map(|(name, emojis)| (name.to_lowercase(), emojis))
        .collect())
}

fn read_text(content: &[u8]) -> Option<String> {
    if content.len() > MAX_TEXT_BYTES {
        return None;
    }
    std::str::from_utf8(content).ok().map(clean_text)
}

fn convert_png(
    png: &[u8],
    temp: &mut Option<tempfile::TempDir>,
    executables: &Executables,
) -> Result<Result<ValidSticker, String>, WastickersError> {
    if png.len() > STATIC_PNG_MAX_SIZE {
        return Ok(Err("un PNG estático pesa como mucho 100 KB".to_string()));
    }
    let Some(sides) = png
        .strip_prefix(PNG_SIGNATURE)
        .and_then(|ihdr| ihdr.first_chunk::<8>())
    else {
        return Ok(Err("no es un PNG".to_string()));
    };
    let [a0, a1, a2, a3, b0, b1, b2, b3] = *sides;
    let sides = (
        u32::from_be_bytes([a0, a1, a2, a3]),
        u32::from_be_bytes([b0, b1, b2, b3]),
    );
    if sides != (STATIC_PNG_SIDE, STATIC_PNG_SIDE) {
        return Ok(Err("un PNG estático tiene que medir 512x512".to_string()));
    }
    let directory = match temp {
        Some(directory) => directory,
        None => temp.insert(
            tempfile::Builder::new()
                .prefix("conversor-stickers-import-")
                .tempdir()
                .map_err(WastickersError::Temp)?,
        ),
    };
    let path = directory.path().join("sticker.png");
    std::fs::write(&path, png).map_err(WastickersError::Temp)?;
    let never = AtomicBool::new(false);
    let converted = executables
        .decoder(&path)
        .decode_image(&never)
        .map_err(|error| error.to_string())
        .and_then(|image| {
            convert_image(
                &image,
                &WHATSAPP_STATIC_STICKER,
                &FramingSpec::default(),
                Tracking::none(),
            )
            .map_err(|error| error.to_string())
        })
        .and_then(|report| ValidSticker::new(report.bytes).map_err(|error| error.to_string()));
    Ok(converted)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::library::tests::{animated, static_image};

    fn options() -> SimpleFileOptions {
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            zip.start_file(*name, options()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn icon() -> Vec<u8> {
        conversor_core::tray_icon(&animated(77)).unwrap()
    }

    fn pack() -> PackToExport {
        PackToExport {
            title: "Gatos".to_string(),
            author: "Ana".to_string(),
            icon: icon(),
            stickers: vec![animated(10), animated(120), animated(240)],
            emojis: vec![
                vec!["🐱".to_string()],
                vec!["🙂".to_string(), "👍🏽".to_string()],
                vec!["❤️".to_string()],
            ],
        }
    }

    fn bytes_of(imported: &Imported) -> Vec<&[u8]> {
        imported.stickers.iter().map(ValidSticker::bytes).collect()
    }

    #[test]
    fn export_and_import_returns_the_same() {
        let pack = pack();
        let zip = export(&pack).unwrap();

        let imported = import(&zip, "otro.wastickers", &Executables::default()).unwrap();

        assert_eq!(imported.title, "Gatos");
        assert_eq!(imported.author, "Ana");
        assert_eq!(imported.icon.as_deref(), Some(pack.icon.as_slice()));
        let expected: Vec<&[u8]> = pack.stickers.iter().map(Vec::as_slice).collect();
        assert_eq!(bytes_of(&imported), expected);
        assert!(imported.notices.is_empty(), "{:?}", imported.notices);
        assert_eq!(imported.emojis, pack.emojis);
    }

    #[test]
    fn without_emojis_json_or_with_bad_emojis_default_is_used() {
        let default = vec![vec![DEFAULT_EMOJI.to_string()]; 3];
        let (a, b, c) = (animated(1), animated(2), animated(3));
        let without = zip_of(&[("1.webp", &a), ("2.webp", &b), ("3.webp", &c)]);
        let imported = import(&without, "x", &Executables::default()).unwrap();
        assert_eq!(imported.emojis, default);
        assert!(imported.notices.is_empty());

        let json = r#"{"1.webp":["hola"],"3.WEBP":["🐱"],"9.webp":["🙂"]}"#.as_bytes();
        let with = zip_of(&[
            ("1.webp", &a),
            ("2.webp", &b),
            ("emojis.json", json),
            ("3.webp", &c),
        ]);
        let imported = import(&with, "x", &Executables::default()).unwrap();
        assert_eq!(
            imported.emojis,
            [
                vec![DEFAULT_EMOJI.to_string()],
                vec![DEFAULT_EMOJI.to_string()],
                vec!["🐱".to_string()]
            ]
        );

        let broken = zip_of(&[("1.webp", &a), ("emojis.json", b"no es json")]);
        let imported = import(&broken, "x", &Executables::default()).unwrap();
        assert_eq!(imported.emojis, default[..1]);
        assert!(
            imported.notices[0].contains("emojis.json"),
            "{:?}",
            imported.notices
        );
    }

    #[test]
    fn exporting_twice_returns_same_bytes_and_names() {
        let first = export(&pack()).unwrap();
        assert_eq!(first, export(&pack()).unwrap());
        let mut zip = ZipArchive::new(Cursor::new(first.as_slice())).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "title.txt",
                "author.txt",
                "cover.png",
                "01.webp",
                "02.webp",
                "03.webp",
                "emojis.json"
            ]
        );
        assert_eq!(
            zip.by_index(3).unwrap().compression(),
            CompressionMethod::Deflated
        );
    }

    #[test]
    fn without_author_app_default_is_exported() {
        let zip = export(&PackToExport {
            author: String::new(),
            ..pack()
        })
        .unwrap();
        assert_eq!(
            import(&zip, "x", &Executables::default()).unwrap().author,
            DEFAULT_AUTHOR
        );
    }

    #[test]
    fn only_last_part_of_names_counts() {
        let one = animated(1);
        let two = animated(2);
        let three = animated(3);
        let zip = zip_of(&[
            ("/x.webp", &one),
            ("dir/../y.webp", &two),
            ("..\\..\\z.webp", &three),
            ("/etc/title.txt", b"Gatos"),
        ]);
        let imported = import(&zip, "x", &Executables::default()).unwrap();
        assert_eq!(bytes_of(&imported), [&one[..], &two[..], &three[..]]);
        assert_eq!(imported.title, "Gatos");
    }

    #[test]
    fn icon_can_be_cover_or_tray_and_if_missing_there_is_none() {
        let sticker = animated(5);
        for name in ["cover.png", "TRAY.png", "tray.webp"] {
            let icon = icon();
            let zip = zip_of(&[(name, &icon), ("1.webp", &sticker)]);
            let imported = import(&zip, "x", &Executables::default()).unwrap();
            if name.ends_with(".png") {
                assert_eq!(imported.icon, Some(icon), "{name}");
            }
        }
        let without_icon = import(
            &zip_of(&[("1.webp", &sticker)]),
            "x",
            &Executables::default(),
        )
        .unwrap();
        assert_eq!(without_icon.icon, None);
        let bad = import(
            &zip_of(&[("cover.png", b"no"), ("1.webp", &sticker)]),
            "x",
            &Executables::default(),
        )
        .unwrap();
        assert_eq!(bad.icon, None);
        assert!(
            bad.notices.iter().any(|notice| notice.contains("icono")),
            "{:?}",
            bad.notices
        );
    }

    #[test]
    fn of_35_stickers_30_are_imported_with_notice() {
        let sticker = static_image(8);
        let names: Vec<String> = (0..35).map(|i| format!("{i:02}.webp")).collect();
        let entries: Vec<(&str, &[u8])> = names
            .iter()
            .map(|n| (n.as_str(), sticker.as_slice()))
            .collect();

        let imported = import(&zip_of(&entries), "x", &Executables::default()).unwrap();

        assert_eq!(imported.stickers.len(), 30);
        assert!(
            imported.notices[0].contains("35 stickers"),
            "{:?}",
            imported.notices
        );
    }

    #[test]
    fn other_kind_ones_are_skipped_with_notice() {
        let animated = animated(4);
        let static_image = static_image(4);
        let zip = zip_of(&[
            ("a.webp", &animated),
            ("b.webp", &static_image),
            ("c.webp", &animated),
            ("roto.webp", b"RIFF"),
        ]);

        let imported = import(&zip, "x", &Executables::default()).unwrap();

        assert_eq!(imported.stickers.len(), 2);
        assert!(
            imported
                .stickers
                .iter()
                .all(|s| s.kind() == StickerKind::Animated)
        );
        let notices = imported.notices.join("\n");
        assert!(notices.contains("«b.webp»: es estático"), "{notices}");
        assert!(notices.contains("«roto.webp»"), "{notices}");
        assert!(notices.contains("no se podrá exportar"), "{notices}");
    }

    fn png_512() -> Vec<u8> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("p.png");
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "color=c=red:size=512x512",
                "-frames:v",
                "1",
            ])
            .arg(&path)
            .output()
            .expect("hace falta ffmpeg para esta prueba");
        assert!(output.status.success());
        std::fs::read(path).unwrap()
    }

    #[test]
    fn static_png_converts_to_webp() {
        let png = png_512();
        let zip = zip_of(&[("00.png", &png), ("01.png", &png[..20])]);

        let imported = import(&zip, "x", &Executables::default()).unwrap();

        assert_eq!(imported.stickers.len(), 1, "{:?}", imported.notices);
        assert_eq!(imported.stickers[0].kind(), StickerKind::Static);
        assert_eq!(&imported.stickers[0].bytes()[8..12], b"WEBP");
        assert!(imported.notices.join(" ").contains("«01.png»"));
    }

    #[test]
    fn png_of_other_size_does_not_reach_ffmpeg() {
        let mut png = png_512();
        png[16..20].copy_from_slice(&511u32.to_be_bytes());
        let zip = zip_of(&[("00.png", &png), ("1.webp", &static_image(1))]);
        let imported = import(&zip, "x", &Executables::default()).unwrap();
        assert!(
            imported.notices[0].contains("512x512"),
            "{:?}",
            imported.notices
        );
    }

    fn declare_size(zip: &mut [u8], name: &str, bytes: u32) {
        for (signature, offset) in [(b"PK\x03\x04", 22), (b"PK\x01\x02", 24)] {
            let start = zip
                .windows(4)
                .enumerate()
                .filter(|(_, window)| window == signature)
                .map(|(i, _)| i)
                .find(|&i| {
                    let name_in = if signature == b"PK\x03\x04" { 30 } else { 46 };
                    zip[i + name_in..].starts_with(name.as_bytes())
                })
                .unwrap();
            zip[start + offset..start + offset + 4].copy_from_slice(&bytes.to_le_bytes());
        }
    }

    #[test]
    fn entry_decompressing_more_than_declared_is_cut() {
        let bomb = vec![0u8; 10_000_000];
        let sticker = animated(9);
        let mut zip = zip_of(&[("bomba.webp", &bomb), ("1.webp", &sticker)]);
        assert!(zip.len() < 100_000);
        declare_size(&mut zip, "bomba.webp", 1_000);

        let imported = import(&zip, "x", &Executables::default()).unwrap();

        assert_eq!(bytes_of(&imported), [&sticker[..]]);
        assert!(
            imported.notices[0].contains("«bomba.webp»"),
            "{:?}",
            imported.notices
        );
    }

    #[test]
    fn entry_over_600_kb_is_skipped() {
        let large = vec![7u8; 700_000];
        let zip = zip_of(&[("grande.webp", &large), ("1.webp", &animated(3))]);
        let imported = import(&zip, "x", &Executables::default()).unwrap();
        assert!(
            imported.notices[0].contains("pasa de 600 KB"),
            "{:?}",
            imported.notices
        );
    }

    #[test]
    fn over_20_mb_decompressed_in_total_is_an_error() {
        let large = vec![1u8; 590_000];
        let names: Vec<String> = (0..36).map(|i| format!("{i}.webp")).collect();
        let entries: Vec<(&str, &[u8])> = names
            .iter()
            .map(|n| (n.as_str(), large.as_slice()))
            .collect();
        let error = import(&zip_of(&entries), "x", &Executables::default()).unwrap_err();
        assert!(
            matches!(error, WastickersError::DecompressesTooMuch),
            "{error}"
        );
    }

    #[test]
    fn encrypted_zip_returns_error() {
        let mut zip = zip_of(&[("1.webp", &animated(1))]);
        for (signature, offset) in [(b"PK\x03\x04", 6), (b"PK\x01\x02", 8)] {
            let start = zip
                .windows(4)
                .position(|window| window == signature)
                .unwrap();
            zip[start + offset] |= 1;
        }
        let error = import(&zip, "x", &Executables::default()).unwrap_err();
        assert!(matches!(error, WastickersError::Encrypted), "{error}");
    }

    #[test]
    fn file_of_21_mb_returns_error_without_reading() {
        let error = import(&vec![0; 21_000_000], "x", &Executables::default()).unwrap_err();
        assert!(matches!(error, WastickersError::TooLarge), "{error}");
    }

    #[test]
    fn zip_with_65_entries_returns_error() {
        let names: Vec<String> = (0..65).map(|i| format!("{i}.txt")).collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
        let error = import(&zip_of(&entries), "x", &Executables::default()).unwrap_err();
        assert!(
            matches!(error, WastickersError::TooManyEntries { entries: 65 }),
            "{error}"
        );
    }

    #[test]
    fn non_zip_or_without_stickers_returns_error() {
        assert!(matches!(
            import(b"hola", "x", &Executables::default()),
            Err(WastickersError::NotZip(_))
        ));
        assert!(matches!(
            import(
                &zip_of(&[("title.txt", b"Gatos"), ("leeme.md", b"x")]),
                "x",
                &Executables::default()
            ),
            Err(WastickersError::NoStickers)
        ));
    }

    #[test]
    fn title_and_author_are_cleaned() {
        let long = "ñ".repeat(200);
        let sticker = animated(1);
        let zip = zip_of(&[
            ("title.txt", "\u{feff} Gatos\n\u{7}malos \r\n".as_bytes()),
            ("author.txt", long.as_bytes()),
            ("1.webp", &sticker),
        ]);
        let imported = import(&zip, "x", &Executables::default()).unwrap();
        assert_eq!(imported.title, "Gatos  malos");
        assert_eq!(imported.author.chars().count(), 128);
        let without = zip_of(&[("title.txt", &[b'a'; 1_001]), ("1.webp", &sticker)]);
        assert_eq!(
            import(&without, "Mis gatos.wastickers", &Executables::default())
                .unwrap()
                .title,
            "Mis gatos"
        );
        assert_eq!(
            import(&without, ".wastickers", &Executables::default())
                .unwrap()
                .title,
            DEFAULT_TITLE
        );
    }
}
