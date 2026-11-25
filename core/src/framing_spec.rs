use crate::error::FramingError;

pub const MAX_ZOOM: f64 = 4.0;

pub const MIN_SCALE: f64 = 0.5;

/// Corner radius of the rounded shape, as a fraction of the side of the drawn content.
const ROUNDED_RADIUS: f64 = 0.22;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FramingMode {
    #[default]
    Crop,
    Fit,
}

/// What fills the empty bands of a fitted frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Background {
    #[default]
    Transparent,
    Color([u8; 3]),
    Blur,
}

/// Mask applied to the drawn content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Square,
    Rounded,
    Circle,
}

/// Vertical placement of a fitted frame inside its square.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Align {
    Top,
    #[default]
    Center,
    Bottom,
}

/// How a source frame becomes a square: a crop (centre and zoom are fractions of the
/// source frame) or the whole frame fitted in a square. The result is masked by `shape` and
/// drawn at `scale` times the canvas, centred on a transparent canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FramingSpec {
    mode: FramingMode,
    center_x: f64,
    center_y: f64,
    zoom: f64,
    background: Background,
    shape: Shape,
    scale: f64,
    align: Align,
}

impl Default for FramingSpec {
    fn default() -> Self {
        FramingSpec {
            mode: FramingMode::Crop,
            center_x: 0.5,
            center_y: 0.5,
            zoom: 1.0,
            background: Background::Transparent,
            shape: Shape::Square,
            scale: 1.0,
            align: Align::Center,
        }
    }
}

impl FramingSpec {
    pub fn new(
        mode: FramingMode,
        center_x: f64,
        center_y: f64,
        zoom: f64,
    ) -> Result<Self, FramingError> {
        let unit = 0.0..=1.0;
        let valid = unit.contains(&center_x)
            && unit.contains(&center_y)
            && (1.0..=MAX_ZOOM).contains(&zoom);
        if !valid {
            return Err(FramingError::NotValid);
        }
        Ok(FramingSpec {
            mode,
            center_x,
            center_y,
            zoom,
            ..FramingSpec::default()
        })
    }

    pub fn fit() -> Self {
        FramingSpec {
            mode: FramingMode::Fit,
            ..FramingSpec::default()
        }
    }

    /// Sets how the content looks: background (only used when fitting), shape mask, size
    /// relative to the canvas (`MIN_SCALE..=1`) and vertical placement (only used when fitting).
    pub fn with_style(
        self,
        background: Background,
        shape: Shape,
        scale: f64,
        align: Align,
    ) -> Result<Self, FramingError> {
        if !(MIN_SCALE..=1.0).contains(&scale) {
            return Err(FramingError::ScaleNotValid);
        }
        Ok(FramingSpec {
            background,
            shape,
            scale,
            align,
            ..self
        })
    }

    pub fn mode(&self) -> FramingMode {
        self.mode
    }

    pub(crate) fn background(&self) -> Background {
        self.background
    }

    pub(crate) fn shape(&self) -> Shape {
        self.shape
    }

    pub(crate) fn align(&self) -> Align {
        self.align
    }

    /// Side of the drawn content inside a canvas of `canvas_side`.
    pub(crate) fn content_side(&self, canvas_side: u32) -> u32 {
        let side = (f64::from(canvas_side) * self.scale).round() as u32;
        side.clamp(1, canvas_side.max(1))
    }

    /// Square crop `(x, y, side)` in source pixels. The centre is moved when the
    /// square would otherwise leave the frame.
    pub(crate) fn crop_rect(&self, width: u32, height: u32) -> (f64, f64, f64) {
        let (width, height) = (f64::from(width), f64::from(height));
        let side = (width.min(height) / self.zoom).max(1.0);
        let x = (self.center_x * width - side / 2.0).clamp(0.0, width - side);
        let y = (self.center_y * height - side / 2.0).clamp(0.0, height - side);
        (x, y, side)
    }

    /// ffmpeg filters that turn each frame into a `canvas_side` square RGBA canvas, to run
    /// after the frame rate filter and with nothing after them.
    pub(crate) fn canvas_filter(&self, canvas_side: u32) -> String {
        let content = self.content_side(canvas_side);
        let mut filters = match self.mode {
            FramingMode::Crop => {
                let side = format!("max(1\\,floor(min(iw\\,ih)/{:.6}))", self.zoom);
                format!(
                    "crop=w={side}:h={side}:\
                     x=max(0\\,min(iw-ow\\,{:.6}*iw-ow/2)):\
                     y=max(0\\,min(ih-oh\\,{:.6}*ih-oh/2)),\
                     scale={content}:{content}:flags=lanczos",
                    self.center_x, self.center_y
                )
            }
            FramingMode::Fit => self.fit_filter(content),
        };
        if let Some(mask) = mask_filter(self.shape, content) {
            filters.push(',');
            filters.push_str(&mask);
        }
        if content < canvas_side {
            let margin = (canvas_side - content) / 2;
            filters.push_str(&format!(
                ",pad={canvas_side}:{canvas_side}:{margin}:{margin}:color=black@0"
            ));
        }
        filters.push_str(",format=rgba");
        filters
    }

    fn fit_filter(&self, content: u32) -> String {
        let (pad_y, overlay_y) = match self.align {
            Align::Top => ("0", "0"),
            Align::Center => ("(oh-ih)/2", "(H-h)/2"),
            Align::Bottom => ("oh-ih", "H-h"),
        };
        let fitted =
            format!("scale={content}:{content}:force_original_aspect_ratio=decrease:flags=lanczos");
        match self.background {
            Background::Transparent => format!(
                "format=rgba,{fitted},\
                 pad={content}:{content}:(ow-iw)/2:{pad_y}:color=black@0"
            ),
            Background::Color([r, g, b]) => format!(
                "{fitted},pad={content}:{content}:(ow-iw)/2:{pad_y}:color=0x{r:02x}{g:02x}{b:02x},\
                 format=rgba"
            ),
            Background::Blur => {
                let radius = (content / 12).max(1);
                format!(
                    "split[cover][whole];\
                     [cover]scale={content}:{content}:force_original_aspect_ratio=increase,\
                     crop={content}:{content},format=gbrp,\
                     gblur=sigma={radius}[blurred];\
                     [whole]{fitted}[sharp];\
                     [blurred][sharp]overlay=x=(W-w)/2:y={overlay_y},format=rgba"
                )
            }
        }
    }
}

/// Alpha of a pixel inside a shape drawn in a square of `side` pixels: 1 inside, 0 outside and a
/// one-pixel ramp on the edge. `x` and `y` are pixel indexes.
pub(crate) fn shape_coverage(shape: Shape, side: u32, x: u32, y: u32) -> f64 {
    let side = f64::from(side);
    let half = side / 2.0;
    let dx = (f64::from(x) + 0.5 - half).abs();
    let dy = (f64::from(y) + 0.5 - half).abs();
    let distance = match shape {
        Shape::Square => return 1.0,
        Shape::Circle => dx.hypot(dy) - half,
        Shape::Rounded => {
            let radius = ROUNDED_RADIUS * side;
            let (qx, qy) = (dx - (half - radius), dy - (half - radius));
            qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
        }
    };
    (0.5 - distance).clamp(0.0, 1.0)
}

/// The same coverage as `shape_coverage`, as an ffmpeg `geq` over an RGBA square.
fn mask_filter(shape: Shape, side: u32) -> Option<String> {
    let half = f64::from(side) / 2.0;
    let distance = match shape {
        Shape::Square => return None,
        Shape::Circle => format!("hypot(abs(X+0.5-{half:.3}),abs(Y+0.5-{half:.3}))-{half:.3}"),
        Shape::Rounded => {
            let radius = ROUNDED_RADIUS * f64::from(side);
            let inner = half - radius;
            let qx = format!("(abs(X+0.5-{half:.3})-{inner:.3})");
            let qy = format!("(abs(Y+0.5-{half:.3})-{inner:.3})");
            format!("hypot(max({qx},0),max({qy},0))+min(max({qx},{qy}),0)-{radius:.3}")
        }
    };
    Some(format!(
        "format=gbrap,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':\
         a='alpha(X,Y)*clip(0.5-({distance}),0,1)'"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_the_centred_whole_square() {
        let spec = FramingSpec::default();
        assert_eq!(spec.crop_rect(640, 360), (140.0, 0.0, 360.0));
    }

    #[test]
    fn zoom_2_crops_half_the_side() {
        let spec = FramingSpec::new(FramingMode::Crop, 0.5, 0.5, 2.0).unwrap();
        assert_eq!(spec.crop_rect(640, 360), (230.0, 90.0, 180.0));
    }

    #[test]
    fn centre_is_clamped_so_the_square_stays_inside() {
        let spec = FramingSpec::new(FramingMode::Crop, 1.0, 0.0, 2.0).unwrap();
        assert_eq!(spec.crop_rect(640, 360), (460.0, 0.0, 180.0));
    }

    #[test]
    fn rejects_values_out_of_range() {
        for (x, y, zoom) in [
            (-0.1, 0.5, 1.0),
            (0.5, 1.1, 1.0),
            (0.5, 0.5, 0.9),
            (0.5, 0.5, 4.1),
            (f64::NAN, 0.5, 1.0),
            (0.5, f64::INFINITY, 1.0),
            (0.5, 0.5, f64::NAN),
        ] {
            assert_eq!(
                FramingSpec::new(FramingMode::Crop, x, y, zoom),
                Err(FramingError::NotValid),
                "{x} {y} {zoom}"
            );
        }
    }

    #[test]
    fn ffmpeg_filters_follow_the_mode() {
        assert!(
            FramingSpec::fit()
                .canvas_filter(512)
                .starts_with("format=rgba,scale=512:512:force_original_aspect_ratio=decrease")
        );
        let crop = FramingSpec::new(FramingMode::Crop, 0.25, 0.75, 2.0).unwrap();
        let filter = crop.canvas_filter(512);
        assert!(
            filter.starts_with("crop=w=max(1\\,floor(min(iw\\,ih)/2.000000))"),
            "{filter}"
        );
        assert!(filter.contains("0.250000*iw"), "{filter}");
        assert!(filter.contains("0.750000*ih"), "{filter}");
    }

    #[test]
    fn scale_outside_half_to_one_is_rejected() {
        for scale in [0.49, 1.01, f64::NAN, 0.0] {
            assert_eq!(
                FramingSpec::fit().with_style(
                    Background::Transparent,
                    Shape::Square,
                    scale,
                    Align::Center
                ),
                Err(FramingError::ScaleNotValid),
                "{scale}"
            );
        }
    }

    #[test]
    fn content_side_follows_the_scale() {
        let half = FramingSpec::fit()
            .with_style(Background::Blur, Shape::Circle, 0.5, Align::Top)
            .unwrap();
        assert_eq!(half.content_side(512), 256);
        assert_eq!(FramingSpec::fit().content_side(512), 512);
        assert!(half.canvas_filter(512).contains("pad=512:512:128:128"));
    }

    #[test]
    fn shape_coverage_is_one_inside_and_zero_in_the_corners() {
        for shape in [Shape::Rounded, Shape::Circle] {
            assert_eq!(shape_coverage(shape, 100, 50, 50), 1.0);
            assert_eq!(shape_coverage(shape, 100, 0, 0), 0.0);
        }
        assert!(shape_coverage(Shape::Circle, 100, 50, 0) > 0.0);
        assert_eq!(shape_coverage(Shape::Rounded, 100, 50, 0), 1.0);
        assert_eq!(shape_coverage(Shape::Square, 100, 0, 0), 1.0);
    }
}
