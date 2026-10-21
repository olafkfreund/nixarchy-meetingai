//! Screen capture primitives.
//!
//! Coordinates arriving from the UI are already monitor-relative **physical**
//! pixels: the caller multiplies by the device pixel ratio, so nothing here
//! rescales them again. Cropping, bounds checks and encoding are pure functions
//! so they can be tested without a display.

use std::io::Cursor;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::{ImageFormat, RgbaImage};

/// Longest edge kept when an image is sent to a model.
///
/// Screenshots are downscaled to this before encoding: a full 4K desktop would
/// otherwise cost far more tokens than an answer needs.
pub const MAX_EDGE: u32 = 1920;

/// Errors from capture handling.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture region is empty or outside the selected monitor")]
    Bounds,
    #[error("the monitor has no usable area")]
    NoMonitor,
    #[error("image encoding failed: {0}")]
    Encode(String),
}

/// A region in monitor-relative physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Whether this region fits inside an image of `width` × `height`.
    ///
    /// Uses checked addition so an overflowing region is rejected instead of
    /// wrapping into a nonsensical crop.
    pub fn fits_in(self, width: u32, height: u32) -> bool {
        self.width > 0
            && self.height > 0
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|right| right <= width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|bottom| bottom <= height)
    }
}

/// An encoded image ready to attach to a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedImage {
    /// Raw base64 bytes without a data-URL prefix.
    pub base64: String,
    pub mime: &'static str,
}

/// Crop a region out of a full-frame screenshot.
pub fn crop(image: &RgbaImage, region: Region) -> Result<RgbaImage, CaptureError> {
    if !region.fits_in(image.width(), image.height()) {
        return Err(CaptureError::Bounds);
    }
    Ok(
        image::imageops::crop_imm(image, region.x, region.y, region.width, region.height)
            .to_image(),
    )
}

/// Shrink so the longest edge is at most [`MAX_EDGE`], preserving aspect ratio.
pub fn downscale(image: RgbaImage) -> RgbaImage {
    let longest = image.width().max(image.height());
    if longest <= MAX_EDGE {
        return image;
    }
    let scale = MAX_EDGE as f32 / longest as f32;
    let width = ((image.width() as f32 * scale).round() as u32).max(1);
    let height = ((image.height() as f32 * scale).round() as u32).max(1);
    image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle)
}

/// Encode an image as base64 PNG or JPEG.
///
/// JPEG has no alpha channel, so it is flattened to RGB first; encoding RGBA
/// directly is rejected by the encoder.
pub fn encode(image: &RgbaImage, png: bool) -> Result<CapturedImage, CaptureError> {
    let mut bytes = Cursor::new(Vec::new());
    if png {
        image
            .write_to(&mut bytes, ImageFormat::Png)
            .map_err(|error| CaptureError::Encode(error.to_string()))?;
    } else {
        let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
        rgb.write_to(&mut bytes, ImageFormat::Jpeg)
            .map_err(|error| CaptureError::Encode(error.to_string()))?;
    }
    Ok(CapturedImage {
        base64: STANDARD.encode(bytes.into_inner()),
        mime: if png { "image/png" } else { "image/jpeg" },
    })
}

/// Full pipeline: crop when a region is given, downscale, then encode.
pub fn capture_to_base64(
    image: &RgbaImage,
    region: Option<Region>,
    png: bool,
) -> Result<CapturedImage, CaptureError> {
    let cropped = match region {
        Some(region) => crop(image, region)?,
        None => image.clone(),
    };
    encode(&downscale(cropped), png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn frame(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, 255]))
    }

    #[test]
    fn region_must_fit_including_edges() {
        assert!(Region::new(0, 0, 100, 100).fits_in(100, 100));
        assert!(Region::new(90, 90, 10, 10).fits_in(100, 100));
        assert!(!Region::new(95, 0, 10, 100).fits_in(100, 100));
        assert!(!Region::new(0, 0, 0, 10).fits_in(100, 100));
        assert!(!Region::new(0, 0, 10, 10).fits_in(5, 5));
    }

    #[test]
    fn overflowing_region_is_rejected_without_panic() {
        let image = frame(50, 50);
        let region = Region::new(u32::MAX - 2, 0, 10, 10);
        assert!(matches!(crop(&image, region), Err(CaptureError::Bounds)));
    }

    #[test]
    fn crop_returns_requested_size() {
        let image = frame(200, 100);
        let cropped = crop(&image, Region::new(10, 5, 40, 20)).unwrap();
        assert_eq!(cropped.width(), 40);
        assert_eq!(cropped.height(), 20);
    }

    #[test]
    fn small_images_are_not_resized() {
        let image = frame(800, 600);
        let scaled = downscale(image.clone());
        assert_eq!((scaled.width(), scaled.height()), (800, 600));
    }

    #[test]
    fn large_images_keep_aspect_ratio() {
        let scaled = downscale(frame(3840, 2160));
        assert_eq!(scaled.width(), MAX_EDGE);
        assert_eq!(scaled.height(), 1080);
    }

    #[test]
    fn encoding_produces_real_images() {
        let png = encode(&frame(4, 4), true).unwrap();
        assert_eq!(png.mime, "image/png");
        let bytes = STANDARD.decode(&png.base64).unwrap();
        assert_eq!(&bytes[1..4], b"PNG");

        let jpeg = encode(&frame(4, 4), false).unwrap();
        assert_eq!(jpeg.mime, "image/jpeg");
        let bytes = STANDARD.decode(&jpeg.base64).unwrap();
        assert_eq!(&bytes[0..2], &[0xff, 0xd8]);
    }

    #[test]
    fn full_pipeline_crops_then_scales() {
        let image = frame(4000, 3000);
        let captured =
            capture_to_base64(&image, Some(Region::new(0, 0, 2000, 1500)), true).unwrap();
        let bytes = STANDARD.decode(&captured.base64).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(decoded.width(), MAX_EDGE);
        assert_eq!(decoded.height(), 1440);
    }

    #[test]
    fn full_pipeline_rejects_bad_region() {
        let image = frame(100, 100);
        assert!(capture_to_base64(&image, Some(Region::new(0, 0, 200, 200)), true).is_err());
    }
}
