use gpui::RenderImage;
use std::{io::Cursor, sync::Arc};

pub(crate) fn decode_album_art(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    match image::load_from_memory(bytes) {
        // Covers are prepared at 320 px, so most need no resizing.
        Ok(image) if image.width() <= 320 && image.height() <= 320 => Some(render_image(image)),
        Ok(image) => Some(render_image(image.thumbnail(320, 320))),
        Err(error) => {
            eprintln!("Could not decode album artwork: {error}");
            None
        }
    }
}

/// Convert a decoded image into the BGRA frame GPUI draws.
///
/// # Arguments
/// * `image` - Decoded image in any color format.
///
/// # Returns
/// An image ready to draw with `gpui::img`.
pub(crate) fn render_image(image: image::DynamicImage) -> Arc<RenderImage> {
    let mut image = image.into_rgba8();
    for pixel in image.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new([image::Frame::new(image)]))
}

pub(crate) fn prepare_cover(bytes: &[u8]) -> Option<Vec<u8>> {
    const MAX_COVER_EDGE: u32 = 320;
    const MAX_SOURCE_EDGE: u32 = 4096;
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_EDGE);
    limits.max_image_height = Some(MAX_SOURCE_EDGE);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let thumbnail = decoded.thumbnail(MAX_COVER_EDGE, MAX_COVER_EDGE);
    let mut output = Cursor::new(Vec::new());
    if decoded.color().has_alpha() {
        let rgba = thumbnail.to_rgba8();
        if rgba.pixels().any(|pixel| pixel[3] != u8::MAX) {
            image::DynamicImage::ImageRgba8(rgba)
                .write_to(&mut output, image::ImageFormat::Png)
                .ok()?;
        } else {
            let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 88)
                .encode_image(&image::DynamicImage::ImageRgb8(rgb))
                .ok()?;
        }
    } else {
        let rgb = image::DynamicImage::ImageRgb8(thumbnail.to_rgb8());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 88)
            .encode_image(&rgb)
            .ok()?;
    }
    Some(output.into_inner())
}
