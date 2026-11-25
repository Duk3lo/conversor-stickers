//! Backup of the whole library as a zip: export, validation and restore.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};

use conversor_core::{TRAY_ICON_MAX_SIZE, inspect_tray_icon};
use tracing::warn;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use super::{
    ICON_FILE, INDEX_FILE, InvalidIndex, MAX_INDEX_SIZE_BYTES, PACKS_DIRECTORY, STICKER_MAX_SIZE,
    ValidSticker, now, read_file, read_index,
};

pub(crate) const MAX_BACKUP_SIZE_BYTES: u64 = 1_000_000_000;
const MAX_ENTRIES: usize = 200 * 31 + 1;

#[derive(Debug, thiserror::Error)]
pub(crate) enum BackupError {
    #[error("no hay nada que guardar: la biblioteca todavía no tiene paquetes")]
    NoLibrary,

    #[error("no se pudo {action}: {source}")]
    Disk {
        action: &'static str,
        #[source]
        source: io::Error,
    },

    #[error("no se pudo escribir la copia: {0}")]
    Writing(#[source] zip::result::ZipError),

    #[error("no es una copia de seguridad (ZIP) válida: {0}")]
    NotZip(#[source] zip::result::ZipError),

    #[error("la copia está cifrada con contraseña y no se puede restaurar")]
    Encrypted,

    #[error("la copia tiene {entries} archivos y una biblioteca no llega a tanto")]
    TooManyEntries { entries: usize },

    #[error("la copia ocupa más de {} MB descomprimida; no es una biblioteca", MAX_BACKUP_SIZE_BYTES / 1_000_000)]
    TooLarge,

    #[error("la copia lleva un archivo que no es de una biblioteca: «{name}»")]
    InvalidName { name: String },

    #[error("«{name}» aparece dos veces en la copia")]
    Repeated { name: String },

    #[error("«{name}» pesa más de lo que puede pesar en una biblioteca")]
    LargeEntry { name: String },

    #[error("la copia es de una versión más nueva de la app (formato {version}); actualiza la app")]
    NewerVersion { version: u32 },

    #[error("la copia no vale: {reason}")]
    NotValid { reason: String },
}

fn disk(action: &'static str) -> impl FnOnce(io::Error) -> BackupError {
    move |source| BackupError::Disk { action, source }
}

fn limit_of(name: &str) -> Option<u64> {
    let parts: Vec<&str> = name.split('/').collect();
    match parts.as_slice() {
        [file] if *file == INDEX_FILE => Some(MAX_INDEX_SIZE_BYTES),
        [directory, pack, file] if *directory == PACKS_DIRECTORY && is_number(pack) => {
            if *file == ICON_FILE {
                Some(TRAY_ICON_MAX_SIZE)
            } else if file.strip_suffix(".webp").is_some_and(is_number) {
                Some(STICKER_MAX_SIZE)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_number(text: &str) -> bool {
    text.parse::<u32>()
        .is_ok_and(|number| number.to_string() == text)
}

pub(crate) fn export(root: &Path) -> Result<Vec<u8>, BackupError> {
    let index = read_file(&root.join(INDEX_FILE), MAX_INDEX_SIZE_BYTES)
        .map_err(disk("leer el índice de la biblioteca"))?
        .ok_or(BackupError::NoLibrary)?;
    let mut entries = vec![(INDEX_FILE.to_string(), index)];
    let mut packs = sorted_names(&root.join(PACKS_DIRECTORY))?;
    packs.retain(|name| is_number(name));
    for pack in packs {
        let directory = root.join(PACKS_DIRECTORY).join(&pack);
        for file in sorted_names(&directory)? {
            let name = format!("{PACKS_DIRECTORY}/{pack}/{file}");
            let Some(limit) = limit_of(&name) else {
                continue;
            };
            if let Some(bytes) = read_file(&directory.join(&file), limit)
                .map_err(disk("leer un archivo de la biblioteca"))?
            {
                entries.push((name, bytes));
            }
        }
    }

    let json = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    let uncompressed = json.compression_method(CompressionMethod::Stored);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        let options = if name == INDEX_FILE {
            json
        } else {
            uncompressed
        };
        zip.start_file(name, options)
            .map_err(BackupError::Writing)?;
        zip.write_all(&bytes)
            .map_err(|error| BackupError::Writing(error.into()))?;
    }
    Ok(zip.finish().map_err(BackupError::Writing)?.into_inner())
}

fn sorted_names(directory: &Path) -> Result<Vec<String>, BackupError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(disk("recorrer la biblioteca")(error)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(disk("recorrer la biblioteca"))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

pub(crate) fn restore(root: &Path, zip: &Path, max_bytes: u64) -> Result<(), BackupError> {
    let parent = root.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(disk("preparar la restauración"))?;
    let temp = tempfile::Builder::new()
        .prefix("library.restoring-")
        .tempdir_in(parent)
        .map_err(disk("preparar la restauración"))?;
    extract(zip, temp.path(), max_bytes)?;
    check(temp.path())?;

    let new = temp.keep();
    let result = swap_library(root, &new);
    if result.is_err()
        && let Err(error) = fs::remove_dir_all(&new)
    {
        warn!(%error, "no se pudo borrar la copia a medio restaurar");
    }
    result
}

fn swap_library(root: &Path, new: &Path) -> Result<(), BackupError> {
    let exists_flag = fs::symlink_metadata(root).is_ok();
    let set_aside = if exists_flag {
        let seconds = now();
        let name = root
            .file_name()
            .map_or_else(|| "library".into(), |name| name.to_string_lossy());
        let mut destination = root.with_file_name(format!("{name}.before-restore-{seconds}"));
        let mut attempt = 1;
        while fs::symlink_metadata(&destination).is_ok() {
            attempt += 1;
            destination = root.with_file_name(format!("{name}.before-restore-{seconds}-{attempt}"));
        }
        fs::rename(root, &destination).map_err(disk("apartar la biblioteca actual"))?;
        Some(destination)
    } else {
        None
    };
    if let Err(error) = fs::rename(new, root) {
        if let Some(set_aside) = &set_aside
            && let Err(error) = fs::rename(set_aside, root)
        {
            warn!(%error, "no se pudo devolver la biblioteca a su sitio");
        }
        return Err(disk("colocar la biblioteca restaurada")(error));
    }
    if let Some(recent) = &set_aside {
        delete_older_set_aside(root, recent);
    }
    Ok(())
}

fn delete_older_set_aside(root: &Path, recent: &Path) {
    let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else {
        return;
    };
    let prefix = format!("{}.before-restore-", name.to_string_lossy());
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) => {
            warn!(%error, "no se pudieron listar las bibliotecas apartadas");
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != recent
            && entry.file_name().to_string_lossy().starts_with(&prefix)
            && let Err(error) = fs::remove_dir_all(&path)
        {
            warn!(%error, "no se pudo borrar una biblioteca apartada anterior");
        }
    }
}

fn extract(zip: &Path, destination: &Path, max_bytes: u64) -> Result<(), BackupError> {
    let file = fs::File::open(zip).map_err(disk("abrir la copia"))?;
    let mut backup = ZipArchive::new(file).map_err(BackupError::NotZip)?;
    if backup.len() > MAX_ENTRIES {
        return Err(BackupError::TooManyEntries {
            entries: backup.len(),
        });
    }
    let mut declared: u64 = 0;
    for index in 0..backup.len() {
        let entry = backup.by_index_raw(index).map_err(BackupError::NotZip)?;
        declared = declared.saturating_add(entry.size());
    }
    if declared > max_bytes {
        return Err(BackupError::TooLarge);
    }
    let mut read_total: u64 = 0;
    let mut seen = HashSet::new();
    for index in 0..backup.len() {
        let entry = backup.by_index(index).map_err(|error| match error {
            zip::result::ZipError::UnsupportedArchive(_) => BackupError::Encrypted,
            other => BackupError::NotZip(other),
        })?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().map_err(BackupError::NotZip)?.into_owned();
        let limit = limit_of(&name).ok_or_else(|| BackupError::InvalidName {
            name: name.chars().take(60).collect(),
        })?;
        if !seen.insert(name.clone()) {
            return Err(BackupError::Repeated { name });
        }
        let mut content = Vec::new();
        entry
            .take(limit + 1)
            .read_to_end(&mut content)
            .map_err(disk("leer la copia"))?;
        if content.len() as u64 > limit {
            return Err(BackupError::LargeEntry { name });
        }
        read_total += content.len() as u64;
        if read_total > max_bytes {
            return Err(BackupError::TooLarge);
        }
        let path: PathBuf = destination.join(&name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(disk("preparar la restauración"))?;
        }
        fs::write(&path, &content).map_err(disk("extraer la copia"))?;
    }
    Ok(())
}

fn check(root: &Path) -> Result<(), BackupError> {
    let not_valid = |reason: String| BackupError::NotValid { reason };
    let bytes = read_file(&root.join(INDEX_FILE), MAX_INDEX_SIZE_BYTES)
        .map_err(disk("leer el índice de la copia"))?
        .ok_or_else(|| not_valid("falta library.json".to_string()))?;
    let index = read_index(&bytes).map_err(|error| match error {
        InvalidIndex::NewerVersion(version) => BackupError::NewerVersion { version },
        InvalidIndex::Damaged(reason) => not_valid(format!("el índice no vale ({reason})")),
    })?;
    for pack in &index.packs {
        let directory = root.join(PACKS_DIRECTORY).join(pack.id.to_string());
        for sticker in &pack.stickers {
            let path = directory.join(format!("{}.webp", sticker.id));
            let bytes = read_file(&path, STICKER_MAX_SIZE)
                .map_err(disk("leer un sticker de la copia"))?
                .ok_or_else(|| {
                    not_valid(format!(
                        "falta el sticker {} del paquete {}",
                        sticker.id, pack.id
                    ))
                })?;
            let valid = ValidSticker::new(bytes).map_err(|error| {
                not_valid(format!(
                    "el sticker {} del paquete {} no vale: {error}",
                    sticker.id, pack.id
                ))
            })?;
            if valid.kind() != sticker.kind {
                return Err(not_valid(format!(
                    "el sticker {} del paquete {} no es del tipo que dice el índice",
                    sticker.id, pack.id
                )));
            }
        }
        if pack.icon_from.is_some() {
            let icon = read_file(&directory.join(ICON_FILE), TRAY_ICON_MAX_SIZE)
                .map_err(disk("leer un icono de la copia"))?
                .ok_or_else(|| not_valid(format!("falta el icono del paquete {}", pack.id)))?;
            inspect_tray_icon(&icon).map_err(|error| {
                not_valid(format!("el icono del paquete {} no vale: {error}", pack.id))
            })?;
        }
    }
    Ok(())
}

pub(crate) fn proposed_name(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("stickers-backup-{year:04}-{month:02}-{day:02}.zip")
}

pub(crate) fn today_name() -> String {
    proposed_name(now())
}

#[cfg(test)]
mod tests {
    use super::super::tests::static_image;
    use super::super::{FavoriteTarget, Library, MediaOrigin};
    use super::*;

    const INDEX_WITH_ONE_STICKER: &str = r#"{"version":1,"next_id":4,"categories":[],
        "packs":[{"id":1,"name":"P","author":"","category":null,"icon_from":null,
        "favorite":false,"modified":1,"published":null,"origin":{"kind":"own"},
        "stickers":[{"id":2,"kind":"animated","size_bytes":10,"emojis":["🙂"],"favorite":false}]}]}"#;

    fn root(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("library")
    }

    fn zip_in(dir: &tempfile::TempDir, entries: &[(&str, &[u8])]) -> PathBuf {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        save(dir, &zip.finish().unwrap().into_inner())
    }

    fn save(dir: &tempfile::TempDir, bytes: &[u8]) -> PathBuf {
        let folder = dir.path().join("zips");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("copia.zip");
        fs::write(&path, bytes).unwrap();
        path
    }

    fn with_data(root: &Path) -> Library {
        let mut library = Library::open(root).unwrap();
        for (name, tone) in [("Gatos", 10), ("Perros", 90)] {
            let pack = library.create_pack(name, None).unwrap();
            for number in 0..3 {
                library
                    .add_sticker(pack, &static_image(tone + number), MediaOrigin::Own)
                    .unwrap();
            }
        }
        let pack = library.index.packs[0].id;
        let sticker = library.index.packs[0].stickers[1].id;
        library
            .set_emojis(pack, sticker, &["🐱".to_string()])
            .unwrap();
        library
            .mark_favorite(FavoriteTarget::Sticker { pack, id: sticker }, true)
            .unwrap();
        library
    }

    fn directory_contents(dir: &tempfile::TempDir) -> Vec<String> {
        sorted_names(dir.path()).unwrap()
    }

    #[test]
    fn round_trip_leaves_same_library() {
        let origin = tempfile::tempdir().unwrap();
        let original = with_data(&root(&origin));
        let zip = export(&root(&origin)).unwrap();
        assert_eq!(zip, export(&root(&origin)).unwrap());

        let destination = tempfile::tempdir().unwrap();
        let path = save(&destination, &zip);
        restore(&root(&destination), &path, MAX_BACKUP_SIZE_BYTES).unwrap();

        let restored = Library::open(&root(&destination)).unwrap();
        assert_eq!(restored.index, original.index);
        assert_eq!(restored.notice, None);
        for pack in &original.index.packs {
            for sticker in &pack.stickers {
                assert_eq!(
                    fs::read(restored.sticker_path(pack.id, sticker.id)).unwrap(),
                    fs::read(original.sticker_path(pack.id, sticker.id)).unwrap()
                );
            }
            assert_eq!(
                fs::read(restored.icon_path(pack.id)).unwrap(),
                fs::read(original.icon_path(pack.id)).unwrap()
            );
        }
    }

    #[test]
    fn restore_sets_aside_previous_library() {
        let origin = tempfile::tempdir().unwrap();
        with_data(&root(&origin));
        let path = save(&origin, &export(&root(&origin)).unwrap());

        let destination = tempfile::tempdir().unwrap();
        let mut current = Library::open(&root(&destination)).unwrap();
        current.create_pack("Lo de antes", None).unwrap();
        restore(&root(&destination), &path, MAX_BACKUP_SIZE_BYTES).unwrap();

        let names = directory_contents(&destination);
        assert_eq!(names.len(), 2, "{names:?}");
        assert_eq!(names[0], "library");
        assert!(names[1].starts_with("library.before-restore-"));
        let set_aside = Library::open(&destination.path().join(&names[1])).unwrap();
        assert_eq!(set_aside.index.packs[0].name, "Lo de antes");
        let new = Library::open(&root(&destination)).unwrap();
        assert_eq!(new.index.packs[0].name, "Gatos");
    }

    #[test]
    fn restore_keeps_only_most_recent_set_aside_library() {
        let origin = tempfile::tempdir().unwrap();
        with_data(&root(&origin));
        let path = save(&origin, &export(&root(&origin)).unwrap());

        let destination = tempfile::tempdir().unwrap();
        let mut first = Library::open(&root(&destination)).unwrap();
        first.create_pack("Primera", None).unwrap();
        restore(&root(&destination), &path, MAX_BACKUP_SIZE_BYTES).unwrap();
        let mut second_one = Library::open(&root(&destination)).unwrap();
        second_one.create_pack("Segunda", None).unwrap();
        restore(&root(&destination), &path, MAX_BACKUP_SIZE_BYTES).unwrap();

        let names = directory_contents(&destination);
        assert_eq!(names.len(), 2, "{names:?}");
        let set_aside = Library::open(&destination.path().join(&names[1])).unwrap();
        assert!(
            set_aside
                .index
                .packs
                .iter()
                .any(|pack| pack.name == "Segunda")
        );
    }

    fn fails_and_leaves_everything_unchanged(zip: &Path, dir: &tempfile::TempDir) -> BackupError {
        let mut current = Library::open(&root(dir)).unwrap();
        current.create_pack("Original", None).unwrap();
        let before = fs::read(root(dir).join(INDEX_FILE)).unwrap();

        let error = restore(&root(dir), zip, MAX_BACKUP_SIZE_BYTES).unwrap_err();

        assert_eq!(fs::read(root(dir).join(INDEX_FILE)).unwrap(), before);
        let names = directory_contents(dir);
        assert_eq!(names, ["library", "zips"], "{error}");
        error
    }

    #[test]
    fn name_escaping_directory_is_rejected() {
        for name in [
            "../x",
            "packs/1/../../x.webp",
            "/etc/passwd",
            "paquetes\\1\\2.webp",
            "packs/1/otra.txt",
            "packs/01/2.webp",
            "packs/1/-2.webp",
            "otra/cosa.json",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let zip = zip_in(&dir, &[(INDEX_FILE, b"{}"), (name, b"x")]);
            let error = fails_and_leaves_everything_unchanged(&zip, &dir);
            assert!(
                matches!(error, BackupError::InvalidName { .. }),
                "{name}: {error}"
            );
        }
        assert!(!Path::new("/etc/x").exists());
    }

    #[test]
    fn more_than_allowed_declared_or_read_returns_error() {
        let origin = tempfile::tempdir().unwrap();
        with_data(&root(&origin));
        let zip = export(&root(&origin)).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = save(&dir, &zip);
        let mut current = Library::open(&root(&dir)).unwrap();
        current.create_pack("Original", None).unwrap();
        let error = restore(&root(&dir), &path, 1_000).unwrap_err();
        assert!(matches!(error, BackupError::TooLarge), "{error}");
        assert_eq!(directory_contents(&dir), ["library", "zips"]);

        let large = vec![0_u8; STICKER_MAX_SIZE as usize + 1];
        let dir = tempfile::tempdir().unwrap();
        let zip = zip_in(
            &dir,
            &[
                (INDEX_FILE, INDEX_WITH_ONE_STICKER.as_bytes()),
                ("packs/1/2.webp", &large),
            ],
        );
        let error = fails_and_leaves_everything_unchanged(&zip, &dir);
        assert!(matches!(error, BackupError::LargeEntry { .. }), "{error}");
    }

    #[test]
    fn broken_or_missing_webp_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let zip = zip_in(
            &dir,
            &[
                (INDEX_FILE, INDEX_WITH_ONE_STICKER.as_bytes()),
                ("packs/1/2.webp", b"no es un webp"),
            ],
        );
        let error = fails_and_leaves_everything_unchanged(&zip, &dir);
        assert!(matches!(error, BackupError::NotValid { .. }), "{error}");
        assert!(error.to_string().contains("no vale"), "{error}");

        let dir = tempfile::tempdir().unwrap();
        let zip = zip_in(&dir, &[(INDEX_FILE, INDEX_WITH_ONE_STICKER.as_bytes())]);
        let error = fails_and_leaves_everything_unchanged(&zip, &dir);
        assert!(error.to_string().contains("falta el sticker"), "{error}");
    }

    #[test]
    fn sticker_of_different_kind_than_index_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let zip = zip_in(
            &dir,
            &[
                (INDEX_FILE, INDEX_WITH_ONE_STICKER.as_bytes()),
                ("packs/1/2.webp", &static_image(5)),
            ],
        );
        let error = fails_and_leaves_everything_unchanged(&zip, &dir);
        assert!(error.to_string().contains("tipo"), "{error}");
    }

    #[test]
    fn broken_newer_or_missing_index_returns_error() {
        for (index, expected) in [
            (Some(&b"{ roto"[..]), "no vale"),
            (Some(&br#"{"version":4,"next_id":1}"#[..]), "más nueva"),
            (None, "falta library.json"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let entries: Vec<(&str, &[u8])> = index
                .map(|bytes| (INDEX_FILE, bytes))
                .into_iter()
                .chain([("packs/1/2.webp", &b"x"[..])])
                .collect();
            let zip = zip_in(&dir, &entries);
            let error = fails_and_leaves_everything_unchanged(&zip, &dir);
            assert!(error.to_string().contains(expected), "{error}");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = save(&dir, b"esto no es un zip");
        let error = fails_and_leaves_everything_unchanged(&path, &dir);
        assert!(matches!(error, BackupError::NotZip(_)), "{error}");
    }

    #[test]
    fn export_without_library_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(export(&root(&dir)), Err(BackupError::NoLibrary)));
    }

    #[test]
    fn name_includes_the_date() {
        assert_eq!(proposed_name(0), "stickers-backup-1970-01-01.zip");
        assert_eq!(
            proposed_name(20_779 * 86_400 + 3_600),
            "stickers-backup-2026-11-22.zip"
        );
        assert_eq!(
            proposed_name(19_782 * 86_400),
            "stickers-backup-2024-02-29.zip"
        );
    }
}
