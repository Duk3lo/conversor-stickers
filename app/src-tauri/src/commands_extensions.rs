//! Installing an extension: the user picks a manifest file and Rust validates it.

use std::fs;
use std::io::Read;

use tauri::{AppHandle, Window};
use tauri_plugin_dialog::DialogExt;
use tracing::warn;

use crate::commands::in_background;
use crate::contract::{UiError, UiErrorKind};
use crate::extensions::{self, ExtensionUi};
use crate::mobile;

/// Asks for a `.json` manifest; `None` when the user cancels.
#[tauri::command]
pub(crate) async fn install_extension(
    app: AppHandle,
    window: Window,
) -> Result<Option<ExtensionUi>, UiError> {
    in_background(move || {
        let dialog = app.dialog().file();
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let picked = dialog
            .set_title("Instalar extensión")
            .add_filter("Extensión", &["json"])
            .add_filter("Todos los archivos", &["*"])
            .blocking_pick_file();
        let Some(path) = picked else {
            return Ok(None);
        };
        let path = mobile::local_path(&app, path, extensions::MAX_MANIFEST_BYTES)?;
        let mut bytes = Vec::new();
        let read = fs::File::open(&path).and_then(|file| {
            file.take(extensions::MAX_MANIFEST_BYTES + 1)
                .read_to_end(&mut bytes)
        });
        if cfg!(target_os = "android")
            && let Err(error) = fs::remove_file(&path)
        {
            warn!(%error, "no se pudo borrar la copia de la extensión elegida");
        }
        read.map_err(|error| {
            UiError::new(
                UiErrorKind::InvalidEntry,
                format!("no se pudo leer el archivo: {error}"),
            )
        })?;
        if bytes.len() as u64 > extensions::MAX_MANIFEST_BYTES {
            return Err(UiError::new(
                UiErrorKind::InvalidEntry,
                "extensión no válida: el archivo es demasiado grande",
            ));
        }
        extensions::parse_manifest(&bytes).map(Some)
    })
    .await
}
