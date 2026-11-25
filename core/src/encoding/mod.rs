mod apng;
mod gif;
mod webp;

pub(crate) use apng::encode_apng;
pub use gif::{GifEncoder, GifNeuQuant};
pub use webp::{MAX_WEBP_QUALITY, encode_thumbnail_webp};
pub(crate) use webp::{
    encode_static_webp, encode_webp, encode_webp_cancelable, read_chunk, read_u24,
};
