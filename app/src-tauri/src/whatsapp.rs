//! Published copy of the packs for "Add to WhatsApp".
//!
//! Rust writes `whatsapp/pack_<id>/` and `whatsapp/index.json`; the Kotlin `ContentProvider`
//! (`StickerProvider`) only reads that. It does not depend on `library.rs`: it receives an
//! already built `PublishablePack`.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use conversor_core::{StickerError, StickerKind, inspect_tray_icon, inspect_whatsapp_sticker};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::storage::write_atomic;

const INDEX_VERSION: u32 = 1;
const INDEX_FILE: &str = "index.json";
const TRAY_FILE: &str = "tray.png";
const PACK_PREFIX: &str = "pack_";
const MAX_INDEX_SIZE_BYTES: u64 = 1_000_000;
const MIN_STICKERS: usize = 3;
const MAX_STICKERS: usize = 30;
pub(crate) const MAX_PUBLISHED_PACKS: usize = 10;
const MAX_TEXT_LENGTH: usize = 128;
const DEFAULT_NAME: &str = "Stickers";
const DEFAULT_AUTHOR: &str = "Conversor de stickers";

#[derive(Debug, thiserror::Error)]
pub(crate) enum WhatsappError {
    #[error("{0}")]
    InvalidPack(String),
    #[error("no se pudo {action}: {source}")]
    Disk {
        action: &'static str,
        source: io::Error,
    },
}

fn disk(action: &'static str) -> impl FnOnce(io::Error) -> WhatsappError {
    move |source| WhatsappError::Disk { action, source }
}

#[derive(Debug, Clone)]
pub(crate) struct PublishableSticker {
    pub(crate) path: PathBuf,
    pub(crate) animated: bool,
    pub(crate) emojis: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PublishablePack {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) animated: bool,
    pub(crate) stickers: Vec<PublishableSticker>,
    pub(crate) tray: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Index {
    version: u32,
    packs: Vec<PackEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    author: String,
    animated: bool,
    pub(crate) image_version: String,
    tray: String,
    stickers: Vec<StickerEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StickerEntry {
    file: String,
    emojis: Vec<String>,
}

fn pack_directory_name(id: u32) -> String {
    format!("{PACK_PREFIX}{id}")
}

fn sanitize(text: &str, default: &str) -> String {
    let mut clean: String = text
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.' | ',' | '\''))
        .collect();
    while clean.contains("..") {
        clean = clean.replace("..", "");
    }
    let cropped: String = clean.trim().chars().take(MAX_TEXT_LENGTH).collect();
    let cropped = cropped.trim();
    if cropped.is_empty() {
        default.to_string()
    } else {
        cropped.to_string()
    }
}

fn validate(pack: &PublishablePack) -> Result<(), WhatsappError> {
    let not_valid = |reason: String| Err(WhatsappError::InvalidPack(reason));
    let how_many = pack.stickers.len();
    if !(MIN_STICKERS..=MAX_STICKERS).contains(&how_many) {
        return not_valid(format!(
            "WhatsApp pide de {MIN_STICKERS} a {MAX_STICKERS} stickers y el paquete tiene {how_many}"
        ));
    }
    if pack
        .stickers
        .iter()
        .any(|sticker| sticker.animated != pack.animated)
    {
        return not_valid(
            "WhatsApp no admite stickers estáticos y animados en el mismo paquete".to_string(),
        );
    }
    Ok(())
}

/// Checks the real files of the pack against what WhatsApp enforces, before launching its
/// intent: WhatsApp's own rejection message does not say which sticker fails. The pack-level
/// rules (count, kind, emojis, texts) are the "Antes de publicar" checks of `library.rs`, which
/// `to_publish` already applies; this adds what only the bytes tell.
pub(crate) fn check_files(pack: &PublishablePack) -> Result<(), WhatsappError> {
    let not_valid = |reason: String| Err(WhatsappError::InvalidPack(reason));
    for (position, sticker) in pack.stickers.iter().enumerate() {
        let number = position + 1;
        let bytes = fs::read(&sticker.path).map_err(disk("leer un sticker"))?;
        let info = match inspect_whatsapp_sticker(&bytes) {
            Ok(info) => info,
            Err(error) => return not_valid(sticker_failure(number, &error)),
        };
        let animated = info.kind == StickerKind::Animated;
        if animated != pack.animated {
            let (is, others) = if animated {
                ("animado", "estáticos")
            } else {
                ("estático", "animados")
            };
            return not_valid(format!(
                "El sticker {number} es {is} y el resto del paquete son {others}: \
                 quítalo o conviértelo para que todos sean del mismo tipo"
            ));
        }
    }
    let tray = fs::read(&pack.tray).map_err(disk("leer el icono"))?;
    if let Err(error) = inspect_tray_icon(&tray) {
        return not_valid(format!(
            "{error}. Elige otro icono en el paquete: tiene que ser un PNG de 96x96 \
             y de 50 KB como mucho"
        ));
    }
    Ok(())
}

fn sticker_failure(number: usize, error: &StickerError) -> String {
    let fix = match error {
        StickerError::AnimatedSingleFrame => return format!("Sticker {number}: {error}"),
        StickerError::NotWebp | StickerError::Unreadable { .. } => {
            "quítalo del paquete y vuelve a añadirlo desde su archivo original"
        }
        StickerError::TooHeavy { .. } => {
            "vuelve a convertirlo con un tamaño máximo menor (animado hasta 500 KB, estático hasta 100 KB)"
        }
        StickerError::IncorrectSide { .. } => "vuelve a convertirlo para que mida 512x512",
        StickerError::TooLong { .. } => "acórtalo a 10 s o menos y vuelve a convertirlo",
        StickerError::FrameTooShort { .. } => {
            "vuelve a convertirlo con menos fotogramas por segundo (cada frame debe durar 8 ms o más)"
        }
        _ => "quítalo del paquete o vuelve a convertirlo",
    };
    format!("Sticker {number}: {error}. Para arreglarlo, {fix}")
}

fn read_index(root: &Path) -> Index {
    let empty = Index {
        version: INDEX_VERSION,
        packs: Vec::new(),
    };
    let path = root.join(INDEX_FILE);
    let read = fs::File::open(&path).and_then(|file| {
        let mut bytes = Vec::new();
        file.take(MAX_INDEX_SIZE_BYTES).read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    match read {
        Ok(bytes) => match serde_json::from_slice::<Index>(&bytes) {
            Ok(index) if index.version == INDEX_VERSION => index,
            _ => {
                warn!("el índice de WhatsApp no se entiende; se empieza de cero");
                empty
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => empty,
        Err(error) => {
            warn!(%error, "no se pudo leer el índice de WhatsApp; se empieza de cero");
            empty
        }
    }
}

fn save_index(root: &Path, index: &Index) -> Result<(), WhatsappError> {
    let json = serde_json::to_vec_pretty(index)
        .map_err(|error| disk("preparar el índice de WhatsApp")(error.into()))?;
    write_atomic(&root.join(INDEX_FILE), &json).map_err(disk("guardar el índice"))
}

fn write_backup(directory: &Path, pack: &PublishablePack) -> io::Result<()> {
    fs::create_dir(directory)?;
    for (position, sticker) in pack.stickers.iter().enumerate() {
        fs::copy(
            &sticker.path,
            directory.join(format!("{:02}.webp", position + 1)),
        )?;
    }
    fs::copy(&pack.tray, directory.join(TRAY_FILE))?;
    Ok(())
}

pub(crate) fn publish(root: &Path, pack: &PublishablePack) -> Result<PackEntry, WhatsappError> {
    validate(pack)?;
    fs::create_dir_all(root).map_err(disk("crear la carpeta de WhatsApp"))?;
    let mut index = read_index(root);
    let directory_name = pack_directory_name(pack.id);
    let previous = index
        .packs
        .iter()
        .position(|entry| entry.id == directory_name);
    if previous.is_none() && index.packs.len() >= MAX_PUBLISHED_PACKS {
        return Err(WhatsappError::InvalidPack(format!(
            "WhatsApp admite como mucho {MAX_PUBLISHED_PACKS} paquetes por app; \
             borra uno que ya no uses"
        )));
    }
    let version = previous
        .and_then(|position| index.packs[position].image_version.parse::<u64>().ok())
        .map_or(1, |version| version.saturating_add(1));
    let entry = PackEntry {
        id: directory_name.clone(),
        name: sanitize(&pack.name, DEFAULT_NAME),
        author: sanitize(&pack.author, DEFAULT_AUTHOR),
        animated: pack.animated,
        image_version: version.to_string(),
        tray: TRAY_FILE.to_string(),
        stickers: pack
            .stickers
            .iter()
            .enumerate()
            .map(|(position, sticker)| StickerEntry {
                file: format!("{:02}.webp", position + 1),
                emojis: sticker.emojis.clone(),
            })
            .collect(),
    };

    let destination = root.join(&directory_name);
    let new = root.join(format!("{directory_name}.nuevo"));
    let old = root.join(format!("{directory_name}.viejo"));
    let _ = fs::remove_dir_all(&new);
    let _ = fs::remove_dir_all(&old);
    if let Err(error) = write_backup(&new, pack) {
        let _ = fs::remove_dir_all(&new);
        return Err(disk("copiar los stickers")(error));
    }
    if destination.exists() {
        fs::rename(&destination, &old).map_err(disk("apartar la copia anterior"))?;
    }
    if let Err(error) = fs::rename(&new, &destination) {
        let _ = fs::rename(&old, &destination);
        let _ = fs::remove_dir_all(&new);
        return Err(disk("colocar la copia nueva")(error));
    }
    let _ = fs::remove_dir_all(&old);

    match previous {
        Some(position) => index.packs[position] = entry.clone(),
        None => index.packs.push(entry.clone()),
    }
    save_index(root, &index)?;
    Ok(entry)
}

pub(crate) fn prune(root: &Path, existing_ids: &[u32]) -> Result<usize, WhatsappError> {
    if !root.is_dir() {
        return Ok(0);
    }
    let current: Vec<String> = existing_ids
        .iter()
        .map(|id| pack_directory_name(*id))
        .collect();
    let mut index = read_index(root);
    let before = index.packs.len();
    index.packs.retain(|entry| current.contains(&entry.id));
    let removed = before - index.packs.len();
    if removed > 0 {
        save_index(root, &index)?;
    }
    let entries = fs::read_dir(root).map_err(disk("recorrer la carpeta de WhatsApp"))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let works = index.packs.iter().any(|published| published.id == name);
        if !name.starts_with(PACK_PREFIX) || works {
            continue;
        }
        let path = entry.path();
        let deleted = match entry.file_type() {
            Ok(kind) if kind.is_dir() => fs::remove_dir_all(&path),
            _ => fs::remove_file(&path),
        };
        if let Err(error) = deleted {
            warn!(%error, "no se pudo borrar una copia publicada que ya no existe");
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::Value;
    use tempfile::TempDir;

    use super::*;

    const EXAMPLES: &str = include_str!("../../src/contract-mobile.examples.json");

    struct Origin {
        _dir: TempDir,
        pack: PublishablePack,
    }

    fn origin(id: u32, stickers: usize, animated: bool, content: &str) -> Origin {
        let dir = tempfile::tempdir().unwrap();
        let stickers = (0..stickers)
            .map(|n| {
                let path = dir.path().join(format!("{n}.webp"));
                fs::write(&path, format!("{content}{n}")).unwrap();
                PublishableSticker {
                    path,
                    animated,
                    emojis: vec!["🙂".to_string()],
                }
            })
            .collect();
        let tray = dir.path().join("icon.png");
        fs::write(&tray, format!("bandeja {content}")).unwrap();
        let pack = PublishablePack {
            id,
            name: "Gatos".into(),
            author: "Conversor de stickers".into(),
            animated,
            stickers,
            tray,
        };
        Origin { _dir: dir, pack }
    }

    fn names(directory: &Path) -> BTreeSet<String> {
        fs::read_dir(directory)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn expected(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    #[test]
    fn publish_creates_copy_and_example_index() {
        let root = tempfile::tempdir().unwrap();
        let origin = origin(3, 3, true, "a");
        for _ in 0..4 {
            publish(root.path(), &origin.pack).unwrap();
        }
        let backup = root.path().join("pack_3");
        assert_eq!(
            names(&backup),
            expected(&["01.webp", "02.webp", "03.webp", "tray.png"])
        );
        assert_eq!(fs::read_to_string(backup.join("02.webp")).unwrap(), "a1");
        assert_eq!(
            fs::read_to_string(backup.join("tray.png")).unwrap(),
            "bandeja a"
        );
        let published: Value =
            serde_json::from_slice(&fs::read(root.path().join("index.json")).unwrap()).unwrap();
        let examples: Value = serde_json::from_str(EXAMPLES).unwrap();
        assert_eq!(published, examples["index"]);
    }

    #[test]
    fn index_has_emojis_of_each_sticker() {
        let root = tempfile::tempdir().unwrap();
        let mut origin = origin(1, 3, false, "a");
        origin.pack.stickers[0].emojis = vec!["🐱".to_string(), "👍🏽".to_string()];
        origin.pack.stickers[2].emojis = vec!["❤️".to_string()];

        let entry = publish(root.path(), &origin.pack).unwrap();

        let emojis: Vec<&[String]> = entry.stickers.iter().map(|s| s.emojis.as_slice()).collect();
        assert_eq!(emojis[0], ["🐱", "👍🏽"]);
        assert_eq!(emojis[1], ["🙂"]);
        assert_eq!(emojis[2], ["❤️"]);
    }

    #[test]
    fn index_example_reads_with_rust_type() {
        let examples: Value = serde_json::from_str(EXAMPLES).unwrap();
        let index: Index = serde_json::from_value(examples["index"].clone()).unwrap();
        assert_eq!(serde_json::to_value(&index).unwrap(), examples["index"]);
    }

    #[test]
    fn republish_bumps_version_and_leaves_no_leftovers() {
        let root = tempfile::tempdir().unwrap();
        publish(root.path(), &origin(1, 5, false, "a").pack).unwrap();
        let new = origin(1, 3, false, "b");
        let entry = publish(root.path(), &new.pack).unwrap();
        assert_eq!(entry.image_version, "2");
        assert_eq!(names(root.path()), expected(&["pack_1", "index.json"]));
        let backup = root.path().join("pack_1");
        assert_eq!(
            names(&backup),
            expected(&["01.webp", "02.webp", "03.webp", "tray.png"])
        );
        assert_eq!(fs::read_to_string(backup.join("01.webp")).unwrap(), "b0");
    }

    #[test]
    fn rejects_fewer_than_3_more_than_30_and_mixed() {
        let root = tempfile::tempdir().unwrap();
        for how_many in [2, 31] {
            let error = publish(root.path(), &origin(1, how_many, false, "a").pack).unwrap_err();
            assert!(matches!(error, WhatsappError::InvalidPack(_)));
        }
        let mut mixed = origin(1, 3, false, "a");
        mixed.pack.stickers[1].animated = true;
        let error = publish(root.path(), &mixed.pack).unwrap_err();
        assert!(matches!(error, WhatsappError::InvalidPack(_)));
        assert!(names(root.path()).is_empty(), "no debe escribir nada");
    }

    #[test]
    fn eleventh_pack_is_rejected_but_updating_a_published_one_is_not() {
        let root = tempfile::tempdir().unwrap();
        for id in 1..=10 {
            publish(root.path(), &origin(id, 3, false, "a").pack).unwrap();
        }
        let error = publish(root.path(), &origin(11, 3, false, "a").pack).unwrap_err();
        assert!(error.to_string().contains("10 paquetes"), "{error}");
        assert!(!root.path().join("pack_11").exists());
        publish(root.path(), &origin(7, 3, false, "b").pack).unwrap();
    }

    #[test]
    fn name_and_author_are_sanitized() {
        assert_eq!(sanitize("Gatos..", "x"), "Gatos");
        assert_eq!(sanitize("<b>", "x"), "b");
        assert_eq!(sanitize("Niño's, Ñu_1-2.", "x"), "Niño's, Ñu_1-2.");
        assert_eq!(sanitize("a....b", "x"), "ab");
        assert_eq!(sanitize(&"a".repeat(200), "x").chars().count(), 128);
        assert_eq!(sanitize("", "Stickers"), "Stickers");
        assert_eq!(sanitize("<>!!", "Stickers"), "Stickers");
        let root = tempfile::tempdir().unwrap();
        let mut origin = origin(1, 3, false, "a");
        origin.pack.name = "  ".into();
        origin.pack.author = "".into();
        let entry = publish(root.path(), &origin.pack).unwrap();
        assert_eq!(entry.name, "Stickers");
        assert_eq!(entry.author, "Conversor de stickers");
    }

    #[test]
    fn prune_deletes_copies_and_entries_of_packs_that_no_longer_exist() {
        let root = tempfile::tempdir().unwrap();
        for id in 1..=3 {
            publish(root.path(), &origin(id, 3, false, "a").pack).unwrap();
        }
        fs::create_dir(root.path().join("pack_9")).unwrap();
        assert_eq!(prune(root.path(), &[1, 3]).unwrap(), 1);
        assert_eq!(
            names(root.path()),
            expected(&["pack_1", "pack_3", "index.json"])
        );
        let index = read_index(root.path());
        let ids: Vec<_> = index.packs.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["pack_1", "pack_3"]);
    }

    #[test]
    fn prune_without_folder_does_nothing() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(prune(&root.path().join("no_existe"), &[1]).unwrap(), 0);
    }

    #[test]
    fn broken_index_is_treated_as_empty() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("index.json"), "{ roto").unwrap();
        publish(root.path(), &origin(2, 3, false, "a").pack).unwrap();
        assert_eq!(read_index(root.path()).packs.len(), 1);
    }

    /// Pseudo-random opaque pixels; `levels` limits the channel values to tune the file size.
    fn noise(side: u32, levels: u32) -> Vec<u8> {
        let mut state = 12_345_u32;
        (0..side * side * 4)
            .map(|position| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                if position % 4 == 3 {
                    255
                } else {
                    ((state >> 24) % levels) as u8
                }
            })
            .collect()
    }

    fn frame(tone: u8) -> Vec<u8> {
        [tone, 20, 30, 255].repeat(512 * 512)
    }

    fn animated_with(side: u32, marks_ms: &[i32], end_ms: i32, frames: &[Vec<u8>]) -> Vec<u8> {
        let mut encoder = webp_animation::Encoder::new((side, side)).unwrap();
        for (mark, frame) in marks_ms.iter().zip(frames) {
            encoder.add_frame(frame, *mark).unwrap();
        }
        encoder.finalize(end_ms).unwrap().to_vec()
    }

    /// A pack of three real stickers (the first one replaced by `first`) and a valid icon.
    fn real_pack(first: Vec<u8>, animated: bool) -> Origin {
        let dir = tempfile::tempdir().unwrap();
        let make = |tone| {
            if animated {
                crate::library::tests::animated(tone)
            } else {
                crate::library::tests::static_image(tone)
            }
        };
        let stickers = [first, make(60), make(120)]
            .into_iter()
            .enumerate()
            .map(|(n, bytes)| {
                let path = dir.path().join(format!("{n}.webp"));
                fs::write(&path, bytes).unwrap();
                PublishableSticker {
                    path,
                    animated,
                    emojis: vec!["🙂".to_string()],
                }
            })
            .collect();
        let tray = dir.path().join("icon.png");
        let icon = conversor_core::tray_icon(&crate::library::tests::static_image(9)).unwrap();
        fs::write(&tray, icon).unwrap();
        let pack = PublishablePack {
            id: 1,
            name: "Gatos".into(),
            author: "Yo".into(),
            animated,
            stickers,
            tray,
        };
        Origin { _dir: dir, pack }
    }

    fn failure(origin: &Origin) -> String {
        check_files(&origin.pack).unwrap_err().to_string()
    }

    #[test]
    fn valid_static_and_animated_packs_pass_the_file_check() {
        check_files(&real_pack(crate::library::tests::static_image(1), false).pack).unwrap();
        check_files(&real_pack(crate::library::tests::animated(1), true).pack).unwrap();
    }

    #[test]
    fn a_file_that_is_not_webp_names_its_sticker() {
        let message = failure(&real_pack(b"no es un webp".to_vec(), false));
        assert!(
            message.starts_with("Sticker 1: el sticker no es un WebP"),
            "{message}"
        );
        assert!(message.contains("vuelve a añadirlo"), "{message}");
    }

    #[test]
    fn wrong_dimensions_are_reported() {
        let small = animated_with(
            256,
            &[0, 200, 400],
            600,
            &[noise(256, 1), noise(256, 256), noise(256, 3)],
        );
        let message = failure(&real_pack(small, true));
        assert!(message.contains("Sticker 1"), "{message}");
        assert!(
            message.contains("256x256 y tiene que medir 512x512"),
            "{message}"
        );
    }

    #[test]
    fn animated_over_500_kb_is_reported() {
        let frames = [noise(512, 256), noise(512, 255), noise(512, 254)];
        let heavy = animated_with(512, &[0, 200, 400], 600, &frames);
        assert!(heavy.len() > 500_000);
        let message = failure(&real_pack(heavy, true));
        assert!(
            message.contains("Sticker 1") && message.contains("pesa"),
            "{message}"
        );
        assert!(message.contains("500 KB"), "{message}");
    }

    #[test]
    fn static_over_100_kb_is_reported() {
        let image = conversor_core::RgbaImage::new(512, 512, noise(512, 64)).unwrap();
        let preset = conversor_core::ImagePreset {
            max_size: conversor_core::MaxSize::from_bytes(10_000_000),
            ..conversor_core::WHATSAPP_STATIC_STICKER
        };
        let heavy = conversor_core::convert_image(
            &image,
            &preset,
            &conversor_core::FramingSpec::default(),
            conversor_core::Tracking::none(),
        )
        .unwrap()
        .bytes;
        assert!(
            heavy.len() > 100_000 && heavy.len() <= 500_000,
            "{}",
            heavy.len()
        );
        let message = failure(&real_pack(heavy, false));
        assert!(
            message.contains("Sticker 1") && message.contains("100 KB"),
            "{message}"
        );
    }

    #[test]
    fn animation_over_10_seconds_is_reported() {
        let long = animated_with(
            512,
            &[0, 5_000, 10_000],
            12_000,
            &[frame(10), frame(90), frame(170)],
        );
        let message = failure(&real_pack(long, true));
        assert!(
            message.contains("Sticker 1") && message.contains("10 s"),
            "{message}"
        );
    }

    #[test]
    fn frames_under_8_ms_are_reported() {
        let quick = animated_with(512, &[0, 4, 8], 12, &[frame(10), frame(90), frame(170)]);
        let message = failure(&real_pack(quick, true));
        assert!(
            message.contains("Sticker 1") && message.contains("8 ms"),
            "{message}"
        );
    }

    #[test]
    fn a_static_sticker_in_an_animated_pack_is_reported() {
        let message = failure(&real_pack(crate::library::tests::static_image(1), true));
        assert!(message.contains("El sticker 1 es estático"), "{message}");
    }

    #[test]
    fn a_bad_tray_icon_is_reported() {
        let mut origin = real_pack(crate::library::tests::static_image(1), false);
        fs::write(&origin.pack.tray, b"no es un png").unwrap();
        let message = failure(&origin);
        assert!(
            message.contains("no es un PNG") && message.contains("96x96"),
            "{message}"
        );
        origin.pack.tray = origin._dir.path().join("falta.png");
        assert!(matches!(
            check_files(&origin.pack),
            Err(WhatsappError::Disk { .. })
        ));
    }
}
