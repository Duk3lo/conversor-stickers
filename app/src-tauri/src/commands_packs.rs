//! Sticker pack commands. All are `async` because all end up on disk: blocking work runs in
//! `spawn_blocking`. Argument names are the ones `contract.ts` sends.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use tauri::{AppHandle, State, Window};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tracing::warn;

use crate::commands::in_background;
use crate::contract::{
    CreatedPack, FavoriteTarget, ImportResult, LibraryUi, PackEdit, UiError, UiErrorKind,
};
use crate::library::backup::{self, BackupError};
use crate::library::{Library, LibraryError};
use crate::mobile::{self, Executables};
use crate::session::{Session, with_extension};
use crate::storage::write_atomic;
use crate::wastickers::{self, WastickersError};

pub(crate) struct LibraryState {
    root: PathBuf,
    opened: Mutex<Option<Library>>,
}

impl LibraryState {
    pub(crate) fn new(root: PathBuf) -> Self {
        LibraryState {
            root,
            opened: Mutex::new(None),
        }
    }

    pub(crate) fn with<T>(
        &self,
        work: impl FnOnce(&mut Library) -> Result<T, LibraryError>,
    ) -> Result<T, UiError> {
        let mut opened = self.opened.lock().unwrap_or_else(PoisonError::into_inner);
        let library = match opened.take() {
            Some(library) => opened.insert(library),
            None => opened.insert(Library::open(&self.root)?),
        };
        Ok(work(library)?)
    }

    fn export_backup(&self) -> Result<Vec<u8>, UiError> {
        let _abierta = self.opened.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(backup::export(&self.root)?)
    }

    fn restore_backup(&self, zip: &Path) -> Result<LibraryUi, UiError> {
        let mut opened = self.opened.lock().unwrap_or_else(PoisonError::into_inner);
        *opened = None;
        backup::restore(&self.root, zip, backup::MAX_BACKUP_SIZE_BYTES)?;
        let new = opened.insert(Library::open(&self.root)?);
        Ok(new.ui())
    }

    fn apply_change<T>(
        &self,
        change: impl FnOnce(&mut Library) -> Result<T, LibraryError>,
    ) -> Result<LibraryUi, UiError> {
        self.with(|library| {
            change(library)?;
            Ok(library.ui())
        })
    }
}

impl From<LibraryError> for UiError {
    fn from(error: LibraryError) -> Self {
        let kind = match error {
            LibraryError::Disk { .. }
            | LibraryError::OutOfIds
            | LibraryError::NewerVersion { .. } => UiErrorKind::Other,
            LibraryError::InvalidName { .. }
            | LibraryError::RepeatedCategory
            | LibraryError::MissingCategory { .. }
            | LibraryError::MissingPack { .. }
            | LibraryError::MissingSticker { .. }
            | LibraryError::Full { .. }
            | LibraryError::InvalidSticker(_)
            | LibraryError::MixedKind { .. }
            | LibraryError::Emoji(_)
            | LibraryError::InvalidOrder
            | LibraryError::NoFavorites
            | LibraryError::Icon(_)
            | LibraryError::NotExportable { .. }
            | LibraryError::HasExternalStickers => UiErrorKind::InvalidPack,
        };
        UiError::new(kind, error.to_string())
    }
}

impl From<BackupError> for UiError {
    fn from(error: BackupError) -> Self {
        let kind = match error {
            BackupError::Disk { .. } | BackupError::Writing(_) => UiErrorKind::Other,
            BackupError::NoLibrary
            | BackupError::NotZip(_)
            | BackupError::Encrypted
            | BackupError::TooManyEntries { .. }
            | BackupError::TooLarge
            | BackupError::InvalidName { .. }
            | BackupError::Repeated { .. }
            | BackupError::LargeEntry { .. }
            | BackupError::NewerVersion { .. }
            | BackupError::NotValid { .. } => UiErrorKind::InvalidPack,
        };
        UiError::new(kind, error.to_string())
    }
}

impl From<WastickersError> for UiError {
    fn from(error: WastickersError) -> Self {
        let kind = match error {
            WastickersError::Writing(_) | WastickersError::Temp(_) => UiErrorKind::Other,
            WastickersError::TooLarge
            | WastickersError::NotZip(_)
            | WastickersError::TooManyEntries { .. }
            | WastickersError::Encrypted
            | WastickersError::DecompressesTooMuch
            | WastickersError::NoStickers => UiErrorKind::InvalidPack,
        };
        UiError::new(kind, error.to_string())
    }
}

fn add_created(
    library: &LibraryState,
    session: &Session,
    pack: u32,
    entry: u32,
) -> Result<LibraryUi, UiError> {
    let bytes = session.result(entry)?;
    let origin = session.media_origin(entry)?;
    library.apply_change(|opened| opened.add_sticker(pack, &bytes, origin))
}

fn proposed_name(name: &str) -> String {
    format!("{}.wastickers", safe_base(name))
}

pub(crate) fn sticker_file_name(pack_name: &str, position: usize) -> String {
    format!("{}-{position}.webp", safe_base(pack_name))
}

fn safe_base(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|char| {
            if char.is_control() || "/\\:*?\"<>|".contains(char) {
                '_'
            } else {
                char
            }
        })
        .collect();
    let sanitized = sanitized.trim().trim_matches('.').trim();
    if sanitized.is_empty() {
        "paquete".to_string()
    } else {
        sanitized.to_string()
    }
}

/// Writes one sticker where the user picks. The file is a WebP, so the name always ends in it.
fn save_sticker_to(path: &Path, bytes: &[u8]) -> Result<String, UiError> {
    let path = with_extension(path, "webp");
    write_atomic(&path, bytes).map_err(|error| {
        UiError::new(
            UiErrorKind::Other,
            format!("no se pudo guardar {}: {error}", path.display()),
        )
    })?;
    Ok(path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned()))
}

fn export_to(library: &LibraryState, id: u32, path: &Path) -> Result<String, UiError> {
    let pack = library.with(|opened| opened.to_export(id))?;
    let bytes = wastickers::export(&pack)?;
    let path = with_extension(path, "wastickers");
    write_atomic(&path, &bytes).map_err(|error| {
        UiError::new(
            UiErrorKind::Other,
            format!("no se pudo guardar {}: {error}", path.display()),
        )
    })?;
    Ok(path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned()))
}

fn read_wastickers(path: &Path) -> Result<Vec<u8>, UiError> {
    let not_valid = |message: String| UiError::new(UiErrorKind::InvalidPack, message);
    let metadata = fs::metadata(path)
        .map_err(|error| not_valid(format!("no se pudo leer el archivo: {error}")))?;
    if !metadata.is_file() {
        return Err(not_valid("no es un archivo".to_string()));
    }
    if metadata.len() > wastickers::MAX_SIZE_BYTES {
        return Err(WastickersError::TooLarge.into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(wastickers::MAX_SIZE_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| not_valid(format!("no se pudo leer el archivo: {error}")))?;
    Ok(bytes)
}

fn import_from(
    library: &LibraryState,
    bytes: &[u8],
    file_name: &str,
    executables: &Executables,
) -> Result<ImportResult, UiError> {
    let imported = wastickers::import(bytes, file_name, executables)?;
    library.with(|opened| {
        let pack = opened.import(&imported)?;
        Ok(ImportResult {
            library: opened.ui(),
            pack,
            notices: imported.notices,
        })
    })
}

#[tauri::command]
pub(crate) async fn library(library: State<'_, Arc<LibraryState>>) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.with(|opened| Ok(opened.ui()))).await
}

#[tauri::command]
pub(crate) async fn create_category(
    name: String,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.create_category(&name))).await
}

#[tauri::command]
pub(crate) async fn rename_category(
    id: u32,
    name: String,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.rename_category(id, &name))).await
}

#[tauri::command]
pub(crate) async fn delete_category(
    id: u32,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.delete_category(id))).await
}

#[tauri::command]
pub(crate) async fn create_pack(
    name: String,
    category: Option<u32>,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.create_pack(&name, category))).await
}

#[tauri::command]
pub(crate) async fn edit_pack(
    id: u32,
    edit: PackEdit,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || {
        library.apply_change(|opened| opened.edit_pack(id, &edit.name, &edit.author, edit.category))
    })
    .await
}

#[tauri::command]
pub(crate) async fn delete_pack(
    id: u32,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.delete_pack(id))).await
}

#[tauri::command]
pub(crate) async fn add_sticker(
    pack: u32,
    entry: u32,
    library: State<'_, Arc<LibraryState>>,
    session: State<'_, Arc<Session>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    let session = Arc::clone(&session);
    in_background(move || add_created(&library, &session, pack, entry)).await
}

#[tauri::command]
pub(crate) async fn remove_sticker(
    pack: u32,
    sticker: u32,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.remove_sticker(pack, sticker))).await
}

#[tauri::command]
pub(crate) async fn use_as_icon(
    pack: u32,
    sticker: u32,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.use_as_icon(pack, sticker))).await
}

#[tauri::command]
pub(crate) async fn set_emojis(
    pack: u32,
    sticker: u32,
    emojis: Vec<String>,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.set_emojis(pack, sticker, &emojis)))
        .await
}

#[tauri::command]
pub(crate) async fn mark_favorite(
    target: FavoriteTarget,
    favorite: bool,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.mark_favorite(target, favorite)))
        .await
}

#[tauri::command]
pub(crate) async fn reorder_stickers(
    pack: u32,
    order: Vec<u32>,
    library: State<'_, Arc<LibraryState>>,
) -> Result<LibraryUi, UiError> {
    let library = Arc::clone(&library);
    in_background(move || library.apply_change(|opened| opened.reorder_stickers(pack, &order)))
        .await
}

#[tauri::command]
pub(crate) async fn create_pack_from_favorites(
    name: String,
    library: State<'_, Arc<LibraryState>>,
) -> Result<CreatedPack, UiError> {
    let library = Arc::clone(&library);
    in_background(move || {
        library.with(|opened| {
            let first = opened
                .create_pack_from_favorites(&name)?
                .first()
                .copied()
                .ok_or(LibraryError::NoFavorites)?;
            Ok(CreatedPack {
                pack: first,
                library: opened.ui(),
            })
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn export_pack(
    id: u32,
    app: AppHandle,
    window: Window,
    library: State<'_, Arc<LibraryState>>,
) -> Result<Option<String>, UiError> {
    let library = Arc::clone(&library);
    in_background(move || {
        let proposal = library.with(|opened| {
            let pack = opened.to_export(id)?;
            Ok(proposed_name(&pack.title))
        })?;
        let dialog = app.dialog().file();
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let picked = dialog
            .set_title("Exportar paquete")
            .set_file_name(&proposal)
            .add_filter("Paquete de stickers", &["wastickers"])
            .blocking_save_file();
        let Some(path) = picked else {
            return Ok(None);
        };
        match path {
            FilePath::Path(path) => export_to(&library, id, &path).map(Some),
            url => {
                let pack = library.with(|opened| opened.to_export(id))?;
                let bytes = wastickers::export(&pack)?;
                let name = mobile::picked_name(&app, &url);
                mobile::write_to(&app, url, &bytes)?;
                Ok(Some(name))
            }
        }
    })
    .await
}

#[tauri::command]
pub(crate) async fn export_sticker(
    pack: u32,
    sticker: u32,
    app: AppHandle,
    window: Window,
    library: State<'_, Arc<LibraryState>>,
) -> Result<Option<String>, UiError> {
    let library = Arc::clone(&library);
    in_background(move || {
        let exported = library.with(|opened| opened.sticker_to_export(pack, sticker))?;
        let dialog = app.dialog().file();
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let picked = dialog
            .set_title("Guardar sticker")
            .set_file_name(sticker_file_name(&exported.pack_name, exported.position))
            .add_filter("WebP", &["webp"])
            .blocking_save_file();
        match picked {
            None => Ok(None),
            Some(FilePath::Path(path)) => save_sticker_to(&path, &exported.bytes).map(Some),
            Some(url) => {
                let name = mobile::picked_name(&app, &url);
                mobile::write_to(&app, url, &exported.bytes)?;
                Ok(Some(name))
            }
        }
    })
    .await
}

#[tauri::command]
pub(crate) async fn export_backup(
    app: AppHandle,
    window: Window,
    library: State<'_, Arc<LibraryState>>,
) -> Result<Option<String>, UiError> {
    let library = Arc::clone(&library);
    in_background(move || {
        let bytes = library.export_backup()?;
        let dialog = app.dialog().file();
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let picked = dialog
            .set_title("Exportar copia de seguridad")
            .set_file_name(backup::today_name())
            .add_filter("Copia de seguridad", &["zip"])
            .blocking_save_file();
        let Some(path) = picked else {
            return Ok(None);
        };
        match path {
            FilePath::Path(path) => {
                let path = with_extension(&path, "zip");
                write_atomic(&path, &bytes).map_err(|error| {
                    UiError::new(
                        UiErrorKind::Other,
                        format!("no se pudo guardar {}: {error}", path.display()),
                    )
                })?;
                Ok(Some(path.to_string_lossy().into_owned()))
            }
            url => {
                let name = mobile::picked_name(&app, &url);
                mobile::write_to(&app, url, &bytes)?;
                Ok(Some(name))
            }
        }
    })
    .await
}

#[tauri::command]
pub(crate) async fn restore_backup(
    app: AppHandle,
    window: Window,
    library: State<'_, Arc<LibraryState>>,
) -> Result<Option<LibraryUi>, UiError> {
    let library = Arc::clone(&library);
    in_background(move || {
        let dialog = app.dialog().file();
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let picked = dialog
            .set_title("Restaurar copia de seguridad")
            .add_filter("Copia de seguridad", &["zip"])
            .add_filter("Todos los archivos", &["*"])
            .blocking_pick_file();
        let Some(path) = picked else {
            return Ok(None);
        };
        let path = mobile::local_path(&app, path, backup::MAX_BACKUP_SIZE_BYTES)?;
        let restored = library.restore_backup(&path);
        if cfg!(target_os = "android")
            && let Err(error) = fs::remove_file(&path)
        {
            warn!(%error, "no se pudo borrar la copia de seguridad elegida");
        }
        restored.map(Some)
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_wastickers(
    app: AppHandle,
    window: Window,
    library: State<'_, Arc<LibraryState>>,
) -> Result<Option<ImportResult>, UiError> {
    let library = Arc::clone(&library);
    in_background(move || {
        let dialog = app.dialog().file();
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let picked = dialog
            .set_title("Importar paquete")
            .add_filter("Paquete de stickers", &["wastickers", "zip"])
            .add_filter("Todos los archivos", &["*"])
            .blocking_pick_file();
        let Some(path) = picked else {
            return Ok(None);
        };
        let name = mobile::picked_name(&app, &path);
        let path = mobile::local_path(&app, path, wastickers::MAX_SIZE_BYTES)?;
        let bytes = read_wastickers(&path);
        if cfg!(target_os = "android")
            && let Err(error) = fs::remove_file(&path)
        {
            warn!(%error, "no se pudo borrar la copia del paquete importado");
        }
        let executables = mobile::executables(&app)?;
        import_from(&library, &bytes?, &name, &executables).map(Some)
    })
    .await
}

pub(crate) fn library_root(app_data: &Path) -> PathBuf {
    app_data.join("library")
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::contract::{CreateRequest, FramingSpecUi, MediaOrigin, PriorityUi, ProgressUi};
    use crate::library::tests::animated;

    fn state(directory: &tempfile::TempDir) -> LibraryState {
        LibraryState::new(library_root(directory.path()))
    }

    #[test]
    fn changes_return_library_and_survive_reopening() {
        let directory = tempfile::tempdir().unwrap();
        let library = state(&directory);
        let ui = library
            .apply_change(|opened| opened.create_category("Anime"))
            .unwrap();
        let category = ui.categories[0].id;
        let ui = library
            .apply_change(|opened| opened.create_pack("Gatos", Some(category)))
            .unwrap();
        assert_eq!(ui.packs[0].category, Some(category));

        let reopened = state(&directory).with(|opened| Ok(opened.ui())).unwrap();

        assert_eq!(reopened, ui);
        assert!(directory.path().join("library/library.json").is_file());
    }

    #[test]
    fn library_errors_have_their_kind() {
        let directory = tempfile::tempdir().unwrap();
        let library = state(&directory);
        let error = library
            .apply_change(|opened| opened.delete_pack(9))
            .unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidPack);
        assert_eq!(error.message, "no hay ningún paquete con el número 9");
        let error = library
            .apply_change(|opened| opened.create_pack("", None))
            .unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidPack);
    }

    #[test]
    fn newer_version_library_errors_every_time_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let root = library_root(directory.path());
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("library.json"), r#"{"version":9}"#).unwrap();
        let library = state(&directory);
        for _ in 0..2 {
            let error = library.with(|opened| Ok(opened.ui())).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::Other);
            assert!(
                error.message.contains("versión más nueva"),
                "{}",
                error.message
            );
        }
        assert_eq!(
            fs::read_to_string(root.join("library.json")).unwrap(),
            r#"{"version":9}"#
        );
    }

    fn video(directory: &Path) -> PathBuf {
        let path = directory.join("baile.mp4");
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x120:rate=10",
                "-t",
                "1",
            ])
            .args(["-c:v", "mpeg4", "-pix_fmt", "yuv420p"])
            .arg(&path)
            .output()
            .expect("hace falta ffmpeg para esta prueba");
        assert!(output.status.success());
        path
    }

    fn create(session: &Session, id: u32, preset: &str) {
        let request = CreateRequest {
            id,
            preset: preset.to_string(),
            start_s: 0.0,
            seconds: 1.0,
            speed: 1.0,
            priority: PriorityUi::Balance,
            fps: None,
            side_px: None,
            framing: FramingSpecUi::default(),
        };
        let result = session.create(&request, &|_: ProgressUi| {}).unwrap();
        assert!(result.meets_limit);
    }

    #[test]
    fn add_discord_gif_returns_invalid_pack_and_sticker_is_accepted() {
        let directory = tempfile::tempdir().unwrap();
        let library = state(&directory);
        let pack = library
            .apply_change(|opened| opened.create_pack("Gatos", None))
            .unwrap()
            .packs[0]
            .id;
        let session = Session::default();
        let id = session.add_files(vec![video(directory.path())])[0].id;

        let not_created = add_created(&library, &session, pack, id).unwrap_err();
        assert_eq!(not_created.kind, UiErrorKind::InvalidEntry);

        create(&session, id, "discord-gif");
        let error = add_created(&library, &session, pack, id).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidPack);
        assert!(
            error
                .message
                .starts_with("no vale como sticker de WhatsApp"),
            "{}",
            error.message
        );

        create(&session, id, "whatsapp-sticker");
        let ui = add_created(&library, &session, pack, id).unwrap();
        assert_eq!(ui.packs[0].stickers.len(), 1);
        assert!(ui.packs[0].icon.is_some());
    }

    #[test]
    fn created_sticker_inherits_media_origin_from_entry() {
        let directory = tempfile::tempdir().unwrap();
        let library = state(&directory);
        let pack = library
            .apply_change(|opened| opened.create_pack("Gatos", None))
            .unwrap()
            .packs[0]
            .id;
        let session = Session::default();
        let id = session.add_files(vec![video(directory.path())])[0].id;
        session.mark_media_origin(id, MediaOrigin::Giphy).unwrap();
        create(&session, id, "whatsapp-sticker");

        let ui = add_created(&library, &session, pack, id).unwrap();

        assert_eq!(ui.packs[0].stickers[0].media_origin, MediaOrigin::Giphy);
    }

    #[test]
    fn export_and_import_with_command_functions() {
        let directory = tempfile::tempdir().unwrap();
        let library = state(&directory);
        let pack = library
            .apply_change(|opened| opened.create_pack("Gatos: los/mejores", None))
            .unwrap()
            .packs[0]
            .id;
        let no_stickers = export_to(&library, pack, &directory.path().join("x"));
        assert_eq!(no_stickers.unwrap_err().kind, UiErrorKind::InvalidPack);
        for tone in [10, 120, 240] {
            library
                .apply_change(|opened| opened.add_sticker(pack, &animated(tone), MediaOrigin::Own))
                .unwrap();
        }

        let name = export_to(&library, pack, &directory.path().join("gatos")).unwrap();

        assert_eq!(name, "gatos.wastickers");
        let bytes = read_wastickers(&directory.path().join(&name)).unwrap();
        let result = import_from(&library, &bytes, &name, &Executables::default()).unwrap();
        assert_eq!(result.library.packs.len(), 2);
        let imported = result
            .library
            .packs
            .iter()
            .find(|p| p.id == result.pack)
            .unwrap();
        assert_eq!(imported.name, "Gatos: los/mejores");
        assert_eq!(imported.stickers.len(), 3);
        assert!(result.notices.is_empty());
    }

    #[test]
    fn import_invalid_returns_invalid_pack() {
        let directory = tempfile::tempdir().unwrap();
        let library = state(&directory);
        let error = import_from(
            &library,
            b"no es un zip",
            "x.wastickers",
            &Executables::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidPack);
        let large = directory.path().join("grande.wastickers");
        fs::File::create(&large)
            .unwrap()
            .set_len(wastickers::MAX_SIZE_BYTES + 1)
            .unwrap();
        assert_eq!(
            read_wastickers(&large).unwrap_err().kind,
            UiErrorKind::InvalidPack
        );
        assert_eq!(
            read_wastickers(directory.path()).unwrap_err().kind,
            UiErrorKind::InvalidPack
        );
        assert!(
            read_wastickers(&directory.path().join("no-existe"))
                .is_err_and(|error| error.kind == UiErrorKind::InvalidPack)
        );
    }

    #[test]
    fn sticker_file_name_has_position_and_safe_pack_name() {
        assert_eq!(sticker_file_name("Gatos", 3), "Gatos-3.webp");
        assert_eq!(sticker_file_name("a/b", 1), "a_b-1.webp");
        assert_eq!(sticker_file_name("..", 2), "paquete-2.webp");
    }

    #[test]
    fn one_sticker_is_saved_as_webp_with_the_picked_name() {
        let directory = tempfile::tempdir().unwrap();
        let name = save_sticker_to(&directory.path().join("mio"), b"RIFFdata").unwrap();
        assert_eq!(name, "mio.webp");
        assert_eq!(
            fs::read(directory.path().join("mio.webp")).unwrap(),
            b"RIFFdata"
        );
    }

    #[test]
    fn proposed_name_has_no_invalid_file_characters() {
        assert_eq!(proposed_name("Gatos"), "Gatos.wastickers");
        assert_eq!(proposed_name("a/b\\c:d*e?"), "a_b_c_d_e_.wastickers");
        assert_eq!(proposed_name(".."), "paquete.wastickers");
        assert_eq!(proposed_name("x\u{7}y"), "x_y.wastickers");
    }
}
