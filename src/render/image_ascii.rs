use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, DynamicImage};
use resvg::{self, tiny_skia, usvg};

use crate::model::{AsciiFrame, ImageAsset};

pub fn ensure_ascii_frames(
    asset: &mut ImageAsset,
    base_dir: &Path,
    width: u16,
    height: u16,
    fps: u16,
) {
    if width == 0 || height == 0 {
        asset.load_error = Some("image area is too small".to_string());
        return;
    }

    if asset.cached_for == Some((width, height))
        && (!asset.frames.is_empty() || asset.load_error.is_some())
    {
        return;
    }

    asset.frames.clear();
    asset.delays_ms.clear();
    asset.load_error = None;

    let full_path = resolve_image_path(base_dir, &asset.path);
    match load_ascii_frames(&full_path, width, height, fps) {
        Ok((frames, delays_ms)) => {
            asset.frames = frames;
            asset.delays_ms = delays_ms;
        }
        Err(err) => {
            asset.load_error = Some(err);
        }
    }

    asset.cached_for = Some((width, height));
}

pub fn current_frame(asset: &ImageAsset, elapsed: Duration) -> Option<&AsciiFrame> {
    if asset.frames.is_empty() {
        return None;
    }
    if asset.frames.len() == 1 {
        return asset.frames.first();
    }

    let mut total_ms: u128 = 0;
    for delay in &asset.delays_ms {
        total_ms += u128::from((*delay).max(1));
    }
    if total_ms == 0 {
        return asset.frames.first();
    }

    let mut cursor = elapsed.as_millis() % total_ms;
    for (idx, delay) in asset.delays_ms.iter().enumerate() {
        let delay = u128::from((*delay).max(1));
        if cursor < delay {
            return asset.frames.get(idx);
        }
        cursor = cursor.saturating_sub(delay);
    }

    asset.frames.first()
}

fn resolve_image_path(base_dir: &Path, image_path: &Path) -> PathBuf {
    if image_path.is_absolute() {
        image_path.to_path_buf()
    } else {
        base_dir.join(image_path)
    }
}

fn load_ascii_frames(
    path: &Path,
    width: u16,
    height: u16,
    fps: u16,
) -> Result<(Vec<AsciiFrame>, Vec<u16>), String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if extension == "gif" {
        load_gif_ascii_frames(path, width, height, fps)
    } else if extension == "svg" {
        load_svg_ascii_frame(path, width, height, fps)
    } else if matches!(
        extension.as_str(),
        "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi"
    ) {
        Err(format!(
            "video files are not supported ({}).",
            path.display()
        ))
    } else {
        let image = image::open(path)
            .map_err(|err| format!("image decode failed ({}): {err}", path.display()))?;
        let frame = dynamic_image_to_ascii_frame(&image, width, height);
        let delay = frame_interval_ms(fps);
        Ok((vec![frame], vec![delay]))
    }
}

fn load_svg_ascii_frame(
    path: &Path,
    width: u16,
    height: u16,
    fps: u16,
) -> Result<(Vec<AsciiFrame>, Vec<u16>), String> {
    let data = std::fs::read(path)
        .map_err(|err| format!("failed to read svg ({}): {err}", path.display()))?;
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(&data, &options)
        .map_err(|err| format!("failed to parse svg ({}): {err}", path.display()))?;

    let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).ok_or_else(|| {
        format!(
            "failed to create svg target canvas ({}): invalid size",
            path.display()
        )
    })?;
    let svg_size = tree.size();
    let sx = width as f32 / svg_size.width();
    let sy = height as f32 / svg_size.height();
    let scale = sx.min(sy);
    let draw_w = svg_size.width() * scale;
    let draw_h = svg_size.height() * scale;
    let tx = (width as f32 - draw_w) * 0.5;
    let ty = (height as f32 - draw_h) * 0.5;
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let rgba = image::RgbaImage::from_raw(width as u32, height as u32, pixmap.data().to_vec())
        .ok_or_else(|| format!("failed to materialize svg pixels ({})", path.display()))?;
    let dyn_image = DynamicImage::ImageRgba8(rgba);
    let frame = dynamic_image_to_ascii_frame(&dyn_image, width, height);
    let delay = frame_interval_ms(fps);
    Ok((vec![frame], vec![delay]))
}

fn load_gif_ascii_frames(
    path: &Path,
    width: u16,
    height: u16,
    fps: u16,
) -> Result<(Vec<AsciiFrame>, Vec<u16>), String> {
    let file = File::open(path)
        .map_err(|err| format!("failed to open gif ({}): {err}", path.display()))?;
    let reader = BufReader::new(file);
    let decoder = GifDecoder::new(reader)
        .map_err(|err| format!("failed to create gif decoder ({}): {err}", path.display()))?;

    let frames = decoder
        .into_frames()
        .collect_frames()
        .map_err(|err| format!("failed to decode gif frames ({}): {err}", path.display()))?;

    if frames.is_empty() {
        return Err(format!("gif has no frames ({})", path.display()));
    }

    let mut ascii_frames = Vec::with_capacity(frames.len());
    let mut delays = Vec::with_capacity(frames.len());
    let min_interval = u32::from(frame_interval_ms(fps));

    for frame in frames {
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let raw_ms = if denominator == 0 {
            numerator
        } else {
            numerator / denominator
        };
        let clamped = raw_ms.clamp(min_interval, 1_000);

        let buffer = frame.into_buffer();
        let dyn_image = DynamicImage::ImageRgba8(buffer);
        ascii_frames.push(dynamic_image_to_ascii_frame(&dyn_image, width, height));
        delays.push(clamped as u16);
    }

    Ok((ascii_frames, delays))
}

fn dynamic_image_to_ascii_frame(image: &DynamicImage, width: u16, height: u16) -> AsciiFrame {
    let resized = image.resize_exact(width as u32, height as u32, FilterType::Triangle);
    let luma = resized.to_luma8();

    let mut lines = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut line = String::with_capacity(width as usize);
        for x in 0..width {
            let value = luma.get_pixel(x as u32, y as u32).0[0];
            line.push(luma_to_ascii(value));
        }
        lines.push(line);
    }

    AsciiFrame {
        lines,
        width,
        height,
    }
}

fn luma_to_ascii(value: u8) -> char {
    match value {
        0..=63 => '█',
        64..=127 => '▓',
        128..=191 => '▒',
        _ => '░',
    }
}

fn frame_interval_ms(fps: u16) -> u16 {
    let fps = fps.max(1);
    let ms = 1_000u16 / fps;
    ms.max(16)
}

