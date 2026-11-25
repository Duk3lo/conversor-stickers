//! Community catalog client, without Tauri: downloads the index and the packs from a static site,
//! verifies them and hands them to the library through [`LocalStore`]. Network data is untrusted:
//! sizes are limited while streaming and nothing is accepted before checking its size, its SHA-256
//! and the core WebP validator.

use std::fs;
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use conversor_core::{
    CatalogError, Category, ICON_FILE, Index, IndexPack, MAX_ICON_SIZE_BYTES, MAX_INDEX_SIZE_BYTES,
    MAX_STICKER_SIZE_BYTES, PACK_FILE, PackSource, StickerError, StickerKind, StickerSource,
    inspect_tray_icon, inspect_whatsapp_sticker, is_valid_id, read_index, sha256_hex,
    validate_source_pack,
};
use thiserror::Error;
use tracing::warn;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::contract_community::{
    CatalogUi, CommunityCategoryUi, CommunityDetailUi, CommunityPackUi, CommunityProgressUi,
    SampleUi,
};
use crate::library::LibraryError;
use crate::storage::write_atomic;

const DEFAULT_BASE_URL: &str = "https://duk3lo.github.io/conversor-stickers-comunidad/";
const TIME_LIMIT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 3;
const CACHE_FRESHNESS_S: u64 = 600;
const ICONS_PER_CALL: usize = 60;
const PROPOSAL_INSTRUCTIONS: &str =
    "https://github.com/Duk3lo/conversor-stickers-comunidad/blob/main/CONTRIBUTING.md";

#[derive(Debug, Error)]
pub(crate) enum CommunityError {
    #[error("La dirección del catálogo no es válida: {url}")]
    InvalidBaseUrl { url: String },

    #[error("Sin conexión y sin catálogo guardado")]
    OfflineWithoutCache,

    #[error("No se pudo conectar con el catálogo: {reason}")]
    Network { reason: String },

    #[error("El catálogo respondió {state} a {url}")]
    Http { state: u16, url: String },

    #[error("El catálogo redirige a otro sitio y no se sigue")]
    DisallowedRedirect,

    #[error("{file} supera el tamaño máximo permitido")]
    TooLarge { file: String },

    #[error("Actualiza la app para ver el catálogo")]
    NewFormat,

    #[error("El catálogo no es válido: {0}")]
    Catalog(#[source] CatalogError),

    #[error("No hay ningún paquete {id} en el catálogo")]
    MissingPack { id: String },

    #[error("Este paquete se retiró del catálogo")]
    WithdrawnPack { id: String },

    #[error("El archivo {file} no coincide con su suma: descarga cancelada")]
    WrongChecksum { file: String },

    #[error("El archivo {file} no tiene el tamaño que dice el catálogo: descarga cancelada")]
    IncorrectSize { file: String },

    #[error("El archivo {file} no es válido: {source}")]
    InvalidFile {
        file: String,
        #[source]
        source: StickerError,
    },

    #[error("El sticker {file} es {real} y el paquete dice que es {declared}")]
    DifferentKind {
        file: String,
        real: &'static str,
        declared: &'static str,
    },

    #[error("Descarga cancelada")]
    Cancelled,

    #[error("No se puede proponer el paquete: {0}")]
    InvalidProposal(String),

    #[error("No se pudo {action}: {source}")]
    Disk {
        action: &'static str,
        #[source]
        source: io::Error,
    },

    #[error(transparent)]
    Library(#[from] LibraryError),
}

impl CommunityError {
    fn is_connection(&self) -> bool {
        matches!(self, Self::Network { .. })
            || matches!(self, Self::Http { state, .. } if *state >= 500)
    }
}

impl From<CatalogError> for CommunityError {
    fn from(error: CatalogError) -> Self {
        match error {
            CatalogError::FormatNotSupported { .. } => Self::NewFormat,
            other => Self::Catalog(other),
        }
    }
}

fn disk(action: &'static str) -> impl FnOnce(io::Error) -> CommunityError {
    move |source| CommunityError::Disk { action, source }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncomingPack {
    pub(crate) id: String,
    pub(crate) version: u32,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) license: String,
    pub(crate) icon: Vec<u8>,
    pub(crate) stickers: Vec<IncomingSticker>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncomingSticker {
    pub(crate) bytes: Vec<u8>,
    pub(crate) emojis: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackToPropose {
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) icon: Vec<u8>,
    pub(crate) stickers: Vec<IncomingSticker>,
}

pub(crate) trait LocalStore {
    fn community_ids(&self) -> Vec<(u32, String, u32)>;

    fn add_from_community(&mut self, pack: IncomingPack) -> Result<u32, LibraryError>;

    fn mark_withdrawn(&mut self, withdrawn: &[(String, String)]) -> Result<Vec<u32>, LibraryError>;

    fn pack_to_propose(&self, id: u32) -> Result<PackToPropose, LibraryError>;
}

pub(crate) fn apply_withdrawn(
    index: &Index,
    store: &mut dyn LocalStore,
) -> Result<Vec<u32>, LibraryError> {
    let locals = store.community_ids();
    let affected: Vec<(String, String)> = index
        .withdrawn
        .iter()
        .filter(|withdrawn| locals.iter().any(|(_, id, _)| *id == withdrawn.id))
        .map(|withdrawn| (withdrawn.id.clone(), withdrawn.reason.clone()))
        .collect();
    if affected.is_empty() {
        return Ok(Vec::new());
    }
    store.mark_withdrawn(&affected)
}

fn allowed_url(url: &str) -> bool {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    if url
        .strip_prefix("https://")
        .is_some_and(|rest| !rest.is_empty())
    {
        return true;
    }
    cfg!(test) && url.starts_with("http://127.0.0.1")
}

pub(crate) fn validate_base_url(url: &str) -> Result<String, CommunityError> {
    if !allowed_url(url) {
        return Err(CommunityError::InvalidBaseUrl {
            url: url.to_owned(),
        });
    }
    let mut base = url.to_owned();
    if !base.ends_with('/') {
        base.push('/');
    }
    Ok(base)
}

pub(crate) fn base_url() -> Result<String, CommunityError> {
    validate_base_url(option_env!("CONVERSOR_CATALOGO_URL").unwrap_or(DEFAULT_BASE_URL))
}

fn origin(url: &str) -> Option<&str> {
    let start = url.find("://")? + 3;
    let end = url[start..]
        .find('/')
        .map_or(url.len(), |position| start + position);
    Some(&url[..end])
}

fn resolve_redirect(current: &str, destination: &str) -> Option<String> {
    if destination.starts_with('/') && !destination.starts_with("//") {
        return Some(format!("{}{destination}", origin(current)?));
    }
    (origin(destination)? == origin(current)?).then(|| destination.to_owned())
}

struct Response {
    body: Option<Vec<u8>>,
    etag: Option<String>,
}

pub(crate) struct Client {
    base: String,
    cache: PathBuf,
    agent: ureq::Agent,
}

#[derive(Debug)]
pub(crate) struct ReadIndex {
    pub(crate) index: Index,
    pub(crate) updated: u64,
    pub(crate) offline: bool,
}

impl Client {
    pub(crate) fn new(base: &str, cache: PathBuf) -> Result<Self, CommunityError> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIME_LIMIT))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Client {
            base: validate_base_url(base)?,
            cache,
            agent,
        })
    }

    fn fetch_text(
        &self,
        url: &str,
        max: u64,
        etag: Option<&str>,
    ) -> Result<Response, CommunityError> {
        let mut current = url.to_owned();
        for _ in 0..=MAX_REDIRECTS {
            let mut request = self.agent.get(&current);
            if let Some(etag) = etag {
                request = request.header("If-None-Match", etag);
            }
            let mut response = request.call().map_err(|error| CommunityError::Network {
                reason: error.to_string(),
            })?;
            let state = response.status().as_u16();
            match state {
                304 => {
                    return Ok(Response {
                        body: None,
                        etag: None,
                    });
                }
                301 | 302 | 303 | 307 | 308 => {
                    let destination = response
                        .headers()
                        .get("location")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|destination| resolve_redirect(&current, destination))
                        .filter(|destination| allowed_url(destination))
                        .ok_or(CommunityError::DisallowedRedirect)?;
                    current = destination;
                    continue;
                }
                200..=299 => {}
                _ => {
                    return Err(CommunityError::Http {
                        state,
                        url: current,
                    });
                }
            }
            let too_much = || CommunityError::TooLarge {
                file: current.clone(),
            };
            let declared = response
                .headers()
                .get("content-length")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            if declared.is_some_and(|bytes| bytes > max) {
                return Err(too_much());
            }
            let etag = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let body = response
                .body_mut()
                .with_config()
                .limit(max)
                .read_to_vec()
                .map_err(|error| match error {
                    ureq::Error::BodyExceedsLimit(_) => too_much(),
                    other => CommunityError::Network {
                        reason: other.to_string(),
                    },
                })?;
            return Ok(Response {
                body: Some(body),
                etag,
            });
        }
        Err(CommunityError::DisallowedRedirect)
    }

    fn index_path(&self) -> PathBuf {
        self.cache.join("index.json")
    }

    fn etag_path(&self) -> PathBuf {
        self.cache.join("index.etag")
    }

    fn cached_index(&self) -> Option<(Index, u64)> {
        let path = self.index_path();
        let bytes = fs::read(&path).ok()?;
        let index = match read_index(&bytes) {
            Ok(index) => index,
            Err(error) => {
                warn!(%error, "el índice guardado no es válido y se ignora");
                return None;
            }
        };
        let seconds = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|date| date.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs());
        Some((index, seconds))
    }

    pub(crate) fn index(&self, force: bool) -> Result<ReadIndex, CommunityError> {
        let in_cache = self.cached_index();
        if !force
            && let Some((index, updated)) = &in_cache
            && now_unix().saturating_sub(*updated) < CACHE_FRESHNESS_S
        {
            return Ok(ReadIndex {
                index: index.clone(),
                updated: *updated,
                offline: false,
            });
        }
        let etag = if force || in_cache.is_none() {
            None
        } else {
            fs::read_to_string(self.etag_path()).ok()
        };
        let url = format!("{}index.json", self.base);
        match self.fetch_text(&url, MAX_INDEX_SIZE_BYTES, etag.as_deref()) {
            Ok(Response {
                body: Some(bytes),
                etag,
            }) => {
                let index = read_index(&bytes)?;
                fs::create_dir_all(&self.cache).map_err(disk("crear la caché"))?;
                write_atomic(&self.index_path(), &bytes).map_err(disk("guardar el índice"))?;
                match etag {
                    Some(etag) => write_atomic(&self.etag_path(), etag.as_bytes())
                        .map_err(disk("guardar el ETag"))?,
                    None => delete_if_exists(&self.etag_path())?,
                }
                Ok(ReadIndex {
                    index,
                    updated: now_unix(),
                    offline: false,
                })
            }
            Ok(Response { body: None, .. }) => {
                let (index, _) = in_cache.ok_or(CommunityError::Network {
                    reason: "el servidor dijo 304 sin que hubiera caché".to_owned(),
                })?;
                let bytes = fs::read(self.index_path()).map_err(disk("leer el índice"))?;
                write_atomic(&self.index_path(), &bytes).map_err(disk("guardar el índice"))?;
                Ok(ReadIndex {
                    index,
                    updated: now_unix(),
                    offline: false,
                })
            }
            Err(error) if error.is_connection() => match in_cache {
                Some((index, updated)) => Ok(ReadIndex {
                    index,
                    updated,
                    offline: true,
                }),
                None => Err(CommunityError::OfflineWithoutCache),
            },
            Err(error) => Err(error),
        }
    }

    fn fetch(
        &self,
        pack: &str,
        file: &str,
        bytes: u64,
        sha256: &str,
        max: u64,
    ) -> Result<Vec<u8>, CommunityError> {
        let url = format!("{}packs/{pack}/{file}", self.base);
        let body = self
            .fetch_text(&url, max, None)?
            .body
            .ok_or_else(|| CommunityError::Http {
                state: 304,
                url: url.clone(),
            })?;
        if body.len() as u64 != bytes {
            return Err(CommunityError::IncorrectSize {
                file: file.to_owned(),
            });
        }
        if sha256_hex(&body) != sha256 {
            return Err(CommunityError::WrongChecksum {
                file: file.to_owned(),
            });
        }
        Ok(body)
    }

    fn cache_path(&self, sha256: &str, extension: &str) -> PathBuf {
        self.cache
            .join("archivos")
            .join(format!("{sha256}.{extension}"))
    }

    fn save_to_cache(
        &self,
        sha256: &str,
        extension: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, CommunityError> {
        let path = self.cache_path(sha256, extension);
        if !path.is_file() {
            fs::create_dir_all(self.cache.join("archivos")).map_err(disk("crear la caché"))?;
            write_atomic(&path, bytes).map_err(disk("guardar en la caché"))?;
        }
        Ok(path)
    }

    fn icon_in_cache(&self, pack: &IndexPack) -> Option<PathBuf> {
        let path = self.cache_path(&pack.icon.sha256, "png");
        path.is_file().then_some(path)
    }

    fn fetch_icon(&self, pack: &IndexPack) -> Result<PathBuf, CommunityError> {
        let icon = &pack.icon;
        let bytes = self.fetch(
            &pack.id,
            &icon.file,
            icon.bytes,
            &icon.sha256,
            MAX_ICON_SIZE_BYTES,
        )?;
        inspect_tray_icon(&bytes).map_err(|source| CommunityError::InvalidFile {
            file: icon.file.clone(),
            source,
        })?;
        self.save_to_cache(&icon.sha256, "png", &bytes)
    }

    pub(crate) fn catalog(
        &self,
        read: &ReadIndex,
        locals: &[(u32, String, u32)],
        notice_accepted: bool,
    ) -> CatalogUi {
        let mut to_fetch = ICONS_PER_CALL;
        let mut connection = !read.offline;
        let mut packs = Vec::with_capacity(read.index.packs.len());
        for pack in &read.index.packs {
            let mut icon = self.icon_in_cache(pack);
            if icon.is_none() && connection && to_fetch > 0 {
                to_fetch -= 1;
                match self.fetch_icon(pack) {
                    Ok(path) => icon = Some(path),
                    Err(error) => {
                        warn!(pack = %pack.id, %error, "no se pudo bajar el icono");
                        connection = !error.is_connection();
                    }
                }
            }
            packs.push(pack_ui(pack, icon.as_deref(), locals));
        }
        let categories = read
            .index
            .categories
            .iter()
            .map(|category| CommunityCategoryUi {
                id: category.id.clone(),
                name: category.name.clone(),
            })
            .collect();
        CatalogUi {
            categories,
            packs,
            updated: read.updated,
            offline: read.offline,
            notice_accepted,
        }
    }

    pub(crate) fn detail(
        &self,
        id: &str,
        locals: &[(u32, String, u32)],
    ) -> Result<CommunityDetailUi, CommunityError> {
        let ReadIndex { index, .. } = self.index(false)?;
        let pack = find(&index, id)?;
        let icon = match self.icon_in_cache(pack) {
            Some(path) => Some(path),
            None => self.fetch_icon(pack).ok(),
        };
        let mut stickers = Vec::with_capacity(pack.stickers.len());
        for sticker in &pack.stickers {
            let path = match self.cache_path(&sticker.sha256, "webp") {
                path if path.is_file() => path,
                _ => {
                    let bytes = self.fetch_sticker(pack, &sticker.file)?;
                    self.save_to_cache(&sticker.sha256, "webp", &bytes)?
                }
            };
            stickers.push(SampleUi {
                path: path.to_string_lossy().into_owned(),
                emojis: sticker.emojis.clone(),
            });
        }
        Ok(CommunityDetailUi {
            pack: pack_ui(pack, icon.as_deref(), locals),
            stickers,
        })
    }

    fn fetch_sticker(&self, pack: &IndexPack, file: &str) -> Result<Vec<u8>, CommunityError> {
        let sticker = pack
            .stickers
            .iter()
            .find(|sticker| sticker.file == file)
            .ok_or_else(|| CommunityError::MissingPack {
                id: pack.id.clone(),
            })?;
        let bytes = self.fetch(
            &pack.id,
            file,
            sticker.bytes,
            &sticker.sha256,
            MAX_STICKER_SIZE_BYTES,
        )?;
        check_sticker(file, &bytes, pack.animated)?;
        Ok(bytes)
    }

    pub(crate) fn download(
        &self,
        id: &str,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(CommunityProgressUi),
    ) -> Result<IncomingPack, CommunityError> {
        let ReadIndex { index, .. } = self.index(false)?;
        if index.withdrawn.iter().any(|withdrawn| withdrawn.id == id) {
            return Err(CommunityError::WithdrawnPack { id: id.to_owned() });
        }
        let pack = find(&index, id)?;
        let total = pack.stickers.len() as u32 + 1;
        progress(CommunityProgressUi::Downloading { done: 0, total });

        let mut stickers = Vec::with_capacity(pack.stickers.len());
        for (done, sticker) in pack.stickers.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(CommunityError::Cancelled);
            }
            let bytes = self.fetch_sticker(pack, &sticker.file)?;
            stickers.push(IncomingSticker {
                bytes,
                emojis: sticker.emojis.clone(),
            });
            progress(CommunityProgressUi::Downloading {
                done: done as u32 + 1,
                total,
            });
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(CommunityError::Cancelled);
        }
        let icon = self.fetch(
            &pack.id,
            &pack.icon.file,
            pack.icon.bytes,
            &pack.icon.sha256,
            MAX_ICON_SIZE_BYTES,
        )?;
        progress(CommunityProgressUi::Verifying);
        inspect_tray_icon(&icon).map_err(|source| CommunityError::InvalidFile {
            file: pack.icon.file.clone(),
            source,
        })?;
        Ok(IncomingPack {
            id: pack.id.clone(),
            version: pack.version,
            name: pack.name.clone(),
            author: pack.author.clone(),
            license: pack.license.clone(),
            icon,
            stickers,
        })
    }

    pub(crate) fn categories(&self) -> Result<Vec<Category>, CommunityError> {
        Ok(self.index(false)?.index.categories)
    }

    pub(crate) fn catalog_ids(&self) -> Vec<String> {
        self.cached_index()
            .map(|(index, _)| index.packs.into_iter().map(|p| p.id).collect())
            .unwrap_or_default()
    }
}

fn find<'a>(index: &'a Index, id: &str) -> Result<&'a IndexPack, CommunityError> {
    index
        .packs
        .iter()
        .find(|pack| pack.id == id)
        .ok_or_else(|| CommunityError::MissingPack { id: id.to_owned() })
}

fn text_kind(animated: bool) -> &'static str {
    if animated { "animado" } else { "estático" }
}

fn check_sticker(file: &str, bytes: &[u8], animated: bool) -> Result<(), CommunityError> {
    let sticker =
        inspect_whatsapp_sticker(bytes).map_err(|source| CommunityError::InvalidFile {
            file: file.to_owned(),
            source,
        })?;
    let real = sticker.kind == StickerKind::Animated;
    if real == animated {
        Ok(())
    } else {
        Err(CommunityError::DifferentKind {
            file: file.to_owned(),
            real: text_kind(real),
            declared: text_kind(animated),
        })
    }
}

fn pack_ui(
    pack: &IndexPack,
    icon: Option<&Path>,
    locals: &[(u32, String, u32)],
) -> CommunityPackUi {
    let local = locals.iter().find(|(_, id, _)| *id == pack.id);
    CommunityPackUi {
        id: pack.id.clone(),
        version: pack.version,
        name: pack.name.clone(),
        author: pack.author.clone(),
        license: pack.license.clone(),
        category: pack.category.clone(),
        tags: pack.tags.clone(),
        animated: pack.animated,
        added: pack.added.clone(),
        stickers: pack.stickers.len() as u32,
        icon: icon.map(|path| path.to_string_lossy().into_owned()),
        downloaded: local.map(|(local_id, _, _)| *local_id),
        updatable: local.is_some_and(|(_, _, version)| *version < pack.version),
    }
}

fn delete_if_exists(path: &Path) -> Result<(), CommunityError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CommunityError::Disk {
            action: "borrar el ETag",
            source,
        }),
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(crate) fn notice_accepted(directory: &Path) -> bool {
    directory.join("community_accepted").is_file()
}

pub(crate) fn accept_notice(directory: &Path) -> Result<(), CommunityError> {
    fs::create_dir_all(directory).map_err(disk("guardar la aceptación del aviso"))?;
    write_atomic(&directory.join("community_accepted"), b"1")
        .map_err(disk("guardar la aceptación del aviso"))
}

fn id_from(name: &str) -> Option<String> {
    let mut id = String::new();
    for char in name.chars().flat_map(char::to_lowercase) {
        let base = match char {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            other if other.is_ascii_alphanumeric() => other,
            _ => '-',
        };
        if base == '-' && (id.is_empty() || id.ends_with('-')) {
            continue;
        }
        id.push(base);
    }
    id.truncate(64);
    let id = id.trim_end_matches('-').to_owned();
    is_valid_id(&id).then_some(id)
}

fn date_of(days: u64) -> String {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let era_year = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * era_year + era_year / 4 - era_year / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = era_year + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

pub(crate) fn propose(
    pack: &PackToPropose,
    license: &str,
    category: &str,
    categories: &[Category],
    existing_ids: &[String],
    today: &str,
) -> Result<Vec<u8>, CommunityError> {
    let id = id_from(&pack.name).ok_or_else(|| {
        CommunityError::InvalidProposal(
            "el nombre del paquete no sirve para un identificador (3 letras o números como mínimo)"
                .to_owned(),
        )
    })?;
    if existing_ids.contains(&id) {
        return Err(CommunityError::InvalidProposal(format!(
            "ya hay un paquete {id} en el catálogo; cambia el nombre"
        )));
    }
    let mut animated_ones = Vec::with_capacity(pack.stickers.len());
    for sticker in &pack.stickers {
        let info = inspect_whatsapp_sticker(&sticker.bytes).map_err(|error| {
            CommunityError::InvalidProposal(format!("un sticker no es válido: {error}"))
        })?;
        animated_ones.push(info.kind == StickerKind::Animated);
    }
    let animated = animated_ones.first().copied().unwrap_or(false);
    if animated_ones
        .iter()
        .any(|&is_animated| is_animated != animated)
    {
        return Err(CommunityError::InvalidProposal(
            "mezcla stickers estáticos y animados".to_owned(),
        ));
    }
    let source = PackSource {
        id: id.clone(),
        version: 1,
        name: pack.name.clone(),
        author: pack.author.clone(),
        license: license.to_owned(),
        category: category.to_owned(),
        tags: Vec::new(),
        animated,
        added: today.to_owned(),
        icon: conversor_core::IconSource {
            file: ICON_FILE.to_owned(),
        },
        stickers: pack
            .stickers
            .iter()
            .enumerate()
            .map(|(number, sticker)| StickerSource {
                file: format!("{:02}.webp", number + 1),
                emojis: sticker.emojis.clone(),
            })
            .collect(),
    };
    validate_source_pack(&source, categories)
        .map_err(|error| CommunityError::InvalidProposal(error.to_string()))?;
    inspect_tray_icon(&pack.icon).map_err(|error| {
        CommunityError::InvalidProposal(format!("el icono no es válido: {error}"))
    })?;

    let json = serde_json::to_vec_pretty(&source).map_err(|error| {
        CommunityError::InvalidProposal(format!("no se pudo escribir pack.json: {error}"))
    })?;
    let readme = format!(
        "Copia la carpeta {id}/ dentro de packs/ del repositorio de la comunidad y abre un \
         pull request. Instrucciones: {PROPOSAL_INSTRUCTIONS}\n"
    );
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let mut write = |name: String, bytes: &[u8]| -> Result<(), CommunityError> {
        zip.start_file(name, options)
            .map_err(|error| disk("crear el zip")(io::Error::other(error)))?;
        zip.write_all(bytes).map_err(disk("crear el zip"))
    };
    write("LEEME.txt".to_owned(), readme.as_bytes())?;
    write(format!("{id}/{PACK_FILE}"), &json)?;
    for (number, sticker) in pack.stickers.iter().enumerate() {
        write(format!("{id}/{:02}.webp", number + 1), &sticker.bytes)?;
    }
    write(format!("{id}/{ICON_FILE}"), &pack.icon)?;
    Ok(zip
        .finish()
        .map_err(|error| disk("crear el zip")(io::Error::other(error)))?
        .into_inner())
}

pub(crate) fn today() -> String {
    date_of(now_unix() / 86_400)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Read};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    use conversor_core::{generate_index, serialize_index, tray_icon, validate_catalog};

    use super::*;
    use crate::library::tests::{animated, static_image};

    enum Route {
        Body(Vec<u8>, Option<&'static str>),
        Redirect(String),
    }

    struct Server {
        url: String,
        paths: Arc<Mutex<HashMap<String, Route>>>,
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Server {
        fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let paths: Arc<Mutex<HashMap<String, Route>>> = Arc::default();
            let stop = Arc::new(AtomicBool::new(false));
            let (paths_thread, stop_thread) = (Arc::clone(&paths), Arc::clone(&stop));
            let thread = std::thread::spawn(move || {
                for connection in listener.incoming() {
                    if stop_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Ok(connection) = connection {
                        serve(connection, &paths_thread);
                    }
                }
            });
            Server {
                url,
                paths,
                stop,
                thread: Some(thread),
            }
        }

        fn put(&self, route: &str, response: Route) {
            self.paths
                .lock()
                .unwrap()
                .insert(route.to_owned(), response);
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ =
                TcpStream::connect(self.url.trim_start_matches("http://").trim_end_matches('/'));
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn serve(mut connection: TcpStream, paths: &Mutex<HashMap<String, Route>>) {
        let mut reader = BufReader::new(connection.try_clone().unwrap());
        let mut first = String::new();
        if reader.read_line(&mut first).unwrap_or(0) == 0 {
            return;
        }
        let route = first.split_whitespace().nth(1).unwrap_or("/").to_owned();
        let mut etag_sent = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("if-none-match")
            {
                etag_sent = Some(value.trim().to_owned());
            }
        }
        let paths = paths.lock().unwrap();
        let response: Vec<u8> = match paths.get(&route) {
            None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
            Some(Route::Redirect(destination)) => format!(
                "HTTP/1.1 302 Found\r\nLocation: {destination}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .into_bytes(),
            Some(Route::Body(_, Some(etag))) if etag_sent.as_deref() == Some(*etag) => {
                b"HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_vec()
            }
            Some(Route::Body(body, etag)) => {
                let mut text = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                if let Some(etag) = etag {
                    text.push_str(&format!("ETag: {etag}\r\n"));
                }
                text.push_str("\r\n");
                let mut bytes = text.into_bytes();
                bytes.extend(body);
                bytes
            }
        };
        let _ = connection.write_all(&response);
    }

    fn publish(server: &Server) -> Index {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::write(
            root.join("categories.json"),
            r#"[{"id":"animales","name":"Animales"}]"#,
        )
        .unwrap();
        fs::write(
            root.join("withdrawn.json"),
            r#"[{"id":"viejo-paquete","date":"2026-11-23","reason":"Retirado por una reclamación"}]"#,
        )
        .unwrap();
        for (id, animated_) in [("gatos-garabato", false), ("bailes-ocho", true)] {
            let folder = root.join("packs").join(id);
            fs::create_dir_all(&folder).unwrap();
            let mut stickers = Vec::new();
            for n in 1..=3u8 {
                let bytes = if animated_ {
                    animated(n * 40)
                } else {
                    static_image(n * 40)
                };
                fs::write(folder.join(format!("{n:02}.webp")), &bytes).unwrap();
                stickers
                    .push(serde_json::json!({"file": format!("{n:02}.webp"), "emojis": ["🐱"]}));
            }
            let first = fs::read(folder.join("01.webp")).unwrap();
            fs::write(folder.join("icon.png"), tray_icon(&first).unwrap()).unwrap();
            let pack = serde_json::json!({
                "id": id, "version": 1, "name": "Paquete de prueba", "author": "Ana",
                "license": "CC0-1.0", "category": "animales", "tags": [],
                "animated": animated_, "added": "2026-11-22",
                "icon": {"file": "icon.png"}, "stickers": stickers
            });
            fs::write(folder.join("pack.json"), pack.to_string()).unwrap();
            for file in fs::read_dir(&folder).unwrap() {
                let path = file.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                if name != "pack.json" {
                    server.put(
                        &format!("/packs/{id}/{name}"),
                        Route::Body(fs::read(&path).unwrap(), None),
                    );
                }
            }
        }
        let index = generate_index(root).unwrap();
        server.put(
            "/index.json",
            Route::Body(
                serialize_index(&index).unwrap().into_bytes(),
                Some("\"v1\""),
            ),
        );
        index
    }

    fn client(server: &Server) -> (Client, tempfile::TempDir) {
        let cache = tempfile::tempdir().unwrap();
        let client = Client::new(&server.url, cache.path().join("community")).unwrap();
        (client, cache)
    }

    #[derive(Default)]
    struct Double {
        locals: Vec<(u32, String, u32)>,
        added: Vec<IncomingPack>,
        withdrawn: Vec<(String, String)>,
    }

    impl LocalStore for Double {
        fn community_ids(&self) -> Vec<(u32, String, u32)> {
            self.locals.clone()
        }

        fn add_from_community(&mut self, pack: IncomingPack) -> Result<u32, LibraryError> {
            self.added.push(pack);
            Ok(self.added.len() as u32)
        }

        fn mark_withdrawn(
            &mut self,
            withdrawn: &[(String, String)],
        ) -> Result<Vec<u32>, LibraryError> {
            self.withdrawn.extend(withdrawn.iter().cloned());
            Ok(self.locals.iter().map(|(id, _, _)| *id).collect())
        }

        fn pack_to_propose(&self, _: u32) -> Result<PackToPropose, LibraryError> {
            Err(LibraryError::OutOfIds)
        }
    }

    fn without_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn good_index_is_read_saved_and_second_time_uses_etag() {
        let server = Server::new();
        let expected = publish(&server);
        let (client, _cache) = client(&server);
        let first = client.index(true).unwrap();
        assert_eq!(first.index, expected);
        assert!(!first.offline);
        let second = client.index(true).unwrap();
        assert_eq!(second.index, expected);
        assert!(client.etag_path().is_file());
    }

    #[test]
    fn broken_format_2_or_too_large_index_is_rejected_and_not_saved() {
        let server = Server::new();
        let (client, _cache) = client(&server);
        server.put("/index.json", Route::Body(b"{ roto".to_vec(), None));
        assert!(matches!(
            client.index(true),
            Err(CommunityError::Catalog(_))
        ));
        server.put(
            "/index.json",
            Route::Body(br#"{"format":2,"otro":1}"#.to_vec(), None),
        );
        let error = client.index(true).unwrap_err();
        assert!(matches!(error, CommunityError::NewFormat));
        assert_eq!(error.to_string(), "Actualiza la app para ver el catálogo");
        server.put("/index.json", Route::Body(vec![b' '; 5_000_001], None));
        assert!(matches!(
            client.index(true),
            Err(CommunityError::TooLarge { .. })
        ));
        assert!(!client.index_path().exists());
    }

    #[test]
    fn offline_uses_cache_and_fails_without_cache() {
        let server = Server::new();
        publish(&server);
        let (client, cache) = client(&server);
        let online = client.index(true).unwrap();

        let closed = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/", listener.local_addr().unwrap())
        };
        let no_network = Client::new(&closed, cache.path().join("community")).unwrap();
        let read = no_network.index(true).unwrap();
        assert!(read.offline);
        assert_eq!(read.index, online.index);

        let empty = tempfile::tempdir().unwrap();
        let nothing = Client::new(&closed, empty.path().join("community")).unwrap();
        let error = nothing.index(true).unwrap_err();
        assert_eq!(error.to_string(), "Sin conexión y sin catálogo guardado");
    }

    #[test]
    fn base_url_must_be_https() {
        assert!(validate_base_url("http://ejemplo.org/").is_err());
        assert!(validate_base_url("ftp://ejemplo.org/").is_err());
        assert!(validate_base_url("https://ejemplo.org/a b").is_err());
        assert_eq!(
            validate_base_url("https://ejemplo.org/catalogo").unwrap(),
            "https://ejemplo.org/catalogo/"
        );
    }

    #[test]
    fn only_same_origin_redirects_are_followed() {
        assert_eq!(
            resolve_redirect("https://a.org/x/y", "/z").as_deref(),
            Some("https://a.org/z")
        );
        assert_eq!(
            resolve_redirect("https://a.org/x", "https://a.org/otro").as_deref(),
            Some("https://a.org/otro")
        );
        assert_eq!(resolve_redirect("https://a.org/x", "https://b.org/x"), None);
        assert_eq!(resolve_redirect("https://a.org/x", "http://a.org/x"), None);
        assert_eq!(resolve_redirect("https://a.org/x", "//b.org/x"), None);
    }

    #[test]
    fn redirect_to_same_server_is_followed_and_to_another_is_not() {
        let server = Server::new();
        publish(&server);
        let (client, _cache) = client(&server);
        server.put("/old.json", Route::Redirect("/index.json".to_owned()));
        let response = client
            .fetch_text(&format!("{}old.json", server.url), 5_000_000, None)
            .unwrap();
        assert!(response.body.is_some());

        server.put(
            "/outside.json",
            Route::Redirect("http://127.0.0.2:1/index.json".to_owned()),
        );
        assert!(matches!(
            client.fetch_text(&format!("{}outside.json", server.url), 5_000_000, None),
            Err(CommunityError::DisallowedRedirect)
        ));
    }

    #[test]
    fn downloading_good_pack_returns_verified_bytes_and_reports_progress() {
        let server = Server::new();
        publish(&server);
        let (client, _cache) = client(&server);
        let mut progress_updates = Vec::new();
        let incoming = client
            .download("gatos-garabato", &without_cancel(), &mut |p| {
                progress_updates.push(p)
            })
            .unwrap();
        assert_eq!(incoming.id, "gatos-garabato");
        assert_eq!(incoming.stickers.len(), 3);
        assert_eq!(incoming.stickers[0].emojis, ["🐱"]);
        assert_eq!(
            progress_updates.first(),
            Some(&CommunityProgressUi::Downloading { done: 0, total: 4 })
        );
        assert_eq!(
            progress_updates.last(),
            Some(&CommunityProgressUi::Verifying)
        );
    }

    #[test]
    fn file_with_bad_checksum_cancels_download_and_adds_nothing() {
        let server = Server::new();
        publish(&server);
        let original = match server
            .paths
            .lock()
            .unwrap()
            .get("/packs/gatos-garabato/03.webp")
        {
            Some(Route::Body(bytes, _)) => bytes.clone(),
            _ => unreachable!(),
        };
        let mut changed = original;
        let last = changed.len() - 1;
        changed[last] ^= 0xff;
        server.put("/packs/gatos-garabato/03.webp", Route::Body(changed, None));
        let (client, _cache) = client(&server);
        let mut store = Double::default();
        let result = client.download("gatos-garabato", &without_cancel(), &mut |_| {});
        let error = result.as_ref().unwrap_err();
        assert_eq!(
            error.to_string(),
            "El archivo 03.webp no coincide con su suma: descarga cancelada"
        );
        if let Ok(pack) = result {
            store.add_from_community(pack).unwrap();
        }
        assert!(store.added.is_empty());
    }

    #[test]
    fn file_of_other_size_or_withdrawn_or_cancelled_pack_is_not_downloaded() {
        let server = Server::new();
        publish(&server);
        server.put(
            "/packs/bailes-ocho/02.webp",
            Route::Body(vec![1, 2, 3], None),
        );
        let (client, _cache) = client(&server);
        assert!(matches!(
            client.download("bailes-ocho", &without_cancel(), &mut |_| {}),
            Err(CommunityError::IncorrectSize { .. })
        ));
        let error = client
            .download("viejo-paquete", &without_cancel(), &mut |_| {})
            .unwrap_err();
        assert_eq!(error.to_string(), "Este paquete se retiró del catálogo");
        assert!(matches!(
            client.download("no-existe", &without_cancel(), &mut |_| {}),
            Err(CommunityError::MissingPack { .. })
        ));
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            client.download("gatos-garabato", &cancelled, &mut |_| {}),
            Err(CommunityError::Cancelled)
        ));
    }

    #[test]
    fn file_larger_than_limit_is_cut() {
        let server = Server::new();
        publish(&server);
        server.put(
            "/packs/gatos-garabato/01.webp",
            Route::Body(vec![0; 500_001], None),
        );
        let (client, _cache) = client(&server);
        assert!(matches!(
            client.download("gatos-garabato", &without_cancel(), &mut |_| {}),
            Err(CommunityError::TooLarge { .. })
        ));
    }

    #[test]
    fn catalog_marks_downloaded_updatable_and_cached_icons() {
        let server = Server::new();
        let index = publish(&server);
        let (client, _cache) = client(&server);
        let read = client.index(true).unwrap();
        let locals = vec![(7, "gatos-garabato".to_owned(), 0)];
        let ui = client.catalog(&read, &locals, true);
        assert_eq!(ui.packs.len(), index.packs.len());
        let gatos = ui.packs.iter().find(|p| p.id == "gatos-garabato").unwrap();
        assert_eq!(gatos.downloaded, Some(7));
        assert!(gatos.updatable);
        assert!(gatos.icon.is_some());
        let bailes = ui.packs.iter().find(|p| p.id == "bailes-ocho").unwrap();
        assert_eq!((bailes.downloaded, bailes.updatable), (None, false));
        assert!(ui.notice_accepted && !ui.offline);
    }

    #[test]
    fn detail_fetches_samples_into_cache() {
        let server = Server::new();
        publish(&server);
        let (client, _cache) = client(&server);
        let detail = client.detail("bailes-ocho", &[]).unwrap();
        assert_eq!(detail.stickers.len(), 3);
        assert!(detail.stickers.iter().all(|s| Path::new(&s.path).is_file()));
    }

    #[test]
    fn index_withdrawn_mark_only_local_packs() {
        let server = Server::new();
        publish(&server);
        let (client, _cache) = client(&server);
        let read = client.index(true).unwrap();
        let mut store = Double {
            locals: vec![
                (3, "viejo-paquete".to_owned(), 1),
                (4, "gatos-garabato".to_owned(), 1),
            ],
            ..Double::default()
        };
        let affected = apply_withdrawn(&read.index, &mut store).unwrap();
        assert_eq!(affected, [3, 4]);
        assert_eq!(
            store.withdrawn,
            [(
                "viejo-paquete".to_owned(),
                "Retirado por una reclamación".to_owned()
            )]
        );
        let mut without_locals = Double::default();
        assert!(
            apply_withdrawn(&read.index, &mut without_locals)
                .unwrap()
                .is_empty()
        );
        assert!(without_locals.withdrawn.is_empty());
    }

    #[test]
    fn notice_is_accepted_once() {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("datos");
        assert!(!notice_accepted(&data));
        accept_notice(&data).unwrap();
        assert!(notice_accepted(&data));
    }

    fn local(animated_: bool) -> PackToPropose {
        let stickers: Vec<IncomingSticker> = (1..=3u8)
            .map(|n| IncomingSticker {
                bytes: if animated_ {
                    animated(n * 40)
                } else {
                    static_image(n * 40)
                },
                emojis: vec!["🙂".to_owned()],
            })
            .collect();
        PackToPropose {
            name: "Mis gatos únicos".to_owned(),
            author: "Ana".to_owned(),
            icon: tray_icon(&stickers[0].bytes).unwrap(),
            stickers,
        }
    }

    fn animales() -> Vec<Category> {
        vec![Category {
            id: "animales".to_owned(),
            name: "Animales".to_owned(),
        }]
    }

    #[test]
    fn propose_generates_zip_the_catalog_validator_accepts() {
        for animated_ in [false, true] {
            let zip = propose(
                &local(animated_),
                "CC-BY-4.0",
                "animales",
                &animales(),
                &[],
                "2026-11-22",
            )
            .unwrap();
            let mut file = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            fs::write(
                root.join("categories.json"),
                r#"[{"id":"animales","name":"Animales"}]"#,
            )
            .unwrap();
            fs::write(root.join("withdrawn.json"), "[]").unwrap();
            let mut seen = Vec::new();
            for index in 0..file.len() {
                let mut entry = file.by_index(index).unwrap();
                let name = entry.name().unwrap().into_owned();
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                if name == "LEEME.txt" {
                    assert!(
                        String::from_utf8(bytes)
                            .unwrap()
                            .contains("CONTRIBUTING.md")
                    );
                    continue;
                }
                assert!(name.starts_with("mis-gatos-unicos/"), "{name}");
                let destination = root.join("packs").join(&name);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::write(destination, bytes).unwrap();
                seen.push(name);
            }
            assert_eq!(seen.len(), 5);
            let catalog = validate_catalog(root).unwrap();
            assert_eq!(catalog.packs[0].animated, animated_);
        }
    }

    #[test]
    fn propose_rejects_license_category_name_or_repeated_id() {
        let pack = local(false);
        let try_fit = |license: &str, category: &str, existing: &[String]| {
            propose(
                &pack,
                license,
                category,
                &animales(),
                existing,
                "2026-11-22",
            )
            .unwrap_err()
            .to_string()
        };
        assert!(try_fit("MIT", "animales", &[]).contains("licencia"));
        assert!(try_fit("CC0-1.0", "nada", &[]).contains("categoría"));
        assert!(
            try_fit("CC0-1.0", "animales", &["mis-gatos-unicos".to_owned()]).contains("ya hay")
        );
        let without_name = PackToPropose {
            name: "!!".to_owned(),
            ..local(false)
        };
        assert!(
            propose(
                &without_name,
                "CC0-1.0",
                "animales",
                &animales(),
                &[],
                "2026-11-22"
            )
            .is_err()
        );
    }

    #[test]
    fn ids_and_dates() {
        assert_eq!(
            id_from("Mis gatos únicos").as_deref(),
            Some("mis-gatos-unicos")
        );
        assert_eq!(id_from("  ¡Ñandú!! ").as_deref(), Some("nandu"));
        assert_eq!(id_from("ab"), None);
        assert!(id_from(&"a".repeat(100)).is_some_and(|id| id.len() == 64));
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(19_723), "2024-01-01");
        assert_eq!(date_of(19_782), "2024-02-29");
    }
}
