//! Pack library on disk: a JSON index and a directory per pack with its WebPs and tray icon.
//!
//! The index is external input (it can be edited by hand or come from another version), so it is
//! validated whole on open. File names come only from numeric ids assigned by the library, never
//! from user text.

use std::collections::HashSet;
use std::fs;
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use conversor_core::{
    DEFAULT_EMOJI, EmojiError, StickerError, StickerKind as CoreKind, WhatsappSticker,
    inspect_tray_icon, inspect_whatsapp_sticker, tray_icon, validate_emojis,
};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::contract::{
    CategoryUi, CheckKey, CheckUi, FavoriteTarget, LibraryUi, MediaOrigin, OriginUi, PackUi,
    StickerKind, StickerUi,
};
pub(crate) mod backup;

use crate::storage::write_atomic;
use crate::wastickers::{Imported, PackToExport};
use crate::whatsapp::{PublishablePack, PublishableSticker};

const VERSION: u32 = 1;
const MAX_INDEX_SIZE_BYTES: u64 = 2_000_000;
const MAX_PACKS: usize = 200;
const MAX_CATEGORIES: usize = 50;
/// WhatsApp limit for the name and author of a pack.
const MAX_NAME_LENGTH: usize = 128;
const MAX_CATEGORY_LENGTH: usize = 40;
/// WhatsApp limits per pack: up to 30 stickers, and 3 to export it.
pub(crate) const MAX_STICKERS: usize = 30;
const QUICK_PACK_ANIMATED: &str = "Mis stickers rápidos";
const QUICK_PACK_STATIC: &str = "Mis stickers rápidos (estáticos)";
pub(crate) const MIN_STICKERS_TO_EXPORT: usize = 3;
/// The most a valid sticker weighs (animated; a static one, 100 KB).
const STICKER_MAX_SIZE: u64 = 500_000;

const INDEX_FILE: &str = "library.json";
const MAX_REASON_LENGTH: usize = 500;
const PACKS_DIRECTORY: &str = "packs";
const ICON_FILE: &str = "tray.png";

pub(crate) struct StickerToExport {
    pub(crate) pack_name: String,
    /// Starts at 1, as the interface counts the stickers.
    pub(crate) position: usize,
    pub(crate) bytes: Vec<u8>,
    pub(crate) media_origin: MediaOrigin,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum LibraryError {
    #[error(
        "la biblioteca es de una versión más nueva de la app (formato {version}); \
         actualiza la app para abrirla"
    )]
    NewerVersion { version: u32 },

    #[error("{field} no es válido: {reason}")]
    InvalidName { field: &'static str, reason: String },

    #[error("ya hay una categoría con ese nombre")]
    RepeatedCategory,

    #[error("no hay ninguna categoría con el número {id}")]
    MissingCategory { id: u32 },

    #[error("no hay ningún paquete con el número {id}")]
    MissingPack { id: u32 },

    #[error("como mucho puede haber {max} {what}")]
    Full { what: &'static str, max: usize },

    #[error("no se pudo {action}: {source}")]
    Disk {
        action: &'static str,
        #[source]
        source: io::Error,
    },

    #[error("la biblioteca se ha quedado sin números para paquetes y stickers")]
    OutOfIds,

    #[error("el paquete no tiene ningún sticker con el número {id}")]
    MissingSticker { id: u32 },

    #[error("no vale como sticker de WhatsApp: {0}")]
    InvalidSticker(#[source] StickerError),

    #[error("este paquete es de stickers {kind}; WhatsApp no deja mezclarlos")]
    MixedKind { kind: &'static str },

    #[error("no se pudo crear el icono del paquete: {0}")]
    Icon(#[source] StickerError),

    #[error("el paquete no se puede exportar: {reason}")]
    NotExportable { reason: String },

    #[error("No se puede proponer: contiene stickers de KLIPY/GIPHY")]
    HasExternalStickers,

    #[error(transparent)]
    Emoji(#[from] EmojiError),

    #[error("el orden tiene que nombrar cada sticker del paquete una sola vez")]
    InvalidOrder,

    #[error("no hay ningún sticker favorito")]
    NoFavorites,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidSticker {
    bytes: Vec<u8>,
    kind: StickerKind,
}

impl ValidSticker {
    pub(crate) fn new(bytes: Vec<u8>) -> Result<Self, StickerError> {
        let sticker = inspect_whatsapp_sticker(&bytes)?;
        Ok(ValidSticker {
            kind: kind_of(&sticker),
            bytes,
        })
    }

    pub(crate) fn kind(&self) -> StickerKind {
        self.kind
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

fn disk(action: &'static str) -> impl FnOnce(io::Error) -> LibraryError {
    move |source| LibraryError::Disk { action, source }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    /// One counter for categories, packs and stickers; a number is never reused, so an orphan file is
    /// never mistaken for another.
    next_id: u32,
    categories: Vec<Category>,
    packs: Vec<Pack>,
}

impl Default for Index {
    fn default() -> Self {
        Index {
            version: VERSION,
            next_id: 1,
            categories: Vec::new(),
            packs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Category {
    id: u32,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pack {
    id: u32,
    name: String,
    author: String,
    category: Option<u32>,
    /// Sticker the tray icon comes from; `None` if the pack is empty.
    icon_from: Option<u32>,
    favorite: bool,
    modified: u64,
    published: Option<u32>,
    origin: OriginUi,
    /// Set on the automatic quick pack of that kind, whatever its name is later edited to.
    #[serde(default)]
    quick: Option<StickerKind>,
    stickers: Vec<Sticker>,
}

#[derive(Debug)]
pub(crate) struct IncomingPack {
    pub(crate) id: String,
    pub(crate) version: u32,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) license: String,
    pub(crate) original_author: String,
    pub(crate) icon: Option<Vec<u8>>,
    pub(crate) stickers: Vec<IncomingSticker>,
}

#[derive(Debug)]
pub(crate) struct IncomingSticker {
    pub(crate) sticker: ValidSticker,
    pub(crate) emojis: Vec<String>,
}

struct NewSticker<'a> {
    sticker: &'a ValidSticker,
    emojis: &'a [String],
    media_origin: MediaOrigin,
}

impl Pack {
    fn sticker_mut(&mut self, id: u32) -> Result<&mut Sticker, LibraryError> {
        self.stickers
            .iter_mut()
            .find(|sticker| sticker.id == id)
            .ok_or(LibraryError::MissingSticker { id })
    }

    fn kind(&self) -> Option<StickerKind> {
        self.stickers.first().map(|sticker| sticker.kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sticker {
    id: u32,
    kind: StickerKind,
    size_bytes: u64,
    emojis: Vec<String>,
    favorite: bool,
    #[serde(default)]
    media_origin: MediaOrigin,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |from| from.as_secs())
}

impl Index {
    fn new_id(&mut self) -> Result<u32, LibraryError> {
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(LibraryError::OutOfIds)?;
        Ok(id)
    }

    fn pack(&self, id: u32) -> Result<&Pack, LibraryError> {
        self.packs
            .iter()
            .find(|pack| pack.id == id)
            .ok_or(LibraryError::MissingPack { id })
    }

    fn pack_mut(&mut self, id: u32) -> Result<&mut Pack, LibraryError> {
        self.packs
            .iter_mut()
            .find(|pack| pack.id == id)
            .ok_or(LibraryError::MissingPack { id })
    }

    fn check_category(&self, category: Option<u32>) -> Result<(), LibraryError> {
        match category {
            Some(id) if !self.categories.iter().any(|category| category.id == id) => {
                Err(LibraryError::MissingCategory { id })
            }
            _ => Ok(()),
        }
    }

    fn check_unique_category(&self, name: &str, except: Option<u32>) -> Result<(), LibraryError> {
        let searched = name.to_lowercase();
        let repeated = self.categories.iter().any(|category| {
            Some(category.id) != except && category.name.to_lowercase() == searched
        });
        if repeated {
            Err(LibraryError::RepeatedCategory)
        } else {
            Ok(())
        }
    }
}

fn same_origin(one: &OriginUi, other: &OriginUi) -> bool {
    match (one, other) {
        (OriginUi::Community { id: one, .. }, OriginUi::Community { id: other, .. }) => {
            one == other
        }
        _ => false,
    }
}

fn clean_reason(reason: &str) -> String {
    let clean: String = reason
        .chars()
        .map(|char| if char.is_control() { ' ' } else { char })
        .take(MAX_REASON_LENGTH)
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        "sin motivo".to_string()
    } else {
        clean.to_string()
    }
}

fn normalize(
    text: &str,
    field: &'static str,
    min: usize,
    max: usize,
) -> Result<String, LibraryError> {
    let not_valid = |reason: String| LibraryError::InvalidName { field, reason };
    let cropped = text.trim();
    if cropped.chars().any(char::is_control) {
        return Err(not_valid(
            "no puede llevar caracteres de control".to_string(),
        ));
    }
    let long = cropped.chars().count();
    if long < min {
        return Err(not_valid("no puede estar vacío".to_string()));
    }
    if long > max {
        return Err(not_valid(format!(
            "como mucho puede tener {max} caracteres"
        )));
    }
    Ok(cropped.to_string())
}

pub(crate) fn normalize_pack_name(name: &str) -> Result<String, LibraryError> {
    normalize(name, "el nombre del paquete", 1, MAX_NAME_LENGTH)
}

pub(crate) fn normalize_author(author: &str) -> Result<String, LibraryError> {
    normalize(author, "el autor", 0, MAX_NAME_LENGTH)
}

fn normalize_category(name: &str) -> Result<String, LibraryError> {
    normalize(name, "el nombre de la categoría", 1, MAX_CATEGORY_LENGTH)
}

enum InvalidIndex {
    NewerVersion(u32),
    Damaged(String),
}

fn read_index(bytes: &[u8]) -> Result<Index, InvalidIndex> {
    #[derive(Deserialize)]
    struct VersionOnly {
        version: u32,
    }
    let damaged = |reason: String| InvalidIndex::Damaged(reason);
    let read_error =
        |error: serde_json::Error| damaged(format!("el índice no se puede leer: {error}"));
    let version = serde_json::from_slice::<VersionOnly>(bytes)
        .map_err(read_error)?
        .version;
    if version > VERSION {
        return Err(InvalidIndex::NewerVersion(version));
    }
    if version != VERSION {
        return Err(damaged(format!("versión {version} desconocida")));
    }
    let index: Index = serde_json::from_slice(bytes).map_err(read_error)?;
    validate(&index).map_err(damaged)?;
    Ok(index)
}

fn validate(index: &Index) -> Result<(), String> {
    if index.categories.len() > MAX_CATEGORIES {
        return Err(format!("tiene más de {MAX_CATEGORIES} categorías"));
    }
    if index.packs.len() > MAX_PACKS {
        return Err(format!("tiene más de {MAX_PACKS} paquetes"));
    }
    let mut ids = HashSet::new();
    let mut check_id = |id: u32| {
        if id == 0 || id >= index.next_id || !ids.insert(id) {
            Err(format!("el número {id} está repetido o fuera de rango"))
        } else {
            Ok(())
        }
    };
    let mut category_names = HashSet::new();
    for category in &index.categories {
        check_id(category.id)?;
        let valid = normalize_category(&category.name).map_err(|e| e.to_string())?;
        if valid != category.name || !category_names.insert(valid.to_lowercase()) {
            return Err(format!("la categoría {} no es válida", category.id));
        }
    }
    for pack in &index.packs {
        check_id(pack.id)?;
        let not_valid = || format!("el paquete {} no es válido", pack.id);
        let name = normalize_pack_name(&pack.name).map_err(|_| not_valid())?;
        let author = normalize_author(&pack.author).map_err(|_| not_valid())?;
        if name != pack.name || author != pack.author {
            return Err(not_valid());
        }
        if let Some(category) = pack.category
            && !index.categories.iter().any(|c| c.id == category)
        {
            return Err(not_valid());
        }
        if pack.stickers.len() > MAX_STICKERS {
            return Err(not_valid());
        }
        for sticker in &pack.stickers {
            check_id(sticker.id)?;
            if Some(sticker.kind) != pack.kind()
                || sticker.size_bytes > STICKER_MAX_SIZE
                || validate_emojis(&sticker.emojis).is_err()
            {
                return Err(not_valid());
            }
        }
        if let OriginUi::Community {
            id,
            license,
            original_author,
            withdrawn,
            ..
        } = &pack.origin
        {
            let valid_text = normalize(id, "el origen", 1, MAX_NAME_LENGTH).is_ok()
                && normalize(license, "el origen", 1, MAX_NAME_LENGTH).is_ok()
                && normalize(original_author, "el origen", 0, MAX_NAME_LENGTH).is_ok()
                && withdrawn.as_deref().is_none_or(|reason| {
                    normalize(reason, "el origen", 1, MAX_REASON_LENGTH).is_ok()
                });
            if !valid_text {
                return Err(not_valid());
            }
        }
        if let Some(id) = pack.icon_from
            && !pack.stickers.iter().any(|sticker| sticker.id == id)
        {
            return Err(not_valid());
        }
    }
    Ok(())
}

fn read_file(path: &Path, max: u64) -> io::Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "no es un archivo normal",
        ));
    }
    if metadata.len() > max {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "es demasiado grande",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "es demasiado grande",
        ));
    }
    Ok(Some(bytes))
}

fn set_aside(root: &Path) -> Result<PathBuf, LibraryError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |from| from.as_secs());
    let name = root
        .file_name()
        .map_or_else(|| "library".into(), |name| name.to_string_lossy());
    let mut destination = root.with_file_name(format!("{name}.damaged-{seconds}"));
    let mut attempt = 1;
    while fs::symlink_metadata(&destination).is_ok() {
        attempt += 1;
        destination = root.with_file_name(format!("{name}.damaged-{seconds}-{attempt}"));
    }
    fs::rename(root, &destination).map_err(disk("apartar la biblioteca dañada"))?;
    fs::create_dir_all(root).map_err(disk("crear la biblioteca"))?;
    Ok(destination)
}

#[derive(Debug)]
pub(crate) struct Library {
    root: PathBuf,
    index: Index,
    notice: Option<String>,
}

impl Library {
    pub(crate) fn open(root: &Path) -> Result<Self, LibraryError> {
        fs::create_dir_all(root).map_err(disk("crear la biblioteca"))?;
        let empty = |notice| Library {
            root: root.to_path_buf(),
            index: Index::default(),
            notice,
        };
        let reason = match read_file(&root.join(INDEX_FILE), MAX_INDEX_SIZE_BYTES) {
            Ok(Some(bytes)) => match read_index(&bytes) {
                Ok(index) => {
                    let mut library = Library {
                        root: root.to_path_buf(),
                        index,
                        notice: None,
                    };
                    library.check_files();
                    return Ok(library);
                }
                Err(InvalidIndex::NewerVersion(version)) => {
                    return Err(LibraryError::NewerVersion { version });
                }
                Err(InvalidIndex::Damaged(reason)) => reason,
            },
            Ok(None) if !has_packs(root) => return Ok(empty(None)),
            Ok(None) => "falta el índice".to_string(),
            Err(error) if error.kind() == ErrorKind::InvalidData => {
                format!("el índice {error}")
            }
            Err(source) => {
                return Err(LibraryError::Disk {
                    action: "leer la biblioteca",
                    source,
                });
            }
        };
        warn!("la biblioteca está dañada; se aparta y se empieza con una vacía");
        let destination = set_aside(root)?;
        Ok(empty(Some(format!(
            "La biblioteca estaba dañada ({reason}). Se ha apartado con sus stickers a \
             {} y se empieza con una vacía.",
            destination.display()
        ))))
    }

    pub(crate) fn packs_directory(root: &Path) -> PathBuf {
        root.join(PACKS_DIRECTORY)
    }

    fn pack_directory(&self, id: u32) -> PathBuf {
        Self::packs_directory(&self.root).join(id.to_string())
    }

    fn sticker_path(&self, pack: u32, sticker: u32) -> PathBuf {
        self.pack_directory(pack).join(format!("{sticker}.webp"))
    }

    fn icon_path(&self, pack: u32) -> PathBuf {
        self.pack_directory(pack).join(ICON_FILE)
    }

    fn read_sticker(&self, pack: u32, sticker: u32) -> Result<Vec<u8>, LibraryError> {
        read_file(&self.sticker_path(pack, sticker), STICKER_MAX_SIZE)
            .and_then(|bytes| {
                bytes.ok_or_else(|| io::Error::new(ErrorKind::NotFound, "falta su archivo"))
            })
            .map_err(disk("leer el sticker"))
    }

    fn write_icon(&self, pack: u32, sticker: u32) -> Result<(), LibraryError> {
        let png = tray_icon(&self.read_sticker(pack, sticker)?).map_err(LibraryError::Icon)?;
        write_atomic(&self.icon_path(pack), &png).map_err(disk("guardar el icono"))
    }

    fn check_files(&mut self) {
        let mut new = self.index.clone();
        let mut lost = 0;
        for pack in &mut new.packs {
            let before = pack.stickers.len();
            pack.stickers
                .retain(|sticker| is_regular_file(&self.sticker_path(pack.id, sticker.id)));
            lost += before - pack.stickers.len();
            let lost_icon = match pack.icon_from {
                Some(id) => {
                    !pack.stickers.iter().any(|sticker| sticker.id == id)
                        || !is_regular_file(&self.icon_path(pack.id))
                }
                None => !pack.stickers.is_empty(),
            };
            if lost_icon {
                pack.icon_from = pack.stickers.first().map(|sticker| sticker.id);
                if let Some(first) = pack.icon_from
                    && let Err(error) = self.write_icon(pack.id, first)
                {
                    warn!(pack = pack.id, %error, "no se pudo regenerar el icono");
                    pack.icon_from = None;
                }
            }
        }
        if new == self.index {
            return;
        }
        if lost > 0 {
            warn!(lost, "se quitan de la biblioteca stickers sin archivo");
            self.notice = Some(format!(
                "Se {} de la biblioteca {lost} {} cuyo archivo faltaba.",
                if lost == 1 { "quitó" } else { "quitaron" },
                if lost == 1 { "sticker" } else { "stickers" },
            ));
        }
        if let Err(error) = self.apply_change(|index| {
            *index = new;
            Ok(())
        }) {
            warn!(%error, "no se pudo guardar la biblioteca revisada");
        }
    }

    pub(crate) fn add_sticker(
        &mut self,
        pack: u32,
        bytes: &[u8],
        media_origin: MediaOrigin,
    ) -> Result<u32, LibraryError> {
        let current = self.index.pack(pack)?;
        if current.stickers.len() >= MAX_STICKERS {
            return Err(LibraryError::Full {
                what: "stickers en un paquete",
                max: MAX_STICKERS,
            });
        }
        let sticker = inspect_whatsapp_sticker(bytes).map_err(LibraryError::InvalidSticker)?;
        let kind = kind_of(&sticker);
        if let Some(of_pack) = current.kind()
            && of_pack != kind
        {
            return Err(LibraryError::MixedKind {
                kind: kind_name(of_pack),
            });
        }
        let needs_icon = current.icon_from.is_none();
        let id = self.index.next_id;
        fs::create_dir_all(self.pack_directory(pack))
            .map_err(disk("crear el directorio del paquete"))?;
        let path = self.sticker_path(pack, id);
        write_atomic(&path, bytes).map_err(disk("guardar el sticker"))?;
        let result = (|| {
            if needs_icon {
                self.write_icon(pack, id)?;
            }
            self.apply_change(|index| {
                let id = index.new_id()?;
                let pack = index.pack_mut(pack)?;
                pack.stickers.push(Sticker {
                    id,
                    kind,
                    size_bytes: sticker.size_bytes,
                    emojis: vec![DEFAULT_EMOJI.to_string()],
                    favorite: false,
                    media_origin,
                });
                pack.modified = now();
                if needs_icon {
                    pack.icon_from = Some(id);
                }
                Ok(id)
            })
        })();
        if result.is_err() {
            delete_file(&path, pack);
        }
        result
    }

    pub(crate) fn remove_sticker(&mut self, pack: u32, sticker: u32) -> Result<(), LibraryError> {
        let current = self.index.pack(pack)?;
        if !current.stickers.iter().any(|s| s.id == sticker) {
            return Err(LibraryError::MissingSticker { id: sticker });
        }
        let mut icon_from = current.icon_from;
        if icon_from == Some(sticker) {
            icon_from = current
                .stickers
                .iter()
                .map(|s| s.id)
                .find(|&id| id != sticker);
            if let Some(new) = icon_from {
                self.write_icon(pack, new)?;
            }
        }
        self.apply_change(|index| {
            let current = index.pack_mut(pack)?;
            current.stickers.retain(|s| s.id != sticker);
            current.icon_from = icon_from;
            current.modified = now();
            Ok(())
        })?;
        delete_file(&self.sticker_path(pack, sticker), pack);
        if icon_from.is_none() {
            delete_file(&self.icon_path(pack), pack);
        }
        Ok(())
    }

    pub(crate) fn import(&mut self, imported: &Imported) -> Result<u32, LibraryError> {
        let default = [DEFAULT_EMOJI.to_string()];
        let stickers: Vec<NewSticker> = imported
            .stickers
            .iter()
            .enumerate()
            .map(|(position, sticker)| NewSticker {
                sticker,
                emojis: imported
                    .emojis
                    .get(position)
                    .map_or(&default[..], Vec::as_slice),
                media_origin: MediaOrigin::Own,
            })
            .collect();
        self.create_with_files(
            &imported.title,
            &imported.author,
            imported.icon.as_deref(),
            &stickers,
            OriginUi::Own,
        )
    }

    fn create_with_files(
        &mut self,
        title: &str,
        author: &str,
        icon: Option<&[u8]>,
        stickers: &[NewSticker],
        origin: OriginUi,
    ) -> Result<u32, LibraryError> {
        let name = normalize_pack_name(title)?;
        let author = normalize_author(author)?;
        let Some(first) = stickers.first() else {
            return Err(LibraryError::NotExportable {
                reason: "no tiene stickers".to_string(),
            });
        };
        let kind = first.sticker.kind();
        if stickers.len() > MAX_STICKERS || stickers.iter().any(|new| new.sticker.kind() != kind) {
            return Err(LibraryError::MixedKind {
                kind: kind_name(kind),
            });
        }
        for new in stickers {
            validate_emojis(new.emojis)?;
        }
        if self.index.packs.len() >= MAX_PACKS {
            return Err(LibraryError::Full {
                what: "paquetes",
                max: MAX_PACKS,
            });
        }
        let pack = self.index.next_id;
        let ids: Vec<u32> = (1..=stickers.len())
            .map(|offset| pack.checked_add(offset as u32))
            .collect::<Option<_>>()
            .ok_or(LibraryError::OutOfIds)?;
        let directory = self.pack_directory(pack);
        let result = (|| {
            fs::create_dir_all(&directory).map_err(disk("crear el directorio del paquete"))?;
            for (id, new) in ids.iter().zip(stickers) {
                write_atomic(&self.sticker_path(pack, *id), new.sticker.bytes())
                    .map_err(disk("guardar el sticker"))?;
            }
            match icon {
                Some(png) => {
                    write_atomic(&self.icon_path(pack), png).map_err(disk("guardar el icono"))?
                }
                None => self.write_icon(pack, ids[0])?,
            }
            self.apply_change(|index| {
                let id = index.new_id()?;
                let mut saved = Vec::new();
                for new in stickers {
                    saved.push(Sticker {
                        id: index.new_id()?,
                        kind,
                        size_bytes: new.sticker.bytes().len() as u64,
                        emojis: new.emojis.to_vec(),
                        favorite: false,
                        media_origin: new.media_origin,
                    });
                }
                index.packs.push(Pack {
                    id,
                    name,
                    author,
                    category: None,
                    icon_from: saved.first().map(|sticker| sticker.id),
                    favorite: false,
                    modified: now(),
                    published: None,
                    origin,
                    quick: None,
                    stickers: saved,
                });
                Ok(id)
            })
        })();
        if result.is_err() {
            delete_directory(&directory, pack);
        }
        result
    }

    pub(crate) fn add_from_community(
        &mut self,
        incoming: IncomingPack,
    ) -> Result<u32, LibraryError> {
        let origin = OriginUi::Community {
            id: normalize(&incoming.id, "el id del catálogo", 1, MAX_NAME_LENGTH)?,
            version: incoming.version,
            license: normalize(&incoming.license, "la licencia", 1, MAX_NAME_LENGTH)?,
            original_author: normalize(
                &incoming.original_author,
                "el autor original",
                0,
                MAX_NAME_LENGTH,
            )?,
            withdrawn: None,
        };
        if let Some(icon) = &incoming.icon {
            inspect_tray_icon(icon).map_err(LibraryError::Icon)?;
        }
        let existing = self
            .index
            .packs
            .iter()
            .find(|pack| same_origin(&pack.origin, &origin))
            .map(|pack| pack.id);
        match existing {
            Some(pack) => self.replace_from_community(pack, &incoming, origin),
            None => {
                let stickers: Vec<NewSticker> = incoming
                    .stickers
                    .iter()
                    .map(|incoming| NewSticker {
                        sticker: &incoming.sticker,
                        emojis: &incoming.emojis,
                        media_origin: MediaOrigin::Own,
                    })
                    .collect();
                self.create_with_files(
                    &incoming.name,
                    &incoming.author,
                    incoming.icon.as_deref(),
                    &stickers,
                    origin,
                )
            }
        }
    }

    fn replace_from_community(
        &mut self,
        pack: u32,
        incoming: &IncomingPack,
        origin: OriginUi,
    ) -> Result<u32, LibraryError> {
        let name = normalize_pack_name(&incoming.name)?;
        let author = normalize_author(&incoming.author)?;
        let Some(first) = incoming.stickers.first() else {
            return Err(LibraryError::NotExportable {
                reason: "no tiene stickers".to_string(),
            });
        };
        let kind = first.sticker.kind();
        if incoming.stickers.len() > MAX_STICKERS
            || incoming
                .stickers
                .iter()
                .any(|new| new.sticker.kind() != kind)
        {
            return Err(LibraryError::MixedKind {
                kind: kind_name(kind),
            });
        }
        for new in &incoming.stickers {
            validate_emojis(&new.emojis)?;
        }
        let first_new = self.index.next_id;
        let ids: Vec<u32> = (0..incoming.stickers.len())
            .map(|offset| first_new.checked_add(offset as u32))
            .collect::<Option<_>>()
            .ok_or(LibraryError::OutOfIds)?;
        let mut kept_by_new = Vec::with_capacity(incoming.stickers.len());
        let old_ones = self.index.pack(pack)?.stickers.clone();
        for new in &incoming.stickers {
            let mut kept = None;
            for old in &old_ones {
                if self.read_sticker(pack, old.id)? == new.sticker.bytes() {
                    kept = Some((old.emojis.clone(), old.favorite));
                    break;
                }
            }
            kept_by_new.push(kept);
        }

        let old_icon = read_file(&self.icon_path(pack), conversor_core::TRAY_ICON_MAX_SIZE)
            .ok()
            .flatten();
        let mut written = Vec::new();
        let result = (|| {
            for (id, new) in ids.iter().zip(&incoming.stickers) {
                let path = self.sticker_path(pack, *id);
                write_atomic(&path, new.sticker.bytes()).map_err(disk("guardar el sticker"))?;
                written.push(path);
            }
            match &incoming.icon {
                Some(png) => {
                    write_atomic(&self.icon_path(pack), png).map_err(disk("guardar el icono"))?
                }
                None => self.write_icon(pack, ids[0])?,
            }
            self.apply_change(|index| {
                for _ in &ids {
                    index.new_id()?;
                }
                let stickers = ids
                    .iter()
                    .zip(&incoming.stickers)
                    .zip(&kept_by_new)
                    .map(|((id, new), kept)| {
                        let (emojis, favorite) =
                            kept.clone().unwrap_or_else(|| (new.emojis.clone(), false));
                        Sticker {
                            id: *id,
                            kind,
                            size_bytes: new.sticker.bytes().len() as u64,
                            emojis,
                            favorite,
                            media_origin: MediaOrigin::Own,
                        }
                    })
                    .collect();
                let current = index.pack_mut(pack)?;
                current.name = name;
                current.author = author;
                current.icon_from = ids.first().copied();
                current.stickers = stickers;
                current.origin = origin;
                current.modified = now();
                Ok(())
            })
        })();
        match result {
            Ok(()) => {
                for old in &old_ones {
                    delete_file(&self.sticker_path(pack, old.id), pack);
                }
                Ok(pack)
            }
            Err(error) => {
                for path in &written {
                    delete_file(path, pack);
                }
                if let Some(png) = old_icon
                    && let Err(error) = write_atomic(&self.icon_path(pack), &png)
                {
                    warn!(pack, %error, "no se pudo devolver el icono anterior");
                }
                Err(error)
            }
        }
    }

    pub(crate) fn community_ids(&self) -> Vec<(u32, String, u32)> {
        self.index
            .packs
            .iter()
            .filter_map(|pack| match &pack.origin {
                OriginUi::Community { id, version, .. } => Some((pack.id, id.clone(), *version)),
                OriginUi::Own => None,
            })
            .collect()
    }

    pub(crate) fn mark_withdrawn(
        &mut self,
        withdrawals: &[(String, String)],
    ) -> Result<Vec<u32>, LibraryError> {
        let mut affected = Vec::new();
        let mut new = self.index.clone();
        for pack in &mut new.packs {
            let OriginUi::Community { id, withdrawn, .. } = &mut pack.origin else {
                continue;
            };
            if let Some((_, reason)) = withdrawals
                .iter()
                .find(|(withdrawn_id, _)| withdrawn_id == id)
            {
                *withdrawn = Some(clean_reason(reason));
                affected.push(pack.id);
            }
        }
        if new != self.index {
            self.apply_change(|index| {
                *index = new;
                Ok(())
            })?;
        }
        Ok(affected)
    }

    pub(crate) fn set_emojis(
        &mut self,
        pack: u32,
        sticker: u32,
        emojis: &[String],
    ) -> Result<(), LibraryError> {
        validate_emojis(emojis)?;
        self.apply_change(|index| {
            let pack = index.pack_mut(pack)?;
            pack.sticker_mut(sticker)?.emojis = emojis.to_vec();
            pack.modified = now();
            Ok(())
        })
    }

    pub(crate) fn mark_favorite(
        &mut self,
        target: FavoriteTarget,
        favorite: bool,
    ) -> Result<(), LibraryError> {
        self.apply_change(|index| {
            match target {
                FavoriteTarget::Pack { id } => index.pack_mut(id)?.favorite = favorite,
                FavoriteTarget::Sticker { pack, id } => {
                    index.pack_mut(pack)?.sticker_mut(id)?.favorite = favorite;
                }
            }
            Ok(())
        })
    }

    pub(crate) fn reorder_stickers(
        &mut self,
        pack: u32,
        order: &[u32],
    ) -> Result<(), LibraryError> {
        self.apply_change(|index| {
            let pack = index.pack_mut(pack)?;
            let mut pending = std::mem::take(&mut pack.stickers);
            let mut sorted = Vec::with_capacity(pending.len());
            for id in order {
                if let Some(position) = pending.iter().position(|sticker| sticker.id == *id) {
                    sorted.push(pending.remove(position));
                }
            }
            if !pending.is_empty() || sorted.len() != order.len() {
                return Err(LibraryError::InvalidOrder);
            }
            pack.stickers = sorted;
            pack.modified = now();
            Ok(())
        })
    }

    pub(crate) fn create_pack_from_favorites(
        &mut self,
        name: &str,
    ) -> Result<Vec<u32>, LibraryError> {
        let base = normalize_pack_name(name)?;
        let mut statics = Vec::new();
        let mut animated_ones = Vec::new();
        for pack in &self.index.packs {
            for sticker in pack.stickers.iter().filter(|sticker| sticker.favorite) {
                let bytes = self.read_sticker(pack.id, sticker.id)?;
                let backup = (
                    ValidSticker::new(bytes).map_err(LibraryError::InvalidSticker)?,
                    sticker.emojis.clone(),
                    sticker.media_origin,
                );
                match sticker.kind {
                    StickerKind::Static => statics.push(backup),
                    StickerKind::Animated => animated_ones.push(backup),
                }
            }
        }
        let groups: Vec<&[(ValidSticker, Vec<String>, MediaOrigin)]> = statics
            .chunks(MAX_STICKERS)
            .chain(animated_ones.chunks(MAX_STICKERS))
            .collect();
        if groups.is_empty() {
            return Err(LibraryError::NoFavorites);
        }
        let mut created = Vec::new();
        for (number, group) in groups.iter().enumerate() {
            let title = if groups.len() == 1 {
                base.clone()
            } else {
                format!("{base} {}", number + 1)
            };
            let stickers: Vec<NewSticker> = group
                .iter()
                .map(|(sticker, emojis, media_origin)| NewSticker {
                    sticker,
                    emojis,
                    media_origin: *media_origin,
                })
                .collect();
            match self.create_with_files(&title, "", None, &stickers, OriginUi::Own) {
                Ok(id) => created.push(id),
                Err(error) => {
                    for id in created {
                        if let Err(error) = self.delete_pack(id) {
                            warn!(pack = id, %error, "no se pudo deshacer el paquete de favoritos");
                        }
                    }
                    return Err(error);
                }
            }
        }
        Ok(created)
    }

    pub(crate) fn check_proposable(&self, id: u32) -> Result<(), LibraryError> {
        let external = self
            .index
            .pack(id)?
            .stickers
            .iter()
            .any(|sticker| sticker.media_origin != MediaOrigin::Own);
        if external {
            return Err(LibraryError::HasExternalStickers);
        }
        Ok(())
    }

    pub(crate) fn to_export(&self, id: u32) -> Result<PackToExport, LibraryError> {
        let pack = self.index.pack(id)?;
        if let Some(reason) = not_exportable_reason(pack) {
            return Err(LibraryError::NotExportable { reason });
        }
        let icon = read_file(&self.icon_path(id), conversor_core::TRAY_ICON_MAX_SIZE)
            .and_then(|bytes| {
                bytes.ok_or_else(|| io::Error::new(ErrorKind::NotFound, "falta su archivo"))
            })
            .map_err(disk("leer el icono"))?;
        let stickers = pack
            .stickers
            .iter()
            .map(|sticker| self.read_sticker(id, sticker.id))
            .collect::<Result<_, _>>()?;
        Ok(PackToExport {
            title: pack.name.clone(),
            author: pack.author.clone(),
            icon,
            stickers,
            emojis: pack
                .stickers
                .iter()
                .map(|sticker| sticker.emojis.clone())
                .collect(),
        })
    }

    /// The WebP of one sticker, with a file name that says which one it is.
    pub(crate) fn sticker_to_export(
        &self,
        pack: u32,
        sticker: u32,
    ) -> Result<StickerToExport, LibraryError> {
        let current = self.index.pack(pack)?;
        let position = current
            .stickers
            .iter()
            .position(|candidate| candidate.id == sticker)
            .ok_or(LibraryError::MissingSticker { id: sticker })?;
        Ok(StickerToExport {
            pack_name: current.name.clone(),
            position: position + 1,
            bytes: self.read_sticker(pack, sticker)?,
            media_origin: current.stickers[position].media_origin,
        })
    }

    pub(crate) fn publishable_ids(&self) -> Vec<u32> {
        self.index
            .packs
            .iter()
            .filter(|pack| {
                !matches!(
                    pack.origin,
                    OriginUi::Community {
                        withdrawn: Some(_),
                        ..
                    }
                )
            })
            .map(|pack| pack.id)
            .collect()
    }

    pub(crate) fn to_publish(&self, id: u32) -> Result<PublishablePack, LibraryError> {
        self.publishable(id, false)
    }

    /// Like `to_publish`, but a pack with fewer than 3 stickers is filled up to 3 by repeating
    /// the newest one: WhatsApp rejects smaller packs. Only the published copy is repeated.
    pub(crate) fn to_publish_filled(&self, id: u32) -> Result<PublishablePack, LibraryError> {
        self.publishable(id, true)
    }

    fn publishable(&self, id: u32, fill: bool) -> Result<PublishablePack, LibraryError> {
        let pack = self.index.pack(id)?;
        let failure = checks(pack)
            .into_iter()
            .find(|check| !check.ok && !(fill && check.key == CheckKey::Count));
        if let Some(check) = failure {
            return Err(LibraryError::NotExportable { reason: check.text });
        }
        let tray = self.icon_path(id);
        if !tray.is_file() {
            return Err(disk("leer el icono")(io::Error::new(
                ErrorKind::NotFound,
                "falta su archivo",
            )));
        }
        let mut stickers: Vec<PublishableSticker> = pack
            .stickers
            .iter()
            .map(|sticker| PublishableSticker {
                path: self.sticker_path(id, sticker.id),
                animated: sticker.kind == StickerKind::Animated,
                emojis: sticker.emojis.clone(),
            })
            .collect();
        if fill && let Some(newest) = stickers.first().cloned() {
            while stickers.len() < MIN_STICKERS_TO_EXPORT {
                stickers.push(newest.clone());
            }
        }
        Ok(PublishablePack {
            id,
            name: pack.name.clone(),
            author: pack.author.clone(),
            animated: pack.kind() == Some(StickerKind::Animated),
            stickers,
            tray,
        })
    }

    /// Puts a sticker first in the automatic quick pack of its kind (one animated, one static),
    /// creating the pack if needed. At 30 stickers the oldest (last) one leaves. Returns the pack.
    /// The pack is found by its `quick` mark, not by name, so renaming it is harmless.
    pub(crate) fn add_quick_sticker(
        &mut self,
        bytes: &[u8],
        media_origin: MediaOrigin,
    ) -> Result<u32, LibraryError> {
        let kind = kind_of(&inspect_whatsapp_sticker(bytes).map_err(LibraryError::InvalidSticker)?);
        let name = match kind {
            StickerKind::Animated => QUICK_PACK_ANIMATED,
            StickerKind::Static => QUICK_PACK_STATIC,
        };
        let existing = self
            .index
            .packs
            .iter()
            .find(|pack| pack.quick == Some(kind));
        let (pack, oldest) = match existing {
            Some(pack) => (pack.id, pack.stickers.last().map(|s| s.id)),
            None => {
                let id = self.create_pack(name, None)?;
                self.apply_change(|index| {
                    index.pack_mut(id)?.quick = Some(kind);
                    Ok(())
                })?;
                (id, None)
            }
        };
        let full = self.index.pack(pack)?.stickers.len() >= MAX_STICKERS;
        if let (true, Some(oldest)) = (full, oldest) {
            self.remove_sticker(pack, oldest)?;
        }
        let added = self.add_sticker(pack, bytes, media_origin)?;
        let mut order: Vec<u32> = vec![added];
        order.extend(
            self.index
                .pack(pack)?
                .stickers
                .iter()
                .map(|sticker| sticker.id)
                .filter(|id| *id != added),
        );
        self.reorder_stickers(pack, &order)?;
        Ok(pack)
    }

    pub(crate) fn mark_published(&mut self, pack: u32, version: u32) -> Result<(), LibraryError> {
        self.apply_change(|index| {
            index.pack_mut(pack)?.published = Some(version);
            Ok(())
        })
    }

    pub(crate) fn use_as_icon(&mut self, pack: u32, sticker: u32) -> Result<(), LibraryError> {
        if !self
            .index
            .pack(pack)?
            .stickers
            .iter()
            .any(|s| s.id == sticker)
        {
            return Err(LibraryError::MissingSticker { id: sticker });
        }
        self.write_icon(pack, sticker)?;
        self.apply_change(|index| {
            let pack = index.pack_mut(pack)?;
            pack.icon_from = Some(sticker);
            pack.modified = now();
            Ok(())
        })
    }

    /// Applies `change` to a copy of the index and saves it; only if saved does it become the open
    /// library, so a disk failure never leaves memory different from the file.
    fn apply_change<T>(
        &mut self,
        change: impl FnOnce(&mut Index) -> Result<T, LibraryError>,
    ) -> Result<T, LibraryError> {
        let mut new = self.index.clone();
        let result = change(&mut new)?;
        let json = serde_json::to_vec_pretty(&new).map_err(|error| LibraryError::Disk {
            action: "preparar el índice",
            source: error.into(),
        })?;
        write_atomic(&self.root.join(INDEX_FILE), &json)
            .map_err(disk("guardar el índice de la biblioteca"))?;
        self.index = new;
        Ok(result)
    }

    pub(crate) fn create_category(&mut self, name: &str) -> Result<u32, LibraryError> {
        let name = normalize_category(name)?;
        self.apply_change(|index| {
            if index.categories.len() >= MAX_CATEGORIES {
                return Err(LibraryError::Full {
                    what: "categorías",
                    max: MAX_CATEGORIES,
                });
            }
            index.check_unique_category(&name, None)?;
            let id = index.new_id()?;
            index.categories.push(Category { id, name });
            Ok(id)
        })
    }

    pub(crate) fn rename_category(&mut self, id: u32, name: &str) -> Result<(), LibraryError> {
        let name = normalize_category(name)?;
        self.apply_change(|index| {
            index.check_category(Some(id))?;
            index.check_unique_category(&name, Some(id))?;
            for category in &mut index.categories {
                if category.id == id {
                    category.name.clone_from(&name);
                }
            }
            Ok(())
        })
    }

    pub(crate) fn delete_category(&mut self, id: u32) -> Result<(), LibraryError> {
        self.apply_change(|index| {
            index.check_category(Some(id))?;
            index.categories.retain(|category| category.id != id);
            for pack in &mut index.packs {
                if pack.category == Some(id) {
                    pack.category = None;
                }
            }
            Ok(())
        })
    }

    pub(crate) fn create_pack(
        &mut self,
        name: &str,
        category: Option<u32>,
    ) -> Result<u32, LibraryError> {
        let name = normalize_pack_name(name)?;
        self.apply_change(|index| {
            if index.packs.len() >= MAX_PACKS {
                return Err(LibraryError::Full {
                    what: "paquetes",
                    max: MAX_PACKS,
                });
            }
            index.check_category(category)?;
            let id = index.new_id()?;
            index.packs.push(Pack {
                id,
                name,
                author: String::new(),
                category,
                icon_from: None,
                favorite: false,
                modified: now(),
                published: None,
                origin: OriginUi::Own,
                quick: None,
                stickers: Vec::new(),
            });
            Ok(id)
        })
    }

    pub(crate) fn edit_pack(
        &mut self,
        id: u32,
        name: &str,
        author: &str,
        category: Option<u32>,
    ) -> Result<(), LibraryError> {
        let name = normalize_pack_name(name)?;
        let author = normalize_author(author)?;
        self.apply_change(|index| {
            index.check_category(category)?;
            let pack = index.pack_mut(id)?;
            pack.name = name;
            pack.author = author;
            pack.category = category;
            pack.modified = now();
            Ok(())
        })
    }

    pub(crate) fn delete_pack(&mut self, id: u32) -> Result<(), LibraryError> {
        self.apply_change(|index| {
            index.pack(id)?;
            index.packs.retain(|pack| pack.id != id);
            Ok(())
        })?;
        delete_directory(&self.pack_directory(id), id);
        Ok(())
    }

    pub(crate) fn ui(&mut self) -> LibraryUi {
        let categories = self
            .index
            .categories
            .iter()
            .map(|category| CategoryUi {
                id: category.id,
                name: category.name.clone(),
            })
            .collect();
        let packs = self
            .index
            .packs
            .iter()
            .map(|pack| self.pack_ui(pack))
            .collect();
        LibraryUi {
            categories,
            packs,
            notice: self.notice.take(),
        }
    }

    fn pack_ui(&self, pack: &Pack) -> PackUi {
        let directory = self.pack_directory(pack.id);
        let text = |path: PathBuf| path.to_string_lossy().into_owned();
        let checks = checks(pack);
        PackUi {
            id: pack.id,
            name: pack.name.clone(),
            author: pack.author.clone(),
            category: pack.category,
            kind: pack.kind(),
            stickers: pack
                .stickers
                .iter()
                .map(|sticker| StickerUi {
                    id: sticker.id,
                    path: text(directory.join(format!("{}.webp", sticker.id))),
                    kind: sticker.kind,
                    size_bytes: sticker.size_bytes,
                    emojis: sticker.emojis.clone(),
                    favorite: sticker.favorite,
                    media_origin: sticker.media_origin,
                })
                .collect(),
            icon: pack.icon_from.map(|_| text(directory.join(ICON_FILE))),
            exportable: checks.iter().all(|check| check.ok),
            checks,
            favorite: pack.favorite,
            modified: pack.modified,
            published: pack.published,
            origin: pack.origin.clone(),
        }
    }
}

fn checks(pack: &Pack) -> Vec<CheckUi> {
    let how_many = pack.stickers.len();
    let check = |key, ok: bool, good: String, bad: String| CheckUi {
        key,
        ok,
        text: if ok { good } else { bad },
    };
    let valid_count = (MIN_STICKERS_TO_EXPORT..=MAX_STICKERS).contains(&how_many);
    let same_kind = pack
        .stickers
        .iter()
        .all(|sticker| Some(sticker.kind) == pack.kind());
    let with_emojis = pack
        .stickers
        .iter()
        .all(|sticker| validate_emojis(&sticker.emojis).is_ok());
    let texts = normalize_pack_name(&pack.name).is_ok() && normalize_author(&pack.author).is_ok();
    let withdrawn = match &pack.origin {
        OriginUi::Community {
            withdrawn: Some(reason),
            ..
        } => Some(reason.as_str()),
        _ => None,
    };
    vec![
        check(
            CheckKey::Count,
            valid_count,
            format!("tiene {how_many} stickers (de {MIN_STICKERS_TO_EXPORT} a {MAX_STICKERS})"),
            format!(
                "necesita de {MIN_STICKERS_TO_EXPORT} a {MAX_STICKERS} stickers y tiene {how_many}"
            ),
        ),
        check(
            CheckKey::Kind,
            same_kind,
            "todos los stickers son del mismo tipo".to_string(),
            "mezcla stickers estáticos y animados".to_string(),
        ),
        check(
            CheckKey::Emojis,
            with_emojis,
            "todos los stickers tienen emoji".to_string(),
            "hay stickers sin un emoji válido".to_string(),
        ),
        check(
            CheckKey::Icon,
            pack.icon_from.is_some(),
            "tiene icono".to_string(),
            "no tiene icono".to_string(),
        ),
        check(
            CheckKey::Texts,
            texts,
            "el nombre y el autor valen".to_string(),
            "el nombre o el autor no valen".to_string(),
        ),
        check(
            CheckKey::Withdrawn,
            withdrawn.is_none(),
            "no está retirado del catálogo".to_string(),
            format!(
                "se retiró del catálogo: {}",
                withdrawn.unwrap_or("sin motivo")
            ),
        ),
    ]
}

fn not_exportable_reason(pack: &Pack) -> Option<String> {
    checks(pack)
        .into_iter()
        .find(|check| !check.ok)
        .map(|check| check.text)
}

fn has_packs(root: &Path) -> bool {
    fs::read_dir(Library::packs_directory(root))
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

fn kind_of(sticker: &WhatsappSticker) -> StickerKind {
    match sticker.kind {
        CoreKind::Static => StickerKind::Static,
        CoreKind::Animated => StickerKind::Animated,
    }
}

fn kind_name(kind: StickerKind) -> &'static str {
    match kind {
        StickerKind::Static => "estáticos",
        StickerKind::Animated => "animados",
    }
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn delete_file(path: &Path, pack: u32) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => warn!(pack, %error, "no se pudo borrar un archivo del paquete"),
    }
}

fn delete_directory(directory: &Path, pack: u32) {
    match fs::remove_dir_all(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => warn!(pack, %error, "no se pudieron borrar los archivos del paquete"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use conversor_core::{
        FramingSpec, RgbaImage, Tracking, WHATSAPP_STATIC_STICKER, convert_image,
    };
    use webp_animation::{Encoder, EncoderOptions, EncodingConfig};

    use super::*;

    pub(crate) fn animated(tone: u8) -> Vec<u8> {
        let options = EncoderOptions {
            encoding_config: Some(EncodingConfig::new_lossy(50.0)),
            ..EncoderOptions::default()
        };
        let mut encoder = Encoder::new_with_options((512, 512), options).unwrap();
        for (index, mark_ms) in [0, 200, 400].into_iter().enumerate() {
            let frame = [tone, (index * 100) as u8, 90, 255].repeat(512 * 512);
            encoder.add_frame(&frame, mark_ms).unwrap();
        }
        encoder.finalize(600).unwrap().to_vec()
    }

    pub(crate) fn static_image(tone: u8) -> Vec<u8> {
        let image = RgbaImage::new(64, 64, [tone, 30, 200, 255].repeat(64 * 64)).unwrap();
        convert_image(
            &image,
            &WHATSAPP_STATIC_STICKER,
            &FramingSpec::default(),
            Tracking::none(),
        )
        .unwrap()
        .bytes
    }

    fn first_failure(pack: &PackUi) -> Option<&str> {
        pack.checks
            .iter()
            .find(|check| !check.ok)
            .map(|check| check.text.as_str())
    }

    fn png_sides(png: &[u8]) -> (u32, u32) {
        let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
        (width, height)
    }

    fn with_stickers(
        directory: &tempfile::TempDir,
        stickers: &[Vec<u8>],
    ) -> (Library, u32, Vec<u32>) {
        let mut library = open(directory);
        let pack = library.create_pack("Gatos", None).unwrap();
        let ids = stickers
            .iter()
            .map(|bytes| library.add_sticker(pack, bytes, MediaOrigin::Own).unwrap())
            .collect();
        (library, pack, ids)
    }

    #[test]
    fn quick_pack_puts_newest_first_and_fills_to_three_when_published() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let first = static_image(10);
        let second = static_image(120);
        let pack = library.add_quick_sticker(&first, MediaOrigin::Own).unwrap();
        assert_eq!(
            library
                .add_quick_sticker(&second, MediaOrigin::Own)
                .unwrap(),
            pack
        );

        let ids: Vec<u32> = library
            .index
            .pack(pack)
            .unwrap()
            .stickers
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids.len(), 2);
        assert_eq!(library.read_sticker(pack, ids[0]).unwrap(), second);
        assert_eq!(library.read_sticker(pack, ids[1]).unwrap(), first);

        assert!(library.to_publish(pack).is_err());
        let filled = library.to_publish_filled(pack).unwrap();
        assert_eq!(filled.stickers.len(), 3);
        assert_eq!(filled.stickers[0].path, filled.stickers[2].path);
        assert_eq!(library.index.pack(pack).unwrap().stickers.len(), 2);
    }

    #[test]
    fn quick_pack_drops_the_oldest_past_thirty_and_keeps_kinds_apart() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let mut pack = 0;
        for tone in 0..31 {
            pack = library
                .add_quick_sticker(&static_image(tone * 7), MediaOrigin::Own)
                .unwrap();
        }
        let current = library.index.pack(pack).unwrap();
        assert_eq!(current.stickers.len(), MAX_STICKERS);
        let newest = current.stickers[0].id;
        assert_eq!(
            library.read_sticker(pack, newest).unwrap(),
            static_image(30 * 7)
        );
        let last = current.stickers[MAX_STICKERS - 1].id;
        assert_eq!(library.read_sticker(pack, last).unwrap(), static_image(7));

        let animated_pack = library
            .add_quick_sticker(&animated(50), MediaOrigin::Own)
            .unwrap();
        assert_ne!(animated_pack, pack);
        assert_eq!(library.index.packs.len(), 2);
        assert_eq!(
            library.index.pack(pack).unwrap().stickers.len(),
            MAX_STICKERS
        );
    }

    #[test]
    fn quick_pack_ignores_a_user_pack_with_the_same_name_and_keeps_the_origin() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let theirs = library.create_pack(QUICK_PACK_STATIC, None).unwrap();
        let quick = library
            .add_quick_sticker(&static_image(10), MediaOrigin::Giphy)
            .unwrap();
        assert_ne!(quick, theirs);
        assert!(library.index.pack(theirs).unwrap().stickers.is_empty());
        library.edit_pack(quick, "Renombrado", "", None).unwrap();
        let again = library
            .add_quick_sticker(&static_image(120), MediaOrigin::Own)
            .unwrap();
        assert_eq!(again, quick);
        let origins: Vec<MediaOrigin> = library
            .index
            .pack(quick)
            .unwrap()
            .stickers
            .iter()
            .map(|s| s.media_origin)
            .collect();
        assert_eq!(origins, [MediaOrigin::Own, MediaOrigin::Giphy]);
    }

    #[test]
    fn quick_pack_invalid_sticker_deletes_nothing_when_full() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let mut pack = 0;
        for tone in 0..MAX_STICKERS {
            pack = library
                .add_quick_sticker(&static_image(tone as u8 * 7), MediaOrigin::Own)
                .unwrap();
        }
        assert!(
            library
                .add_quick_sticker(b"no es un sticker", MediaOrigin::Own)
                .is_err()
        );
        assert_eq!(
            library.index.pack(pack).unwrap().stickers.len(),
            MAX_STICKERS
        );
    }

    #[test]
    fn export_and_import_into_another_library_returns_same_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let stickers = [static_image(10), static_image(120), static_image(240)];
        let (mut library, pack, ids) = with_stickers(&directory, &stickers);
        library.edit_pack(pack, "Gatos", "Ana", None).unwrap();
        library
            .set_emojis(pack, ids[1], &["🐱".to_string(), "👍🏽".to_string()])
            .unwrap();
        let zip = crate::wastickers::export(&library.to_export(pack).unwrap()).unwrap();
        let icon = fs::read(library.icon_path(pack)).unwrap();

        let other = tempfile::tempdir().unwrap();
        let mut new = open(&other);
        new.create_pack("Ya estaba", None).unwrap();
        let imported =
            crate::wastickers::import(&zip, "x.wastickers", &crate::mobile::Executables::default())
                .unwrap();
        let id = new.import(&imported).unwrap();

        let ui = open(&other).ui();
        let pack_ui = ui.packs.iter().find(|p| p.id == id).unwrap();
        assert_eq!(
            (pack_ui.name.as_str(), pack_ui.author.as_str()),
            ("Gatos", "Ana")
        );
        assert!(pack_ui.exportable);
        let read: Vec<Vec<u8>> = pack_ui
            .stickers
            .iter()
            .map(|s| fs::read(&s.path).unwrap())
            .collect();
        assert_eq!(read, stickers);
        let emojis: Vec<&[String]> = pack_ui
            .stickers
            .iter()
            .map(|s| s.emojis.as_slice())
            .collect();
        assert_eq!(emojis, [&["🙂"][..], &["🐱", "👍🏽"][..], &["🙂"][..]]);
        assert_eq!(fs::read(pack_ui.icon.as_ref().unwrap()).unwrap(), icon);
        assert_eq!(ui.packs.len(), 2);
    }

    #[test]
    fn import_without_icon_generates_it() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let imported = Imported {
            title: "Gatos".to_string(),
            author: String::new(),
            icon: None,
            stickers: vec![ValidSticker::new(animated(3)).unwrap()],
            emojis: vec![vec!["🐱".to_string()]],
            notices: Vec::new(),
        };
        let id = library.import(&imported).unwrap();
        assert_eq!(
            png_sides(&fs::read(library.icon_path(id)).unwrap()),
            (96, 96)
        );
        let ui = library.ui();
        assert_eq!(ui.packs[0].stickers[0].emojis, ["🐱"]);
        assert_eq!(
            first_failure(&ui.packs[0]),
            Some("necesita de 3 a 30 stickers y tiene 1")
        );
    }

    #[test]
    fn export_only_with_3_to_30_stickers() {
        let directory = tempfile::tempdir().unwrap();
        let (library, pack, _) = with_stickers(&directory, &[animated(1), animated(2)]);
        assert!(matches!(
            library.to_export(pack),
            Err(LibraryError::NotExportable { .. })
        ));
        assert!(matches!(
            library.to_export(99),
            Err(LibraryError::MissingPack { id: 99 })
        ));
    }

    #[test]
    fn to_publish_returns_paths_in_order_and_rejects_what_cannot_be_exported() {
        let directory = tempfile::tempdir().unwrap();
        let (library, pack, ids) =
            with_stickers(&directory, &[animated(1), animated(2), animated(3)]);
        let publishable = library.to_publish(pack).unwrap();
        assert!(publishable.animated);
        assert!(publishable.stickers.iter().all(|s| s.emojis == ["🙂"]));
        let expected: Vec<_> = ids
            .iter()
            .map(|id| library.sticker_path(pack, *id))
            .collect();
        let given: Vec<_> = publishable
            .stickers
            .iter()
            .map(|s| s.path.clone())
            .collect();
        assert_eq!(given, expected);
        assert_eq!(publishable.tray, library.icon_path(pack));
        assert!(publishable.tray.is_file());

        let other = tempfile::tempdir().unwrap();
        let (short, short_id, _) = with_stickers(&other, &[animated(1), animated(2)]);
        assert!(matches!(
            short.to_publish(short_id),
            Err(LibraryError::NotExportable { .. })
        ));
        assert!(matches!(
            short.to_publish(99),
            Err(LibraryError::MissingPack { id: 99 })
        ));
    }

    #[test]
    fn one_sticker_is_exported_with_its_position() {
        let directory = tempfile::tempdir().unwrap();
        let stickers = [animated(10), animated(120), animated(240)];
        let (library, pack, ids) = with_stickers(&directory, &stickers);

        let exported = library.sticker_to_export(pack, ids[1]).unwrap();

        assert_eq!(exported.bytes, stickers[1]);
        assert_eq!(exported.position, 2);
        assert!(matches!(
            library.sticker_to_export(pack, 999),
            Err(LibraryError::MissingSticker { id: 999 })
        ));
        assert!(matches!(
            library.sticker_to_export(999, ids[0]),
            Err(LibraryError::MissingPack { id: 999 })
        ));
    }

    #[test]
    fn add_three_valid_saves_files_and_icon() {
        let directory = tempfile::tempdir().unwrap();
        let stickers = [animated(10), animated(120), animated(240)];
        let (mut library, pack, ids) = with_stickers(&directory, &stickers);

        for (id, bytes) in ids.iter().zip(&stickers) {
            assert_eq!(&fs::read(library.sticker_path(pack, *id)).unwrap(), bytes);
        }
        let icon = fs::read(library.icon_path(pack)).unwrap();
        assert_eq!(png_sides(&icon), (96, 96));
        let ui = open(&directory).ui();
        let pack_ui = &ui.packs[0];
        assert_eq!(pack_ui.kind, Some(StickerKind::Animated));
        assert_eq!(pack_ui.stickers.len(), 3);
        assert_eq!(pack_ui.stickers[0].size_bytes, stickers[0].len() as u64);
        assert!(
            pack_ui.stickers[0]
                .path
                .ends_with(&format!("{}.webp", ids[0]))
        );
        assert!(pack_ui.icon.as_deref().unwrap().ends_with("tray.png"));
        assert!(pack_ui.exportable, "{pack_ui:?}");
        assert_eq!(first_failure(pack_ui), None);
        assert_eq!(library.ui(), ui);
    }

    #[test]
    fn static_in_animated_pack_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, _) = with_stickers(&directory, &[animated(10)]);

        let error = library
            .add_sticker(pack, &static_image(10), MediaOrigin::Own)
            .unwrap_err();

        assert!(matches!(error, LibraryError::MixedKind { .. }), "{error}");
        assert!(error.to_string().contains("animados"), "{error}");
        assert_eq!(library.ui().packs[0].stickers.len(), 1);
        assert_eq!(
            fs::read_dir(library.pack_directory(pack)).unwrap().count(),
            2
        );
    }

    #[test]
    fn non_whatsapp_sticker_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, _) = with_stickers(&directory, &[]);
        for bytes in [b"GIF89a".to_vec(), Vec::new(), animated(1)[..500].to_vec()] {
            let error = library
                .add_sticker(pack, &bytes, MediaOrigin::Own)
                .unwrap_err();
            assert!(matches!(error, LibraryError::InvalidSticker(_)), "{error}");
        }
        assert!(matches!(
            library.add_sticker(99, &animated(1), MediaOrigin::Own),
            Err(LibraryError::MissingPack { id: 99 })
        ));
        assert!(library.ui().packs[0].stickers.is_empty());
    }

    #[test]
    fn sticker_31_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let sticker = static_image(50);
        let (mut library, pack, _) = with_stickers(&directory, &vec![sticker.clone(); 30]);

        let error = library
            .add_sticker(pack, &sticker, MediaOrigin::Own)
            .unwrap_err();

        assert!(
            matches!(error, LibraryError::Full { max: 30, .. }),
            "{error}"
        );
        assert_eq!(open(&directory).ui().packs[0].stickers.len(), 30);
    }

    #[test]
    fn removing_icon_sticker_regenerates_icon_from_next() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, ids) = with_stickers(&directory, &[animated(10), animated(240)]);
        let icon_before = fs::read(library.icon_path(pack)).unwrap();

        library.remove_sticker(pack, ids[0]).unwrap();

        assert!(!library.sticker_path(pack, ids[0]).exists());
        let icon = fs::read(library.icon_path(pack)).unwrap();
        assert_ne!(icon, icon_before);
        assert_eq!(library.index.pack(pack).unwrap().icon_from, Some(ids[1]));

        library.remove_sticker(pack, ids[1]).unwrap();
        let ui = open(&directory).ui();
        assert_eq!((ui.packs[0].kind, ui.packs[0].icon.as_ref()), (None, None));
        assert!(!library.icon_path(pack).exists());
        assert!(matches!(
            library.remove_sticker(pack, ids[1]),
            Err(LibraryError::MissingSticker { .. })
        ));
        library
            .add_sticker(pack, &static_image(1), MediaOrigin::Own)
            .unwrap();
    }

    #[test]
    fn use_as_icon_regenerates_from_another_sticker() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, ids) = with_stickers(&directory, &[animated(10), animated(240)]);
        let icon_before = fs::read(library.icon_path(pack)).unwrap();

        library.use_as_icon(pack, ids[1]).unwrap();

        assert_ne!(fs::read(library.icon_path(pack)).unwrap(), icon_before);
        assert_eq!(open(&directory).index.packs[0].icon_from, Some(ids[1]));
        assert!(matches!(
            library.use_as_icon(pack, 999),
            Err(LibraryError::MissingSticker { id: 999 })
        ));
    }

    #[test]
    fn deleting_pack_deletes_its_stickers() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, _) = with_stickers(&directory, &[static_image(3)]);
        let files = library.pack_directory(pack);
        assert!(files.is_dir());

        library.delete_pack(pack).unwrap();

        assert!(!files.exists());
    }

    #[test]
    fn sticker_with_missing_file_is_removed_on_reopen_with_notice() {
        let directory = tempfile::tempdir().unwrap();
        let (library, pack, ids) =
            with_stickers(&directory, &[animated(10), animated(120), animated(240)]);
        fs::remove_file(library.sticker_path(pack, ids[0])).unwrap();
        fs::remove_file(library.sticker_path(pack, ids[2])).unwrap();

        let mut reopened = open(&directory);

        let ui = reopened.ui();
        let ids_ui: Vec<u32> = ui.packs[0].stickers.iter().map(|s| s.id).collect();
        assert_eq!(ids_ui, [ids[1]]);
        assert_eq!(
            ui.notice.as_deref(),
            Some("Se quitaron de la biblioteca 2 stickers cuyo archivo faltaba.")
        );
        assert_eq!(reopened.index.packs[0].icon_from, Some(ids[1]));
        assert_eq!(open(&directory).ui().notice, None);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_sticker_counts_as_lost() {
        let directory = tempfile::tempdir().unwrap();
        let (library, pack, ids) = with_stickers(&directory, &[animated(10), animated(90)]);
        let path = library.sticker_path(pack, ids[1]);
        let outside = directory.path().join("fuera.webp");
        fs::rename(&path, &outside).unwrap();
        std::os::unix::fs::symlink(&outside, &path).unwrap();

        let ui = open(&directory).ui();

        assert_eq!(ui.packs[0].stickers.len(), 1);
        assert!(ui.notice.is_some());
    }

    #[test]
    fn lost_icon_is_regenerated_on_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let (library, pack, _) = with_stickers(&directory, &[static_image(9)]);
        fs::remove_file(library.icon_path(pack)).unwrap();

        let ui = open(&directory).ui();

        assert!(library.icon_path(pack).is_file());
        assert!(ui.packs[0].icon.is_some());
        assert_eq!(ui.notice, None);
    }

    fn root(directory: &tempfile::TempDir) -> PathBuf {
        directory.path().join("library")
    }

    fn open(directory: &tempfile::TempDir) -> Library {
        Library::open(&root(directory)).unwrap()
    }

    fn write_index(directory: &tempfile::TempDir, json: &str) {
        fs::create_dir_all(root(directory)).unwrap();
        fs::write(root(directory).join(INDEX_FILE), json).unwrap();
    }

    #[test]
    fn empty_directory_returns_empty_library() {
        let directory = tempfile::tempdir().unwrap();
        let ui = open(&directory).ui();
        assert_eq!(
            ui,
            LibraryUi {
                categories: Vec::new(),
                packs: Vec::new(),
                notice: None
            }
        );
        assert!(!root(&directory).join(INDEX_FILE).exists());
    }

    #[test]
    fn created_items_survive_reopening() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let anime = library.create_category("  Anime ").unwrap();
        let gatos = library.create_pack("Gatos", Some(anime)).unwrap();
        library
            .edit_pack(gatos, "Gatos\n", " Ana ", Some(anime))
            .unwrap();
        library.create_pack("Perros", None).unwrap();
        let before = library.ui();

        let after = open(&directory).ui();

        assert_eq!(after, before);
        assert_eq!(after.categories[0].name, "Anime");
        let pack = &after.packs[0];
        assert_eq!((pack.name.as_str(), pack.author.as_str()), ("Gatos", "Ana"));
        assert_eq!(pack.category, Some(anime));
        assert_eq!(pack.kind, None);
        assert!(!pack.exportable);
        assert_eq!(
            first_failure(pack),
            Some("necesita de 3 a 30 stickers y tiene 0")
        );
        let ids: HashSet<u32> = [anime, gatos, after.packs[1].id].into();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn invalid_names_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let long = "a".repeat(129);
        for name in ["", "   ", long.as_str(), "Ga\u{7}tos", "Ga\ntos"] {
            assert!(
                matches!(
                    library.create_pack(name, None),
                    Err(LibraryError::InvalidName { .. })
                ),
                "{name:?}"
            );
        }
        assert!(library.create_pack(&"ñ".repeat(128), None).is_ok());
        let pack = library.create_pack("Gatos", None).unwrap();
        assert!(matches!(
            library.edit_pack(pack, "Gatos", &long, None),
            Err(LibraryError::InvalidName { .. })
        ));
        assert!(matches!(
            library.create_category(&"c".repeat(41)),
            Err(LibraryError::InvalidName { .. })
        ));
        library.create_category("Anime").unwrap();
        assert!(matches!(
            library.create_category("anime"),
            Err(LibraryError::RepeatedCategory)
        ));
        assert_eq!(library.ui().categories.len(), 1);
    }

    #[test]
    fn renaming_category_allows_case_change_but_not_duplicate() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let anime = library.create_category("anime").unwrap();
        library.create_category("Memes").unwrap();
        library.rename_category(anime, "Anime").unwrap();
        assert!(matches!(
            library.rename_category(anime, "MEMES"),
            Err(LibraryError::RepeatedCategory)
        ));
        assert!(matches!(
            library.rename_category(99, "Otra"),
            Err(LibraryError::MissingCategory { id: 99 })
        ));
        assert_eq!(open(&directory).ui().categories[0].name, "Anime");
    }

    #[test]
    fn deleting_category_leaves_its_packs_uncategorized() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let anime = library.create_category("Anime").unwrap();
        let pack = library.create_pack("Gatos", Some(anime)).unwrap();

        library.delete_category(anime).unwrap();

        let ui = open(&directory).ui();
        assert!(ui.categories.is_empty());
        assert_eq!(ui.packs[0].id, pack);
        assert_eq!(ui.packs[0].category, None);
    }

    #[test]
    fn missing_ids_give_error_and_change_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        assert!(matches!(
            library.create_pack("Gatos", Some(7)),
            Err(LibraryError::MissingCategory { id: 7 })
        ));
        assert!(matches!(
            library.edit_pack(3, "Gatos", "", None),
            Err(LibraryError::MissingPack { id: 3 })
        ));
        assert!(matches!(
            library.delete_pack(3),
            Err(LibraryError::MissingPack { id: 3 })
        ));
        assert!(matches!(
            library.delete_category(3),
            Err(LibraryError::MissingCategory { id: 3 })
        ));
        assert!(library.ui().packs.is_empty());
        assert!(!root(&directory).join(INDEX_FILE).exists());
    }

    #[test]
    fn deleting_pack_removes_it_and_its_directory() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let pack = library.create_pack("Gatos", None).unwrap();
        let files = library.pack_directory(pack);
        fs::create_dir_all(&files).unwrap();
        fs::write(files.join("5.webp"), b"x").unwrap();

        library.delete_pack(pack).unwrap();

        assert!(!files.exists());
        assert!(open(&directory).ui().packs.is_empty());
    }

    #[test]
    fn broken_json_sets_aside_whole_directory_and_warns() {
        let directory = tempfile::tempdir().unwrap();
        write_index(&directory, "{ esto no es json");
        let sticker = root(&directory).join("packs/3/5.webp");
        fs::create_dir_all(sticker.parent().unwrap()).unwrap();
        fs::write(&sticker, b"sticker").unwrap();

        let mut library = open(&directory);

        let ui = library.ui();
        assert!(ui.packs.is_empty());
        let notice = ui.notice.expect("tiene que avisar");
        let set_aside: Vec<PathBuf> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().contains("library.damaged-"))
            .collect();
        assert_eq!(set_aside.len(), 1, "{set_aside:?}");
        assert!(
            notice.contains(&set_aside[0].display().to_string()),
            "{notice}"
        );
        assert_eq!(
            fs::read(set_aside[0].join("packs/3/5.webp")).unwrap(),
            b"sticker"
        );
        assert_eq!(
            fs::read_to_string(set_aside[0].join(INDEX_FILE)).unwrap(),
            "{ esto no es json"
        );
        assert_eq!(library.ui().notice, None);
        library.create_pack("Nuevo", None).unwrap();
        assert_eq!(open(&directory).ui().packs.len(), 1);
    }

    #[test]
    fn newer_version_index_returns_error_and_is_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let json = r#"{"version":4,"next_id":1,"otra_cosa":[]}"#;
        write_index(&directory, json);

        let error = Library::open(&root(&directory)).unwrap_err();

        assert!(
            matches!(error, LibraryError::NewerVersion { version: 4 }),
            "{error}"
        );
        assert_eq!(
            fs::read_to_string(root(&directory).join(INDEX_FILE)).unwrap(),
            json
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn media_origin_is_saved_and_favorites_inherit_it() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let pack = library.create_pack("Mixto", None).unwrap();
        let own = library
            .add_sticker(pack, &animated(1), MediaOrigin::Own)
            .unwrap();
        let from_klipy = library
            .add_sticker(pack, &animated(2), MediaOrigin::Klipy)
            .unwrap();
        for id in [own, from_klipy] {
            library
                .mark_favorite(FavoriteTarget::Sticker { pack, id }, true)
                .unwrap();
        }
        library.create_pack_from_favorites("Favoritos").unwrap();

        let ui = open(&directory).ui();
        let origins = |index: usize| -> Vec<MediaOrigin> {
            ui.packs[index]
                .stickers
                .iter()
                .map(|sticker| sticker.media_origin)
                .collect()
        };
        assert_eq!(origins(0), [MediaOrigin::Own, MediaOrigin::Klipy]);
        assert_eq!(origins(1), [MediaOrigin::Own, MediaOrigin::Klipy]);
    }

    #[test]
    fn pack_with_external_stickers_cannot_be_proposed() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let own = library.create_pack("Propio", None).unwrap();
        library
            .add_sticker(own, &animated(1), MediaOrigin::Own)
            .unwrap();
        let external = library.create_pack("Externo", None).unwrap();
        library
            .add_sticker(external, &animated(1), MediaOrigin::Own)
            .unwrap();
        library
            .add_sticker(external, &animated(2), MediaOrigin::Giphy)
            .unwrap();

        library.check_proposable(own).unwrap();
        let error = library.check_proposable(external).unwrap_err();
        assert!(matches!(error, LibraryError::HasExternalStickers));
        assert_eq!(
            error.to_string(),
            "No se puede proponer: contiene stickers de KLIPY/GIPHY"
        );
        assert!(matches!(
            library.check_proposable(99),
            Err(LibraryError::MissingPack { id: 99 })
        ));
    }

    #[test]
    fn community_origin_survives_reopening() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let pack = library.create_pack("Gatos", None).unwrap();
        let origin = OriginUi::Community {
            id: "gatos-garabato".to_string(),
            version: 3,
            license: "CC0-1.0".to_string(),
            original_author: "Ana".to_string(),
            withdrawn: Some("Retirado por una reclamación".to_string()),
        };
        library
            .apply_change(|index| {
                let pack = index.pack_mut(pack)?;
                pack.origin = origin.clone();
                pack.published = Some(3);
                Ok(())
            })
            .unwrap();

        let ui = open(&directory).ui();

        assert_eq!(ui.packs[0].origin, origin);
        assert_eq!(ui.packs[0].published, Some(3));
    }

    #[test]
    fn non_emoji_in_index_sets_it_aside() {
        let directory = tempfile::tempdir().unwrap();
        write_index(
            &directory,
            r#"{"version":1,"next_id":9,"categories":[],
            "packs":[{"id":1,"name":"P","author":"","category":null,"icon_from":2,
            "favorite":false,"modified":1,"published":null,"origin":{"kind":"own"},
            "stickers":[{"id":2,"kind":"animated","size_bytes":10,"emojis":["x"],"favorite":false}]}]}"#,
        );
        assert!(open(&directory).ui().notice.is_some());
    }

    #[test]
    fn set_emojis_validates_and_saves() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, ids) = with_stickers(&directory, &[static_image(1)]);
        let emojis = vec!["🐱".to_string(), "👍🏽".to_string()];

        library.set_emojis(pack, ids[0], &emojis).unwrap();

        assert_eq!(open(&directory).ui().packs[0].stickers[0].emojis, emojis);
        for bad_ones in [vec![], vec!["a".to_string()], vec!["🙂".to_string(); 4]] {
            assert!(matches!(
                library.set_emojis(pack, ids[0], &bad_ones),
                Err(LibraryError::Emoji(_))
            ));
        }
        assert!(matches!(
            library.set_emojis(pack, 99, &emojis),
            Err(LibraryError::MissingSticker { id: 99 })
        ));
        assert!(matches!(
            library.set_emojis(99, ids[0], &emojis),
            Err(LibraryError::MissingPack { id: 99 })
        ));
        assert_eq!(library.ui().packs[0].stickers[0].emojis, emojis);
    }

    fn incoming(version: u32, stickers: &[(Vec<u8>, &str)]) -> IncomingPack {
        IncomingPack {
            id: "gatos-garabato".to_string(),
            version,
            name: "Gatos garabato".to_string(),
            author: "Ana".to_string(),
            license: "CC-BY-4.0".to_string(),
            original_author: "Ana Pérez".to_string(),
            icon: Some(tray_icon(&stickers[0].0).unwrap()),
            stickers: stickers
                .iter()
                .map(|(bytes, emoji)| IncomingSticker {
                    sticker: ValidSticker::new(bytes.clone()).unwrap(),
                    emojis: vec![(*emoji).to_string()],
                })
                .collect(),
        }
    }

    #[test]
    fn add_from_community_creates_pack_with_origin_and_emojis() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let pack = incoming(
            1,
            &[
                (static_image(1), "🐱"),
                (static_image(2), "🐶"),
                (static_image(3), "🐭"),
            ],
        );
        let icon = pack.icon.clone().unwrap();

        let id = library.add_from_community(pack).unwrap();

        let ui = open(&directory).ui();
        let created = &ui.packs[0];
        assert_eq!(created.id, id);
        assert_eq!(
            (created.name.as_str(), created.author.as_str()),
            ("Gatos garabato", "Ana")
        );
        assert_eq!(
            created.origin,
            OriginUi::Community {
                id: "gatos-garabato".to_string(),
                version: 1,
                license: "CC-BY-4.0".to_string(),
                original_author: "Ana Pérez".to_string(),
                withdrawn: None,
            }
        );
        let emojis: Vec<&[String]> = created
            .stickers
            .iter()
            .map(|s| s.emojis.as_slice())
            .collect();
        assert_eq!(emojis, [&["🐱"][..], &["🐶"][..], &["🐭"][..]]);
        assert!(created.exportable);
        assert_eq!(fs::read(library.icon_path(id)).unwrap(), icon);
        assert_eq!(
            library.community_ids(),
            [(id, "gatos-garabato".to_string(), 1)]
        );
    }

    #[test]
    fn add_from_community_is_all_or_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let good_ones = [
            (static_image(1), "🐱"),
            (static_image(2), "🐶"),
            (static_image(3), "🐭"),
        ];

        let mut bad_emoji = incoming(1, &good_ones);
        bad_emoji.stickers[2].emojis = vec!["texto".to_string()];
        let mut mixed = incoming(1, &good_ones);
        mixed.stickers[1].sticker = ValidSticker::new(animated(5)).unwrap();
        let mut bad_icon = incoming(1, &good_ones);
        bad_icon.icon = Some(b"no es un png".to_vec());
        let mut bad_license = incoming(1, &good_ones);
        bad_license.license = "CC\n0".to_string();
        let mut no_stickers = incoming(1, &good_ones);
        no_stickers.stickers.clear();

        for bad in [bad_emoji, mixed, bad_icon, bad_license, no_stickers] {
            assert!(library.add_from_community(bad).is_err());
        }

        assert!(library.ui().packs.is_empty());
        assert!(library.community_ids().is_empty());
        assert!(!root(&directory).join(INDEX_FILE).exists());
        let files = fs::read_dir(root(&directory).join(PACKS_DIRECTORY))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(files, 0);
    }

    #[test]
    fn updating_community_pack_keeps_user_favorites_and_emojis() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let (a, b, c, d) = (
            static_image(10),
            static_image(20),
            static_image(30),
            static_image(40),
        );
        let id = library
            .add_from_community(incoming(
                1,
                &[(a.clone(), "🐱"), (b.clone(), "🐶"), (c.clone(), "🐭")],
            ))
            .unwrap();
        let category = library.create_category("Animales").unwrap();
        library
            .edit_pack(id, "Gatos garabato", "Ana", Some(category))
            .unwrap();
        library
            .mark_favorite(FavoriteTarget::Pack { id }, true)
            .unwrap();
        library.mark_published(id, 3).unwrap();
        let old_ones: Vec<u32> = library
            .index
            .pack(id)
            .unwrap()
            .stickers
            .iter()
            .map(|s| s.id)
            .collect();
        library
            .set_emojis(id, old_ones[0], &["😻".to_string()])
            .unwrap();
        for sticker in &old_ones[1..] {
            library
                .mark_favorite(
                    FavoriteTarget::Sticker {
                        pack: id,
                        id: *sticker,
                    },
                    true,
                )
                .unwrap();
        }

        let new = library
            .add_from_community(incoming(
                2,
                &[(a.clone(), "🙀"), (c.clone(), "🙀"), (d.clone(), "🦊")],
            ))
            .unwrap();

        assert_eq!(new, id);
        let ui = open(&directory).ui();
        assert_eq!(ui.packs.len(), 1);
        let pack = &ui.packs[0];
        assert_eq!(pack.category, Some(category));
        assert!(pack.favorite);
        assert_eq!(pack.published, Some(3));
        assert!(matches!(
            pack.origin,
            OriginUi::Community { version: 2, .. }
        ));
        let state: Vec<(String, bool)> = pack
            .stickers
            .iter()
            .map(|s| (s.emojis.join(""), s.favorite))
            .collect();
        assert_eq!(
            state,
            [
                ("😻".to_string(), false),
                ("🐭".to_string(), true),
                ("🦊".to_string(), false)
            ]
        );
        let bytes: Vec<Vec<u8>> = pack
            .stickers
            .iter()
            .map(|s| fs::read(&s.path).unwrap())
            .collect();
        assert_eq!(bytes, [a, c, d]);
        assert_eq!(fs::read_dir(library.pack_directory(id)).unwrap().count(), 4);
        for old in old_ones {
            assert!(!library.sticker_path(id, old).exists());
        }
        assert_eq!(
            library.community_ids(),
            [(id, "gatos-garabato".to_string(), 2)]
        );
    }

    #[test]
    fn updating_with_invalid_data_leaves_pack_as_it_was() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let good_ones = [
            (static_image(1), "🐱"),
            (static_image(2), "🐶"),
            (static_image(3), "🐭"),
        ];
        let id = library.add_from_community(incoming(1, &good_ones)).unwrap();
        let before = library.ui();
        let mut bad = incoming(2, &good_ones);
        bad.stickers[0].emojis = vec![];

        assert!(matches!(
            library.add_from_community(bad),
            Err(LibraryError::Emoji(_))
        ));

        assert_eq!(library.ui(), before);
        assert_eq!(fs::read_dir(library.pack_directory(id)).unwrap().count(), 4);
    }

    #[test]
    fn mark_withdrawn_only_touches_named_community_packs() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        let good_ones = [
            (static_image(1), "🐱"),
            (static_image(2), "🐶"),
            (static_image(3), "🐭"),
        ];
        let withdrawn = library.add_from_community(incoming(1, &good_ones)).unwrap();
        let mut other = incoming(1, &good_ones);
        other.id = "otro-paquete".to_string();
        let other = library.add_from_community(other).unwrap();
        let own = library.create_pack("Mío", None).unwrap();

        let affected = library
            .mark_withdrawn(&[
                (
                    "gatos-garabato".to_string(),
                    "Retirado\nsin permiso\u{7}".to_string(),
                ),
                ("no-lo-tengo".to_string(), "x".to_string()),
            ])
            .unwrap();

        assert_eq!(affected, [withdrawn]);
        let ui = open(&directory).ui();
        assert_eq!(ui.notice, None);
        let origin = |id: u32| ui.packs.iter().find(|p| p.id == id).unwrap().origin.clone();
        assert!(matches!(
            origin(withdrawn),
            OriginUi::Community { withdrawn: Some(ref reason), .. } if reason == "Retirado sin permiso"
        ));
        assert!(matches!(
            origin(other),
            OriginUi::Community {
                withdrawn: None,
                ..
            }
        ));
        assert_eq!(origin(own), OriginUi::Own);
        let pack = ui.packs.iter().find(|p| p.id == withdrawn).unwrap();
        assert!(!pack.exportable);
        assert_eq!(
            library
                .mark_withdrawn(&[(
                    "gatos-garabato".to_string(),
                    "Retirado\nsin permiso".to_string()
                )])
                .unwrap(),
            [withdrawn]
        );
        assert_eq!(library.mark_withdrawn(&[]).unwrap(), Vec::<u32>::new());
    }

    #[test]
    fn mark_published_saves_version() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, _) = with_stickers(&directory, &[]);
        library.mark_published(pack, 4).unwrap();
        assert_eq!(open(&directory).ui().packs[0].published, Some(4));
        assert!(matches!(
            library.mark_published(99, 1),
            Err(LibraryError::MissingPack { id: 99 })
        ));
    }

    #[test]
    fn mark_favorite_on_pack_and_sticker() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, ids) =
            with_stickers(&directory, &[static_image(1), static_image(2)]);

        library
            .mark_favorite(FavoriteTarget::Pack { id: pack }, true)
            .unwrap();
        library
            .mark_favorite(FavoriteTarget::Sticker { pack, id: ids[1] }, true)
            .unwrap();

        let ui = open(&directory).ui();
        assert!(ui.packs[0].favorite);
        let favorites: Vec<bool> = ui.packs[0].stickers.iter().map(|s| s.favorite).collect();
        assert_eq!(favorites, [false, true]);
        library
            .mark_favorite(FavoriteTarget::Pack { id: pack }, false)
            .unwrap();
        assert!(!library.ui().packs[0].favorite);
        assert!(matches!(
            library.mark_favorite(FavoriteTarget::Pack { id: 99 }, true),
            Err(LibraryError::MissingPack { id: 99 })
        ));
        assert!(matches!(
            library.mark_favorite(FavoriteTarget::Sticker { pack, id: 99 }, true),
            Err(LibraryError::MissingSticker { id: 99 })
        ));
    }

    #[test]
    fn reorder_accepts_a_permutation_and_rejects_the_rest() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, ids) = with_stickers(
            &directory,
            &[static_image(1), static_image(2), static_image(3)],
        );
        let order_ui = |library: &mut Library| -> Vec<u32> {
            library.ui().packs[0]
                .stickers
                .iter()
                .map(|s| s.id)
                .collect()
        };

        library
            .reorder_stickers(pack, &[ids[2], ids[0], ids[1]])
            .unwrap();

        assert_eq!(order_ui(&mut library), [ids[2], ids[0], ids[1]]);
        assert_eq!(order_ui(&mut open(&directory)), [ids[2], ids[0], ids[1]]);
        let bad_ones: [&[u32]; 5] = [
            &[],
            &[ids[0], ids[1]],
            &[ids[0], ids[0], ids[1]],
            &[ids[0], ids[1], ids[2], ids[2]],
            &[ids[0], ids[1], 999],
        ];
        for order in bad_ones {
            assert!(
                matches!(
                    library.reorder_stickers(pack, order),
                    Err(LibraryError::InvalidOrder)
                ),
                "{order:?}"
            );
        }
        assert_eq!(order_ui(&mut library), [ids[2], ids[0], ids[1]]);
        assert!(matches!(
            library.reorder_stickers(99, &[]),
            Err(LibraryError::MissingPack { id: 99 })
        ));
    }

    fn with_favorites(library: &mut Library, name: &str, how_many: usize, kind: StickerKind) {
        let pack = library.create_pack(name, None).unwrap();
        for number in 0..how_many {
            let bytes = match kind {
                StickerKind::Static => static_image((number % 250) as u8),
                StickerKind::Animated => animated((number % 250) as u8),
            };
            let id = library.add_sticker(pack, &bytes, MediaOrigin::Own).unwrap();
            library
                .mark_favorite(FavoriteTarget::Sticker { pack, id }, true)
                .unwrap();
        }
    }

    #[test]
    fn thirty_five_favorites_give_two_packs_and_keep_emojis() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        with_favorites(&mut library, "A", 30, StickerKind::Static);
        with_favorites(&mut library, "B", 5, StickerKind::Static);
        let a = library.publishable_ids()[0];
        let first = library.ui().packs[0].stickers[0].id;
        library.set_emojis(a, first, &["🐱".to_string()]).unwrap();

        let created = library.create_pack_from_favorites("Mis favoritos").unwrap();

        assert_eq!(created.len(), 2);
        let ui = open(&directory).ui();
        assert_eq!(ui.packs.len(), 4);
        let new_ones: Vec<&PackUi> = ui
            .packs
            .iter()
            .filter(|p| created.contains(&p.id))
            .collect();
        assert_eq!(new_ones[0].id, created[0]);
        assert_eq!(new_ones[0].name, "Mis favoritos 1");
        assert_eq!(new_ones[1].name, "Mis favoritos 2");
        assert_eq!(
            (new_ones[0].stickers.len(), new_ones[1].stickers.len()),
            (30, 5)
        );
        assert_eq!(new_ones[0].stickers[0].emojis, ["🐱"]);
        assert!(
            new_ones
                .iter()
                .all(|p| p.origin == OriginUi::Own && p.icon.is_some())
        );
        assert!(new_ones[0].stickers.iter().all(|s| !s.favorite));
    }

    #[test]
    fn single_kind_favorites_give_one_pack_and_mixed_are_split() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        with_favorites(&mut library, "A", 3, StickerKind::Static);
        let one = library.create_pack_from_favorites("Favs").unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(library.ui().packs.last().unwrap().name, "Favs");

        with_favorites(&mut library, "B", 3, StickerKind::Animated);
        let two = library.create_pack_from_favorites("Otros").unwrap();
        assert_eq!(two.len(), 2);
    }

    #[test]
    fn no_favorites_or_bad_name_creates_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, _, _) = with_stickers(&directory, &[static_image(1)]);
        assert!(matches!(
            library.create_pack_from_favorites("Favs"),
            Err(LibraryError::NoFavorites)
        ));
        assert!(matches!(
            library.create_pack_from_favorites(""),
            Err(LibraryError::InvalidName { .. })
        ));
        assert_eq!(library.ui().packs.len(), 1);
    }

    #[test]
    fn checks_say_what_is_missing_and_action_is_disabled() {
        let directory = tempfile::tempdir().unwrap();
        let (mut library, pack, _) = with_stickers(&directory, &[static_image(1), static_image(2)]);
        let keys = |ui: &PackUi| -> Vec<(CheckKey, bool)> {
            ui.checks.iter().map(|c| (c.key, c.ok)).collect()
        };

        let short = library.ui().packs[0].clone();
        assert!(!short.exportable);
        assert_eq!(
            keys(&short),
            [
                (CheckKey::Count, false),
                (CheckKey::Kind, true),
                (CheckKey::Emojis, true),
                (CheckKey::Icon, true),
                (CheckKey::Texts, true),
                (CheckKey::Withdrawn, true),
            ]
        );

        library
            .add_sticker(pack, &static_image(3), MediaOrigin::Own)
            .unwrap();
        assert!(library.ui().packs[0].exportable);

        library
            .apply_change(|index| {
                index.pack_mut(pack)?.origin = OriginUi::Community {
                    id: "gatos".to_string(),
                    version: 1,
                    license: "CC0-1.0".to_string(),
                    original_author: "Ana".to_string(),
                    withdrawn: Some("derechos de autor".to_string()),
                };
                Ok(())
            })
            .unwrap();
        let withdrawn = library.ui().packs[0].clone();
        assert!(!withdrawn.exportable);
        assert_eq!(
            first_failure(&withdrawn),
            Some("se retiró del catálogo: derechos de autor")
        );
        assert!(matches!(
            library.to_export(pack),
            Err(LibraryError::NotExportable { .. })
        ));
    }

    #[test]
    fn index_over_2_mb_is_set_aside() {
        let directory = tempfile::tempdir().unwrap();
        let mut json = String::from(r#"{"version":1,"next_id":1,"categories":[],"packs":[]}"#);
        json.push_str(&" ".repeat(3_000_000));
        write_index(&directory, &json);

        let ui = open(&directory).ui();

        assert!(ui.notice.unwrap().contains("demasiado grande"));
        assert!(!root(&directory).join(INDEX_FILE).exists());
    }

    #[test]
    fn inconsistent_index_is_set_aside() {
        let cases = [
            r#"{"version":1,"next_id":5,"categories":[{"id":1,"name":"A"}],
                "packs":[{"id":1,"name":"P","author":"","category":null,"icon_from":null,"stickers":[]}]}"#,
            r#"{"version":1,"next_id":2,"categories":[{"id":3,"name":"A"}],"packs":[]}"#,
            r#"{"version":1,"next_id":5,"categories":[],
                "packs":[{"id":1,"name":"P","author":"","category":4,"icon_from":null,"stickers":[]}]}"#,
            r#"{"version":1,"next_id":5,"categories":[{"id":1,"name":"A\u0007"}],"packs":[]}"#,
            r#"{"version":1,"next_id":5,"categories":[{"id":1,"name":"A"},{"id":2,"name":"a"}],"packs":[]}"#,
            r#"{"version":1,"next_id":5,"categories":[],"packs":[],"path":"/etc"}"#,
            r#"{"version":0,"next_id":5,"categories":[],"packs":[]}"#,
            r#"{"version":1,"next_id":9,"categories":[],
                "packs":[{"id":1,"name":"P","author":"","category":null,"icon_from":2,"stickers":[
                {"id":2,"kind":"animated","size_bytes":10},{"id":3,"kind":"static","size_bytes":10}]}]}"#,
            r#"{"version":1,"next_id":9,"categories":[],
                "packs":[{"id":1,"name":"P","author":"","category":null,"icon_from":7,"stickers":[
                {"id":2,"kind":"animated","size_bytes":10}]}]}"#,
        ];
        for json in cases {
            let directory = tempfile::tempdir().unwrap();
            write_index(&directory, json);
            let ui = open(&directory).ui();
            assert!(ui.notice.is_some(), "{json}");
            assert!(ui.packs.is_empty() && ui.categories.is_empty(), "{json}");
        }
    }

    #[test]
    fn no_index_but_with_packs_is_set_aside() {
        let directory = tempfile::tempdir().unwrap();
        let sticker = root(&directory).join("packs/3/5.webp");
        fs::create_dir_all(sticker.parent().unwrap()).unwrap();
        fs::write(&sticker, b"sticker").unwrap();

        let ui = open(&directory).ui();

        assert!(ui.notice.unwrap().contains("falta el índice"));
        assert!(!sticker.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_index_is_not_followed() {
        let directory = tempfile::tempdir().unwrap();
        let outside = directory.path().join("fuera.json");
        fs::write(
            &outside,
            r#"{"version":1,"next_id":1,"categories":[],"packs":[]}"#,
        )
        .unwrap();
        fs::create_dir_all(root(&directory)).unwrap();
        std::os::unix::fs::symlink(&outside, root(&directory).join(INDEX_FILE)).unwrap();

        let ui = open(&directory).ui();

        assert!(ui.notice.unwrap().contains("no es un archivo normal"));
        assert!(outside.exists());
    }

    #[test]
    fn at_most_50_categories_and_200_packs() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        for number in 0..MAX_CATEGORIES {
            library.create_category(&format!("c{number}")).unwrap();
        }
        assert!(matches!(
            library.create_category("otra"),
            Err(LibraryError::Full { max: 50, .. })
        ));
        for number in 0..MAX_PACKS {
            library.create_pack(&format!("p{number}"), None).unwrap();
        }
        assert!(matches!(
            library.create_pack("otro", None),
            Err(LibraryError::Full { max: 200, .. })
        ));
        let ui = open(&directory).ui();
        assert_eq!(
            (ui.categories.len(), ui.packs.len(), ui.notice),
            (50, 200, None)
        );
    }

    #[test]
    fn memory_unchanged_if_saving_fails() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = open(&directory);
        fs::create_dir_all(root(&directory).join(INDEX_FILE)).unwrap();

        let error = library.create_pack("Gatos", None).unwrap_err();

        assert!(matches!(error, LibraryError::Disk { .. }), "{error}");
        assert!(library.ui().packs.is_empty());
    }
}
