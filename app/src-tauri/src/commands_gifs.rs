//! GIF search and KLIPY sticker commands: thin wrappers that validate what arrives over IPC and
//! call `gifs`. Network and disk work runs in `spawn_blocking`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tauri::{AppHandle, State};
use tracing::warn;

use crate::commands::{in_background, prepare_executables};
use crate::commands_packs::LibraryState;
use crate::contract::{CreatedPack, EntryUi, MediaOrigin, UiError, UiErrorKind};
use crate::contract_gifs::{
    GifProviderUi, GifResultUi, GifsPageUi, KlipyCategoryUi, KlipyStickerUi, StickersPageUi,
};
use crate::external_sticker::to_whatsapp_sticker;
use crate::gif_providers::{Credential, Provider, Providers};
use crate::gifs::{
    GifError, GifProvider, GifResult, Giphy, Klipy, StickerResult, media_in_cache, prune_cache,
    with_thumbnails,
};
use crate::session::Session;

const CACHE_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

pub(crate) struct GifsState {
    providers: Providers,
    cache: PathBuf,
    last: Mutex<HashMap<String, GifResult>>,
    last_stickers: Mutex<HashMap<String, StickerResult>>,
}

fn klipy_client(credential: &Credential) -> Result<Klipy, GifError> {
    match credential {
        Credential::Key(key) => Klipy::new(key),
        Credential::Proxy(proxy) => Klipy::through_proxy(proxy),
    }
}

impl GifsState {
    pub(crate) fn new(cache: PathBuf, config: PathBuf) -> Self {
        if let Err(error) = prune_cache(&cache, CACHE_MAX_AGE) {
            warn!(%error, "no se pudo limpiar la caché de GIF");
        }
        GifsState {
            providers: Providers::new(config),
            cache,
            last: Mutex::default(),
            last_stickers: Mutex::default(),
        }
    }

    fn provider(&self) -> Result<(Provider, Box<dyn GifProvider>), GifError> {
        let (provider, credential) = self.providers.active()?;
        let client: Box<dyn GifProvider> = match (provider, credential) {
            (Provider::Klipy, credential) => Box::new(klipy_client(&credential)?),
            (Provider::Giphy, Credential::Key(key)) => Box::new(Giphy::new(&key)?),
            // The proxy only serves KLIPY.
            (Provider::Giphy, Credential::Proxy(_)) => {
                return Err(GifError::MissingKey { provider: "GIPHY" });
            }
        };
        Ok((provider, client))
    }

    fn find(&self, text: Option<&str>, page: u32) -> Result<GifsPageUi, GifError> {
        let (_, provider) = self.provider()?;
        let found = match text {
            Some(text) => provider.find(text, page)?,
            None => provider.trending(page)?,
        };
        let done = with_thumbnails(provider.as_ref(), &self.cache, found.results);
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if page <= 1 {
            last.clear();
        }
        let results = done
            .into_iter()
            .map(|(result, thumbnail)| {
                let ui = GifResultUi {
                    id: result.id.clone(),
                    title: result.title.clone(),
                    thumbnail: thumbnail.to_string_lossy().into_owned(),
                    width: result.width,
                    height: result.height,
                };
                last.insert(result.id.clone(), result);
                ui
            })
            .collect();
        Ok(GifsPageUi {
            results,
            next: found.next,
            attribution: provider.attribution().to_owned(),
        })
    }

    fn fetch(&self, id: &str) -> Result<(PathBuf, MediaOrigin), GifError> {
        let result = self
            .last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
            .ok_or(GifError::UnknownGif)?;
        let (provider, client) = self.provider()?;
        let origin = match provider {
            Provider::Klipy => MediaOrigin::Klipy,
            Provider::Giphy => MediaOrigin::Giphy,
        };
        let path = media_in_cache(client.as_ref(), &self.cache, &result)?;
        Ok((path, origin))
    }

    fn sticker_cache(&self) -> PathBuf {
        self.cache.join("stickers")
    }

    fn klipy(&self) -> Result<Klipy, GifError> {
        klipy_client(&self.providers.klipy_access()?)
    }

    fn stickers(
        &self,
        text: Option<&str>,
        category: Option<&str>,
        page: u32,
    ) -> Result<StickersPageUi, GifError> {
        let klipy = self.klipy()?;
        let found = klipy.stickers(text.or(category), page)?;
        let results: Vec<GifResult> = found
            .results
            .iter()
            .map(|sticker| sticker.result.clone())
            .collect();
        let done = with_thumbnails(&klipy, &self.sticker_cache(), results);
        let mut by_id: HashMap<String, StickerResult> = found
            .results
            .into_iter()
            .map(|sticker| (sticker.result.id.clone(), sticker))
            .collect();
        let mut last = self
            .last_stickers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if page <= 1 {
            last.clear();
        }
        let results = done
            .into_iter()
            .filter_map(|(result, thumbnail)| {
                let sticker = by_id.remove(&result.id)?;
                let ui = KlipyStickerUi {
                    id: result.id.clone(),
                    title: result.title,
                    thumbnail: thumbnail.to_string_lossy().into_owned(),
                    animated: sticker.animated,
                };
                last.insert(result.id, sticker);
                Some(ui)
            })
            .collect();
        Ok(StickersPageUi {
            results,
            next: found.next,
        })
    }

    fn categories(&self) -> Result<Vec<KlipyCategoryUi>, GifError> {
        Ok(self
            .klipy()?
            .sticker_categories()?
            .into_iter()
            .map(|category| KlipyCategoryUi {
                id: category.id,
                name: category.name,
            })
            .collect())
    }

    fn fetch_sticker(&self, id: &str) -> Result<(PathBuf, bool), GifError> {
        let sticker = self
            .last_stickers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
            .ok_or(GifError::UnknownSticker)?;
        let path = media_in_cache(&self.klipy()?, &self.sticker_cache(), &sticker.result)?;
        Ok((path, sticker.animated))
    }
}

enum Destination {
    Existing(u32),
    New(String),
}

fn destination_of(pack: Option<u32>, new_name: Option<String>) -> Result<Destination, UiError> {
    match (pack, new_name) {
        (Some(pack), None) => Ok(Destination::Existing(pack)),
        (None, Some(name)) => Ok(Destination::New(name)),
        _ => Err(UiError::new(
            UiErrorKind::InvalidPack,
            "elige un paquete o escribe el nombre de uno nuevo, no los dos",
        )),
    }
}

fn save_to_pack(
    library: &LibraryState,
    bytes: &[u8],
    destination: &Destination,
) -> Result<CreatedPack, UiError> {
    library.with(|opened| {
        let pack = match destination {
            Destination::Existing(pack) => *pack,
            Destination::New(name) => opened.create_pack(name, None)?,
        };
        opened.add_sticker(pack, bytes, MediaOrigin::Klipy)?;
        Ok(CreatedPack {
            library: opened.ui(),
            pack,
        })
    })
}

impl From<GifError> for UiError {
    fn from(error: GifError) -> Self {
        let kind = match error {
            GifError::Disk { .. } => UiErrorKind::Other,
            _ => UiErrorKind::FailedDownload,
        };
        UiError::new(kind, error.to_string())
    }
}

#[tauri::command]
pub(crate) fn gif_provider(gifs: State<'_, Arc<GifsState>>) -> Result<GifProviderUi, UiError> {
    let state = gifs.providers.state()?;
    Ok(GifProviderUi {
        provider: state.provider,
        key_set: state.key_set,
        key_origin: state.key_origin,
    })
}

#[tauri::command]
pub(crate) fn configure_gif_provider(
    provider: Provider,
    key: Option<String>,
    gifs: State<'_, Arc<GifsState>>,
) -> Result<(), UiError> {
    Ok(gifs.providers.configure(provider, key.as_deref())?)
}

#[tauri::command]
pub(crate) async fn search_gifs(
    text: Option<String>,
    page: u32,
    gifs: State<'_, Arc<GifsState>>,
) -> Result<GifsPageUi, UiError> {
    let gifs = Arc::clone(&gifs);
    in_background(move || Ok(gifs.find(text.as_deref(), page)?)).await
}

#[tauri::command]
pub(crate) async fn use_gif(
    id: String,
    app: AppHandle,
    gifs: State<'_, Arc<GifsState>>,
    session: State<'_, Arc<Session>>,
) -> Result<EntryUi, UiError> {
    let gifs = Arc::clone(&gifs);
    let session = Arc::clone(&session);
    in_background(move || {
        let (path, origin) = gifs.fetch(&id)?;
        prepare_executables(&app, &session)?;
        let entry = session
            .add_files(vec![path])
            .into_iter()
            .next()
            .ok_or_else(|| {
                UiError::new(UiErrorKind::Other, "la sesión no devolvió ninguna entrada")
            })?;
        session.mark_media_origin(entry.id, origin)?;
        Ok(entry)
    })
    .await
}

#[tauri::command]
pub(crate) async fn klipy_stickers(
    text: Option<String>,
    category: Option<String>,
    page: u32,
    gifs: State<'_, Arc<GifsState>>,
) -> Result<StickersPageUi, UiError> {
    let gifs = Arc::clone(&gifs);
    in_background(move || Ok(gifs.stickers(text.as_deref(), category.as_deref(), page)?)).await
}

#[tauri::command]
pub(crate) async fn klipy_categories(
    gifs: State<'_, Arc<GifsState>>,
) -> Result<Vec<KlipyCategoryUi>, UiError> {
    let gifs = Arc::clone(&gifs);
    in_background(move || Ok(gifs.categories()?)).await
}

#[tauri::command]
pub(crate) async fn add_klipy_sticker(
    id: String,
    pack: Option<u32>,
    new_name: Option<String>,
    app: AppHandle,
    gifs: State<'_, Arc<GifsState>>,
    session: State<'_, Arc<Session>>,
    library: State<'_, Arc<LibraryState>>,
) -> Result<CreatedPack, UiError> {
    let gifs = Arc::clone(&gifs);
    let session = Arc::clone(&session);
    let library = Arc::clone(&library);
    in_background(move || {
        let destination = destination_of(pack, new_name)?;
        let (path, animated) = gifs.fetch_sticker(&id)?;
        prepare_executables(&app, &session)?;
        let bytes = to_whatsapp_sticker(
            &path,
            animated,
            &session.executables(),
            &AtomicBool::new(false),
        )?;
        save_to_pack(&library, &bytes, &destination)
    })
    .await
}

#[cfg(test)]
mod tests {
    use crate::library::tests::animated;

    use super::*;

    fn library() -> (tempfile::TempDir, LibraryState) {
        let directory = tempfile::tempdir().unwrap();
        let state = LibraryState::new(directory.path().join("library"));
        (directory, state)
    }

    #[test]
    fn destination_is_a_pack_or_a_new_name_but_not_both_nor_neither() {
        assert!(matches!(
            destination_of(Some(3), None),
            Ok(Destination::Existing(3))
        ));
        assert!(matches!(
            destination_of(None, Some("Nuevo".to_owned())),
            Ok(Destination::New(_))
        ));
        assert!(destination_of(None, None).is_err());
        assert!(destination_of(Some(3), Some("Nuevo".to_owned())).is_err());
    }

    #[test]
    fn sticker_goes_to_new_pack_then_to_that_same_pack() {
        let (_directory, state) = library();
        let created = save_to_pack(
            &state,
            &animated(10),
            &Destination::New("De KLIPY".to_owned()),
        )
        .unwrap();
        let ui = created.library.packs.iter().find(|p| p.id == created.pack);
        assert_eq!(ui.unwrap().stickers.len(), 1);

        let other =
            save_to_pack(&state, &animated(20), &Destination::Existing(created.pack)).unwrap();
        assert_eq!(other.pack, created.pack);
        let ui = other.library.packs.iter().find(|p| p.id == created.pack);
        assert_eq!(ui.unwrap().stickers.len(), 2);
    }

    #[test]
    fn missing_pack_or_empty_name_is_an_error() {
        let (_directory, state) = library();
        assert!(save_to_pack(&state, &animated(10), &Destination::Existing(99)).is_err());
        assert!(save_to_pack(&state, &animated(10), &Destination::New("  ".to_owned())).is_err());
    }
}
