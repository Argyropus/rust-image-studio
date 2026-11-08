//! Input images for image-to-image modes: loading from files, the clipboard, or
//! the gallery, off the UI thread.

use egui::{Context, TextureHandle, TextureOptions};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;

pub const MAX_INPUTS: usize = 3;

pub struct InputImage {
    pub name: String,
    pub rgba: Arc<image::RgbaImage>,
    pub tex: TextureHandle,
    /// Prompt recorded in the file, when this app made it (None for photos/clipboard).
    pub record: Option<String>,
}

impl InputImage {
    pub fn size(&self) -> (u32, u32) {
        (self.rgba.width(), self.rgba.height())
    }
}

/// Decoded image waiting to be turned into an `InputImage` on the UI thread, with the
/// prompt recorded in the file if this app made it.
pub type Loaded = Result<(String, image::RgbaImage, Option<String>), String>;

pub fn is_image_path(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp"))
}

/// Writes `src` (a generated PNG) to `dest` in the format its extension names:
/// PNG is copied as-is (keeps the embedded generation metadata), WebP is lossless,
/// JPEG uses quality 92. Returns the written file size.
pub fn export_image(src: &std::path::Path, dest: &std::path::Path) -> anyhow::Result<u64> {
    let ext = dest.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    match ext.as_str() {
        "webp" => {
            image::open(src)?.save_with_format(dest, image::ImageFormat::WebP)?;
        }
        "jpg" | "jpeg" => {
            let rgb = image::open(src)?.to_rgb8();
            let mut out = std::io::BufWriter::new(std::fs::File::create(dest)?);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92).encode_image(&rgb)?;
        }
        _ => {
            std::fs::copy(src, dest)?;
        }
    }
    Ok(std::fs::metadata(dest)?.len())
}

pub fn load_file(path: PathBuf, tx: Sender<Loaded>, ctx: Context) {
    std::thread::spawn(move || {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        // One read: decode and look for the recorded prompt from the same bytes.
        let res = std::fs::read(&path).map_err(|e| format!("{name}: {e}")).and_then(|bytes| {
            let prompt = crate::metadata::prompt_from_bytes(&bytes);
            image::load_from_memory(&bytes).map(|img| (name.clone(), img.to_rgba8(), prompt)).map_err(|e| format!("{name}: {e}"))
        });
        let _ = tx.send(res);
        ctx.request_repaint();
    });
}

pub fn from_clipboard() -> Loaded {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let img = cb.get_image().map_err(|_| t!("클립보드에 이미지가 없습니다", "No image on the clipboard").to_owned())?;
    let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
        .ok_or_else(|| t!("클립보드 이미지 형식을 읽을 수 없습니다", "Can't read the clipboard image format").to_owned())?;
    Ok((t!("클립보드", "Clipboard").to_owned(), rgba, None))
}

pub fn make_input(ctx: &Context, name: String, rgba: image::RgbaImage) -> InputImage {
    // Display texture capped at 1280 px; the full-resolution pixels go to the engine.
    // Resizes from a borrow (no full-size copy); small images are used as they are.
    let (w, h) = rgba.dimensions();
    let s = (1280.0 / w.max(h) as f32).min(1.0);
    let color = if s < 1.0 {
        let (tw, th) = (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1));
        crate::engine::rgba_to_color(&image::imageops::thumbnail(&rgba, tw, th))
    } else {
        crate::engine::rgba_to_color(&rgba)
    };
    let tex = ctx.load_texture(format!("input-{name}"), color, TextureOptions::LINEAR);
    InputImage { name, rgba: Arc::new(rgba), tex, record: None }
}

#[cfg(test)]
mod tests {
    #[test]
    fn webp_round_trip_and_export() {
        let dir = std::env::temp_dir().join("qis_webp_test");
        std::fs::create_dir_all(&dir).unwrap();
        let img = image::RgbaImage::from_fn(64, 48, |x, y| image::Rgba([x as u8 * 4, y as u8 * 5, 128, 255]));
        let png = dir.join("src.png");
        img.save(&png).unwrap();

        // Input side: a .webp file must decode (this is what failed before).
        let webp = dir.join("in.webp");
        img.save_with_format(&webp, image::ImageFormat::WebP).unwrap();
        assert!(super::is_image_path(&webp));
        let back = image::open(&webp).unwrap().to_rgba8();
        assert_eq!(back.dimensions(), (64, 48));
        assert_eq!(back, img, "lossless WebP must round-trip exactly");

        // Export side: every Save As format writes a decodable file.
        for name in ["out.png", "out.webp", "out.jpg"] {
            let dest = dir.join(name);
            assert!(super::export_image(&png, &dest).unwrap() > 0);
            assert_eq!(image::open(&dest).unwrap().width(), 64, "{name}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod lossy_sample {
    /// Decodes a real-world (lossy) WebP when QIS_WEBP_SAMPLE points at one.
    #[test]
    fn decode_sample_webp() {
        let Some(p) = std::env::var_os("QIS_WEBP_SAMPLE") else { return };
        let img = image::open(&p).expect("decode sample webp");
        println!("sample webp decoded: {}x{}", img.width(), img.height());
    }
}
