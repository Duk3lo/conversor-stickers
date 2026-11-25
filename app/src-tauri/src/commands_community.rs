//! Community commands: thin wrappers that parse, call `community` and return.
//!
//! The network never runs while the library lock is held: the lock is taken only to read local
//! data and, at the end, to add the verified pack.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State, Window};
use tauri_plugin_dialog::DialogExt;
use tracing::warn;

use crate::commands::in_background;
use crate::commands_mobile as mobile_whatsapp;
use crate::commands_packs::LibraryState;
use crate::community::{
    Client, CommunityError, IncomingPack, IncomingSticker, LocalStore, PackToPropose,
    accept_notice, apply_withdrawn, base_url, notice_accepted, propose, today,
};
use crate::contract::{CreatedPack, UiError, UiErrorKind};
use crate::contract_community::{CatalogUi, CommunityDetailUi, CommunityProgressUi};
use crate::library::{
    IncomingPack as CommunityPack, IncomingSticker as AcceptedSticker, Library, LibraryError,
    ValidSticker,
};
use crate::mobile;

pub(crate) struct CommunityState {
    client: Client,
    data: PathBuf,
    downloading: AtomicBool,
    cancel: AtomicBool,
}

impl CommunityState {
    pub(crate) fn new(cache: PathBuf, data: PathBuf) -> Result<Self, CommunityError> {
        Ok(CommunityState {
            client: Client::new(&base_url()?, cache)?,
            data,
            downloading: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
        })
    }
}

impl From<CommunityError> for UiError {
    fn from(error: CommunityError) -> Self {
        let kind = match &error {
            CommunityError::Library(_) => None,
            CommunityError::Cancelled => Some(UiErrorKind::Cancelled),
            CommunityError::InvalidProposal(_) => Some(UiErrorKind::InvalidPack),
            CommunityError::Disk { .. } | CommunityError::InvalidBaseUrl { .. } => {
                Some(UiErrorKind::Other)
            }
            _ => Some(UiErrorKind::FailedDownload),
        };
        match (kind, error) {
            (Some(kind), error) => UiError::new(kind, error.to_string()),
            (None, CommunityError::Library(internal)) => UiError::from(internal),
            (None, other) => UiError::new(UiErrorKind::Other, other.to_string()),
        }
    }
}

impl LocalStore for Library {
    fn community_ids(&self) -> Vec<(u32, String, u32)> {
        Library::community_ids(self)
    }

    fn add_from_community(&mut self, pack: IncomingPack) -> Result<u32, LibraryError> {
        let stickers = pack
            .stickers
            .into_iter()
            .map(|sticker| {
                Ok(AcceptedSticker {
                    sticker: ValidSticker::new(sticker.bytes)
                        .map_err(LibraryError::InvalidSticker)?,
                    emojis: sticker.emojis,
                })
            })
            .collect::<Result<Vec<_>, LibraryError>>()?;
        Library::add_from_community(
            self,
            CommunityPack {
                id: pack.id,
                version: pack.version,
                name: pack.name,
                original_author: pack.author.clone(),
                author: pack.author,
                license: pack.license,
                icon: Some(pack.icon),
                stickers,
            },
        )
    }

    fn mark_withdrawn(&mut self, withdrawn: &[(String, String)]) -> Result<Vec<u32>, LibraryError> {
        Library::mark_withdrawn(self, withdrawn)
    }

    fn pack_to_propose(&self, id: u32) -> Result<PackToPropose, LibraryError> {
        self.check_proposable(id)?;
        let pack = self.to_export(id)?;
        Ok(PackToPropose {
            name: pack.title,
            author: pack.author,
            icon: pack.icon,
            stickers: pack
                .stickers
                .into_iter()
                .zip(pack.emojis)
                .map(|(bytes, emojis)| IncomingSticker { bytes, emojis })
                .collect(),
        })
    }
}

fn locals(library: &LibraryState) -> Result<Vec<(u32, String, u32)>, UiError> {
    library.with(|opened| Ok(opened.community_ids()))
}

#[tauri::command]
pub(crate) async fn catalog(
    force: bool,
    app: AppHandle,
    community: State<'_, Arc<CommunityState>>,
    library: State<'_, Arc<LibraryState>>,
) -> Result<CatalogUi, UiError> {
    let community = Arc::clone(&community);
    let library = Arc::clone(&library);
    in_background(move || {
        let read = community.client.index(force)?;
        let (withdrawn, in_library) = library.with(|opened| {
            let withdrawn = apply_withdrawn(&read.index, opened)?;
            Ok((withdrawn, opened.community_ids()))
        })?;
        if !withdrawn.is_empty() {
            warn!(
                packs = withdrawn.len(),
                "paquetes locales retirados del catálogo"
            );
            if cfg!(target_os = "android") {
                match app.path().app_data_dir() {
                    Ok(data) => mobile_whatsapp::prune_backups(&data, &library),
                    Err(error) => warn!(%error, "no se pudo podar la copia de WhatsApp"),
                }
            }
        }
        let accepted = notice_accepted(&community.data);
        Ok(community.client.catalog(&read, &in_library, accepted))
    })
    .await
}

#[tauri::command]
pub(crate) async fn community_detail(
    id: String,
    community: State<'_, Arc<CommunityState>>,
    library: State<'_, Arc<LibraryState>>,
) -> Result<CommunityDetailUi, UiError> {
    let community = Arc::clone(&community);
    let library = Arc::clone(&library);
    in_background(move || {
        let in_library = locals(&library)?;
        Ok(community.client.detail(&id, &in_library)?)
    })
    .await
}

#[tauri::command]
pub(crate) async fn accept_community_notice(
    community: State<'_, Arc<CommunityState>>,
) -> Result<(), UiError> {
    let community = Arc::clone(&community);
    in_background(move || Ok(accept_notice(&community.data)?)).await
}

#[tauri::command]
pub(crate) async fn download_from_community(
    id: String,
    channel: Channel<CommunityProgressUi>,
    community: State<'_, Arc<CommunityState>>,
    library: State<'_, Arc<LibraryState>>,
) -> Result<CreatedPack, UiError> {
    let community = Arc::clone(&community);
    let library = Arc::clone(&library);
    in_background(move || {
        if !notice_accepted(&community.data) {
            return Err(UiError::new(
                UiErrorKind::Other,
                "Antes de descargar hay que aceptar el aviso de la comunidad",
            ));
        }
        if community.downloading.swap(true, Ordering::AcqRel) {
            return Err(UiError::new(
                UiErrorKind::Busy,
                "Ya hay una descarga en curso",
            ));
        }
        community.cancel.store(false, Ordering::Relaxed);
        let _release = ReleaseDownload(&community.downloading);
        download(&id, &channel, &community, &library)
    })
    .await
}

struct ReleaseDownload<'a>(&'a AtomicBool);

impl Drop for ReleaseDownload<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn download(
    id: &str,
    channel: &Channel<CommunityProgressUi>,
    community: &CommunityState,
    library: &LibraryState,
) -> Result<CreatedPack, UiError> {
    let mut notify = |progress| {
        if let Err(error) = channel.send(progress) {
            warn!(%error, "no se pudo enviar el progreso de la comunidad");
        }
    };
    let incoming = community
        .client
        .download(id, &community.cancel, &mut notify)?;
    library.with(|opened| {
        let pack = LocalStore::add_from_community(opened, incoming)?;
        Ok(CreatedPack {
            library: opened.ui(),
            pack,
        })
    })
}

#[tauri::command]
pub(crate) fn cancel_community_download(community: State<'_, Arc<CommunityState>>) {
    community.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub(crate) async fn propose_to_community(
    pack: u32,
    license: String,
    category: String,
    app: AppHandle,
    window: Window,
    community: State<'_, Arc<CommunityState>>,
    library: State<'_, Arc<LibraryState>>,
) -> Result<Option<String>, UiError> {
    let community = Arc::clone(&community);
    let library = Arc::clone(&library);
    in_background(move || {
        let local = library.with(|opened| opened.pack_to_propose(pack))?;
        let categories = community.client.categories()?;
        let existing = community.client.catalog_ids();
        let zip = propose(
            &local,
            &license,
            &category,
            &categories,
            &existing,
            &today(),
        )?;

        let dialog = app.dialog().file();
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let Some(picked) = dialog
            .set_title("Guardar la propuesta para la comunidad")
            .set_file_name("community-proposal.zip")
            .add_filter("Zip", &["zip"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let name = mobile::picked_name(&app, &picked);
        mobile::write_to(&app, picked, &zip)?;
        Ok(Some(name))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_releases_download_on_drop_even_on_panic() {
        let downloading = AtomicBool::new(true);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ReleaseDownload(&downloading);
            panic!("fallo simulado en la descarga");
        }));
        assert!(result.is_err());
        assert!(!downloading.load(Ordering::Acquire));
    }
}
