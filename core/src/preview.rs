use crate::clip::Clip;
use crate::duplicates::remove_near_duplicates_scaled;
use crate::encoding::encode_webp;
use crate::error::PreviewError;
use crate::scaling::{Framing, scale_frames};

pub const PREVIEW_SIDE: u32 = 160;

const PREVIEW_QUALITY: u8 = 40;

pub(crate) fn encode_preview(clip: &Clip) -> Result<Vec<u8>, PreviewError> {
    let framing = Framing::new(PREVIEW_SIDE, PREVIEW_SIDE, PREVIEW_SIDE);
    let scaling = scale_frames(clip.side(), clip.frames(), framing)?;
    let without_duplicates = remove_near_duplicates_scaled(scaling);
    Ok(encode_webp(
        &without_duplicates,
        PREVIEW_QUALITY,
        false,
        None,
    )?)
}

#[cfg(test)]
mod tests {
    use webp_animation::Decoder;

    use super::*;
    use crate::clip::Frame;

    #[test]
    fn is_webp_of_160_with_duration_of_clip_and_without_duplicates() {
        let side = 512;
        let frame = |gray: u8| Frame {
            rgba: [gray, gray, gray, 255].repeat(side * side).into(),
            duration_ms: 100,
        };
        let clip = Clip::new(512, vec![frame(0), frame(0), frame(200)]).unwrap();
        let webp = encode_preview(&clip).unwrap();

        let decoder = Decoder::new(&webp).unwrap();
        assert_eq!(decoder.dimensions(), (160, 160));
        let marks: Vec<i32> = decoder.into_iter().map(|f| f.timestamp()).collect();
        assert_eq!(marks, [200, 300]);
    }
}
