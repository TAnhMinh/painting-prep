//! Image files: decode to RGBA8 and encode the result. The only module
//! that uses the `image` crate.

use std::path::Path;

use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageReader, ImageResult};
use prep_core::BYTES_PER_PIXEL;

/// Decodes `path` to RGBA8, upright: EXIF orientation is applied, as the
/// browser does on the web path (spec F1).
///
/// No ICC colour management: a Display P3 photo is read as if it were
/// sRGB, so it comes out slightly duller than in the browser.
pub fn load(path: &Path) -> ImageResult<(Vec<u8>, u32, u32)> {
    let mut decoder = ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    Ok((rgba.into_raw(), width, height))
}

/// Encodes opaque RGBA8 to `path`, in the format given by its extension.
/// Saved as RGB, since alpha is always 255 (spec F1), so JPEG output works
/// too.
pub fn save(path: &Path, rgba: &[u8], width: u32, height: u32) -> ImageResult<()> {
    let (pixels, _) = rgba.as_chunks::<BYTES_PER_PIXEL>();
    let rgb: Vec<u8> = pixels.iter().flat_map(|&[r, g, b, _]| [r, g, b]).collect();
    image::save_buffer(path, &rgb, width, height, ExtendedColorType::Rgb8)
}
