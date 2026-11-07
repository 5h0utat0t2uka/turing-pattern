//! Colors, image rendering, and PNG encoding.

use crate::rng::{Rng, Stream};

pub type Color = [u8; 3];

/// (background, pattern)
pub type Palette = [Color; 2];

pub fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Color {
    let sector = (h * 6.0).floor();
    let f = h * 6.0 - sector;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match sector as i64 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r, g, b].map(|c| (255.0 * c).round() as u8)
}

/// Pick a contrasting color pair from the seed: one dark, one light, in random order.
pub fn random_palette(seed: u64) -> Palette {
    let mut rng = Rng::new(seed, Stream::Palette);
    let hue = rng.next_f64();
    let dark = hsv_to_rgb(hue, rng.uniform(0.4, 0.9), rng.uniform(0.05, 0.25));
    let light = hsv_to_rgb(
        (hue + rng.uniform(0.25, 0.75)) % 1.0,
        rng.uniform(0.5, 1.0),
        rng.uniform(0.85, 1.0),
    );
    if rng.next_f64() < 0.5 {
        [dark, light]
    } else {
        [light, dark]
    }
}

/// Map concentration to colors; a fixed scale avoids per-frame contrast changes.
pub fn render(v: &[f32], palette: Palette) -> Vec<u8> {
    let [background, pattern] = palette.map(|c| c.map(f32::from));
    let mut pixels = Vec::with_capacity(v.len() * 3);
    for &value in v {
        let t = (value / 0.5).clamp(0.0, 1.0);
        for k in 0..3 {
            pixels.push((background[k] + (pattern[k] - background[k]) * t).round() as u8);
        }
    }
    pixels
}

/// Area-weighted column ranges: for each output index, the source indices
/// it covers and how much of each (weights sum to `src / dst`).
fn area_weights(src: usize, dst: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = src as f64 / dst as f64;
    (0..dst)
        .map(|i| {
            let (start, end) = (i as f64 * scale, (i + 1) as f64 * scale);
            (start.floor() as usize..(end.ceil() as usize).min(src))
                .map(|j| (j, (end.min(j as f64 + 1.0) - start.max(j as f64)) as f32))
                .filter(|&(_, w)| w > 0.0)
                .collect()
        })
        .collect()
}

/// Shrink a square RGB image by averaging the source area under each pixel.
pub fn downsample(pixels: &[u8], src: usize, dst: usize) -> Vec<u8> {
    assert!(dst > 0 && dst <= src && pixels.len() == src * src * 3);
    if src == dst {
        return pixels.to_vec();
    }
    let weights = area_weights(src, dst);
    // Rows first: src rows × dst columns, kept in floating point.
    let mut rows = vec![0.0f32; src * dst * 3];
    for y in 0..src {
        for (x, columns) in weights.iter().enumerate() {
            for &(j, w) in columns {
                for k in 0..3 {
                    rows[(y * dst + x) * 3 + k] += w * f32::from(pixels[(y * src + j) * 3 + k]);
                }
            }
        }
    }
    let area = (src as f32 / dst as f32).powi(2);
    let mut out = vec![0u8; dst * dst * 3];
    for (y, lines) in weights.iter().enumerate() {
        for x in 0..dst {
            for k in 0..3 {
                let sum: f32 = lines
                    .iter()
                    .map(|&(j, w)| w * rows[(j * dst + x) * 3 + k])
                    .sum();
                out[(y * dst + x) * 3 + k] = (sum / area).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Lay square RGB tiles out left to right, top to bottom, `columns` per row,
/// separated by `gap` pixels. Gaps and unused cells are transparent, so the
/// terminal background shows through. Returns RGBA pixels, width, and height.
pub fn tile(tiles: &[Vec<u8>], size: usize, columns: usize, gap: usize) -> (Vec<u8>, usize, usize) {
    assert!(!tiles.is_empty() && columns > 0);
    assert!(tiles.iter().all(|t| t.len() == size * size * 3));
    let columns = columns.min(tiles.len());
    let rows = tiles.len().div_ceil(columns);
    let width = columns * size + (columns - 1) * gap;
    let height = rows * size + (rows - 1) * gap;
    let mut out = vec![0u8; width * height * 4];
    for (index, tile) in tiles.iter().enumerate() {
        let (left, top) = (
            (index % columns) * (size + gap),
            (index / columns) * (size + gap),
        );
        for y in 0..size {
            for x in 0..size {
                let src = &tile[(y * size + x) * 3..][..3];
                let dst = ((top + y) * width + left + x) * 4;
                out[dst..dst + 3].copy_from_slice(src);
                out[dst + 3] = 255;
            }
        }
    }
    (out, width, height)
}

/// Encode RGB (or RGBA when `alpha`) pixels as PNG.
pub fn encode_png(
    pixels: &[u8],
    width: usize,
    height: usize,
    alpha: bool,
) -> Result<Vec<u8>, String> {
    let too_large = |_| "image is too large for PNG".to_string();
    let (width, height) = (
        u32::try_from(width).map_err(too_large)?,
        u32::try_from(height).map_err(too_large)?,
    );
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(if alpha {
        png::ColorType::Rgba
    } else {
        png::ColorType::Rgb
    });
    encoder.set_depth(png::BitDepth::Eight);
    // chafa decodes it immediately; speed matters more than file size.
    encoder.set_compression(png::Compression::Fastest);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(pixels).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_uses_a_fixed_scale() {
        let palette = [[0, 0, 0], [200, 100, 50]];
        let pixels = render(&[0.0, 0.25, 0.5, 0.9, -0.1], palette);
        assert_eq!(
            pixels,
            [0, 0, 0, 100, 50, 25, 200, 100, 50, 200, 100, 50, 0, 0, 0]
        );
    }

    #[test]
    fn palette_is_reproducible_contrasting_and_varies_by_seed() {
        assert_eq!(random_palette(42), random_palette(42));
        let palettes: std::collections::HashSet<_> = (0..50).map(random_palette).collect();
        assert!(palettes.len() > 40);
        for [background, pattern] in palettes {
            let brightness = |c: Color| i32::from(*c.iter().max().unwrap());
            assert!((brightness(background) - brightness(pattern)).abs() > 100);
        }
    }

    #[test]
    fn hsv_matches_known_colors() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv_to_rgb(1.0 / 3.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv_to_rgb(2.0 / 3.0, 1.0, 1.0), [0, 0, 255]);
        assert_eq!(hsv_to_rgb(0.5, 0.0, 0.5), [128, 128, 128]);
    }

    #[test]
    fn downsample_averages_covered_area() {
        let image = |values: &[u8]| values.iter().flat_map(|&v| [v, v, v]).collect::<Vec<u8>>();
        // Same size is unchanged.
        let src = image(&[1, 2, 3, 4]);
        assert_eq!(downsample(&src, 2, 2), src);
        // 2×2 → 1×1 is the mean.
        assert_eq!(downsample(&image(&[0, 100, 200, 100]), 2, 1), image(&[100]));
        // 3×3 → 2×2 splits the middle row and column in half.
        let src = image(&[0, 0, 0, 0, 0, 0, 0, 0, 180]);
        // Bottom-right output covers 1.5×1.5 source pixels: 0.25 of 0 … 1.0 of 180.
        assert_eq!(downsample(&src, 3, 2), image(&[0, 0, 0, 80]));
        // A flat image stays flat for any ratio.
        assert!(
            downsample(&image(&[77; 64 * 64]), 64, 17)
                .iter()
                .all(|&v| v == 77)
        );
    }

    #[test]
    fn png_has_signature_and_dimensions() {
        let png = encode_png(&[0; 3 * 6], 3, 2, false).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let info = reader.info();
        assert_eq!(
            (info.width, info.height, info.color_type),
            (3, 2, png::ColorType::Rgb)
        );

        let png = encode_png(&[0; 4 * 4], 2, 2, true).unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    }

    #[test]
    fn tiles_are_laid_out_with_transparent_gaps() {
        // Three 2×2 tiles of values 1, 2, 3; two per row; gap of 1.
        let tiles: Vec<Vec<u8>> = (1..=3).map(|v| vec![v; 2 * 2 * 3]).collect();
        let (pixels, width, height) = tile(&tiles, 2, 2, 1);
        assert_eq!((width, height), (5, 5));
        let at = |x: usize, y: usize| &pixels[(y * width + x) * 4..][..4];
        assert_eq!(at(0, 0), [1, 1, 1, 255]);
        assert_eq!(at(1, 1), [1, 1, 1, 255]);
        assert_eq!(at(2, 0), [0, 0, 0, 0]); // gap column
        assert_eq!(at(3, 1), [2, 2, 2, 255]);
        assert_eq!(at(0, 2), [0, 0, 0, 0]); // gap row
        assert_eq!(at(1, 4), [3, 3, 3, 255]);
        assert_eq!(at(4, 4), [0, 0, 0, 0]); // unused cell
        // Fewer tiles than columns: a single row, no trailing gap.
        assert_eq!(tile(&tiles[..1], 2, 4, 1).1, 2);
    }
}
