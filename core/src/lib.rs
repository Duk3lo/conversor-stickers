//! Converter core: turns a local video into a sticker or GIF, and an image into a
//! static sticker, following the rules of each preset.

mod catalog;
mod clip;
mod conversion;
mod decoding;
mod denoise;
mod download;
mod duplicates;
mod emojis;
mod encoding;
mod error;
mod framing_spec;
mod image;
mod options;
mod palette;
mod preset;
mod preview;
mod process;
mod scaling;
mod size_fitting;
mod sticker_whatsapp;
mod texts;
mod time;
mod tracking;

pub use catalog::{
    ALLOWED_LICENSES, Catalog, CatalogError, Category, ICON_FILE, INDEX_FORMAT, IconSource, Index,
    IndexFile, IndexPack, IndexSticker, MAX_ICON_SIZE_BYTES, MAX_INDEX_SIZE_BYTES, MAX_PACKS,
    MAX_STICKER_SIZE_BYTES, MAX_STICKERS, MAX_TAGS, MIN_STICKERS, PACK_FILE, PackSource,
    StickerSource, Withdrawn, generate_index, is_sticker_file, is_valid_id, read_index,
    serialize_index, sha256_hex, validate_catalog, validate_index, validate_source_pack,
};
pub use clip::{Clip, Frame, ScaledClip};
pub use conversion::{Estimate, Report, convert, estimate, plan};
pub use decoding::{
    DEFAULT_TIME_LIMIT, Decoder, FfmpegDecoder, ImageInfo, MAX_DECODED_SHORT_SIDE,
    MAX_IMAGE_SIZE_BYTES, MAX_SOUND_SECONDS, MAX_VIDEO_SIDE_PX, MAX_VIDEO_SIZE_BYTES,
    SoundContainer, VideoInfo,
};
pub use download::{
    DOWNLOAD_TIME_LIMIT, DownloadMode, DownloadProgress, DownloadedVideo, Link,
    SAVE_MAX_SIZE_BYTES, SAVE_TIME_LIMIT, YtDlpDownloader, looks_like_link,
};
pub use emojis::{DEFAULT_EMOJI, EmojiError, validate_emojis};
pub use encoding::{GifEncoder, GifNeuQuant, MAX_WEBP_QUALITY, encode_thumbnail_webp};
pub use error::{
    ClipError, ConversionError, DecodingError, DownloadError, EncodingError, FramingError,
    ImageError, OptionsError, PreviewError, ScalingError, SizeFittingError, SpeedError,
    StickerError,
};
pub use framing_spec::{Align, Background, FramingMode, FramingSpec, MAX_ZOOM, MIN_SCALE, Shape};
pub use image::{
    IMAGE_PRESETS, ImagePreset, ImageReport, RgbaImage, WHATSAPP_STATIC_STICKER, convert_image,
    find_image_preset,
};
pub use options::{ConversionOptions, ConversionParams, Fps, MaxSize, Priority, Speed};
pub use preset::{
    Canvas, DISCORD_EMOJI, DISCORD_GIF, DISCORD_STICKER, Format, MaxMbRange, PRESETS, Preset,
    WHATSAPP_STICKER, find_preset,
};
pub use preview::PREVIEW_SIDE;
pub use size_fitting::{Attempt, Knob, Ratio, Reduction};
pub use sticker_whatsapp::{
    StickerKind, TRAY_ICON_MAX_SIZE, TRAY_ICON_SIDE, WhatsappSticker, inspect_tray_icon,
    inspect_whatsapp_sticker, tray_icon,
};
pub use texts::{decimal, decimal_1, readable_size};
pub use time::{TimePlan, plan_time};
pub use tracking::{PreviewReceiver, Progress, Tracking};
