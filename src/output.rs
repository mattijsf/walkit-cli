// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes a preview and its thumbnails as PNG files.

use std::path::{Path, PathBuf};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::imageops::{self, FilterType as Resample};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

use crate::params::Thumbnail;
use crate::shot::Shot;

/// PNG bytes of an RGBA image, compressed as far as the encoder goes.
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, CompressionType::Best, FilterType::Adaptive)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// The preview resized for `thumb`: nearest pixels when it grows, so the pixels stay sharp,
/// and a Lanczos filter when it shrinks.
pub fn thumbnail(shot: &Shot, thumb: &Thumbnail) -> Result<(u32, u32, Vec<u8>), String> {
    let source = RgbaImage::from_raw(shot.width, shot.height, shot.rgba.clone())
        .ok_or("the preview's pixels do not fill its size")?;
    let width = thumb.width;
    let height = thumb.height.unwrap_or_else(|| {
        ((u64::from(shot.height) * u64::from(width) + u64::from(shot.width) / 2)
            / u64::from(shot.width))
        .max(1) as u32
    });
    let filter = if width >= shot.width && height >= shot.height {
        Resample::Nearest
    } else {
        Resample::Lanczos3
    };
    let resized = imageops::resize(&source, width, height, filter);
    Ok((width, height, resized.into_raw()))
}

/// Writes the preview to `path` and each thumbnail next to it, and gives the paths written.
pub fn write(shot: &Shot, path: &Path, thumbnails: &[Thumbnail]) -> Result<Vec<PathBuf>, String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut written = Vec::new();
    let mut save = |path: PathBuf, bytes: Vec<u8>| -> Result<(), String> {
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(path);
        Ok(())
    };
    save(
        path.to_path_buf(),
        png(shot.width, shot.height, &shot.rgba)?,
    )?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    for thumb in thumbnails {
        let (w, h, rgba) = thumbnail(shot, thumb)?;
        save(
            path.with_file_name(format!("{stem}{}.png", thumb.suffix)),
            png(w, h, &rgba)?,
        )?;
    }
    Ok(written)
}
