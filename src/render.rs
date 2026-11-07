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

/// OKLab coordinates [L, a, b] of an sRGB color; distances in this space
/// track how different two colors look (Björn Ottosson's OKLab).
pub fn oklab(color: Color) -> [f64; 3] {
    let [r, g, b] = color.map(|c| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// Perceptual difference between two colors (Euclidean distance in OKLab).
pub fn color_difference(a: Color, b: Color) -> f64 {
    let (a, b) = (oklab(a), oklab(b));
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
}

/// Minimum OKLab difference between the two colors.
pub const MIN_COLOR_DIFFERENCE: f64 = 0.3;
/// Minimum OKLab lightness difference, so the pattern stays visible without
/// relying on hue alone (color vision differences, grayscale).
pub const MIN_LIGHTNESS_DIFFERENCE: f64 = 0.08;

/// A highly saturated, bright color of the given hue.
fn vivid(rng: &mut Rng, hue: f64) -> Color {
    let saturation = rng.uniform(0.8, 1.0);
    let value = rng.uniform(0.9, 1.0);
    hsv_to_rgb(hue, saturation, value)
}

/// Pick two vivid colors from the seed, with hues 120°–240° apart.
///
/// Both colors are highly saturated and bright, so they differ mainly in hue.
/// Pairs that look too similar are drawn again from the same random stream,
/// so the result is still determined by the seed.
pub fn random_palette(seed: u64) -> Palette {
    let mut rng = Rng::new(seed, Stream::Palette);
    for _ in 0..64 {
        let hue = rng.next_f64();
        let first = vivid(&mut rng, hue);
        let offset = rng.uniform(1.0 / 3.0, 2.0 / 3.0);
        let second = vivid(&mut rng, (hue + offset) % 1.0);
        if color_difference(first, second) >= MIN_COLOR_DIFFERENCE
            && (oklab(first)[0] - oklab(second)[0]).abs() >= MIN_LIGHTNESS_DIFFERENCE
        {
            return [first, second];
        }
    }
    // About 10% of draws are rejected, so 64 failures in a row never happen
    // in practice; the fallback keeps the function total.
    [[255, 0, 255], [0, 255, 255]]
}

/// sRGB color from OKLab coordinates, clamped to the displayable range.
pub fn from_oklab([l, a, b]: [f64; 3]) -> Color {
    let l_ = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m_ = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s_ = (l - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
    let linear = [
        4.076_741_662_1 * l_ - 3.307_711_591_3 * m_ + 0.230_969_929_2 * s_,
        -1.268_438_004_6 * l_ + 2.609_757_401_1 * m_ - 0.341_319_396_5 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_614_7 * m_ + 1.707_614_701_0 * s_,
    ];
    linear.map(|c| {
        let c = c.clamp(0.0, 1.0);
        let encoded = if c <= 0.003_130_8 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (255.0 * encoded).round() as u8
    })
}

/// Blend two colors in OKLCh (lightness, chroma, hue), turning the hue the
/// short way round. Unlike mixing RGB values, colors from opposite sides of
/// the hue circle stay vivid in between instead of passing through gray.
pub fn mix(palette: Palette, t: f64) -> Color {
    let [l0, a0, b0] = oklab(palette[0]);
    let [l1, a1, b1] = oklab(palette[1]);
    let (c0, c1) = (a0.hypot(b0), a1.hypot(b1));
    let (h0, h1) = (b0.atan2(a0), b1.atan2(a1));
    let turn =
        (h1 - h0 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
    let (l, c, h) = (l0 + (l1 - l0) * t, c0 + (c1 - c0) * t, h0 + turn * t);
    from_oklab([l, c * h.cos(), c * h.sin()])
}

/// Steps in the precomputed blend table.
const BLEND_STEPS: usize = 4096;

/// Blend amount between the two palette colors for a concentration `v`.
/// The scale is fixed, so frames of an animation never change contrast.
pub fn blend_amount(v: f32) -> f32 {
    (v / 0.5).clamp(0.0, 1.0)
}

/// Map concentration to colors; a fixed scale avoids per-frame contrast changes.
pub fn render(v: &[f32], palette: Palette) -> Vec<u8> {
    // The endpoints are the palette colors exactly; blends come from a table.
    let mut table: Vec<Color> = (0..BLEND_STEPS)
        .map(|i| mix(palette, i as f64 / (BLEND_STEPS - 1) as f64))
        .collect();
    table[0] = palette[0];
    table[BLEND_STEPS - 1] = palette[1];
    let mut pixels = Vec::with_capacity(v.len() * 3);
    for &value in v {
        let index = (blend_amount(value) * (BLEND_STEPS - 1) as f32).round() as usize;
        pixels.extend_from_slice(&table[index]);
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

/// Shrink a square image with `channels` values per pixel by averaging the
/// source area under each output pixel; returns sums divided by the area.
fn area_average(values: &[f32], channels: usize, src: usize, dst: usize) -> Vec<f32> {
    assert!(dst > 0 && dst <= src && values.len() == src * src * channels);
    let weights = area_weights(src, dst);
    // Rows first: src rows × dst columns.
    let mut rows = vec![0.0f32; src * dst * channels];
    for y in 0..src {
        for (x, columns) in weights.iter().enumerate() {
            for &(j, w) in columns {
                for k in 0..channels {
                    rows[(y * dst + x) * channels + k] += w * values[(y * src + j) * channels + k];
                }
            }
        }
    }
    let area = (src as f32 / dst as f32).powi(2);
    let mut out = vec![0.0f32; dst * dst * channels];
    for (y, lines) in weights.iter().enumerate() {
        for x in 0..dst {
            for k in 0..channels {
                let sum: f32 = lines
                    .iter()
                    .map(|&(j, w)| w * rows[(j * dst + x) * channels + k])
                    .sum();
                out[(y * dst + x) * channels + k] = sum / area;
            }
        }
    }
    out
}

/// Shrink a square RGB image by averaging the source area under each pixel.
pub fn downsample(pixels: &[u8], src: usize, dst: usize) -> Vec<u8> {
    if src == dst {
        return pixels.to_vec();
    }
    let values: Vec<f32> = pixels.iter().map(|&c| f32::from(c)).collect();
    area_average(&values, 3, src, dst)
        .into_iter()
        .map(|c| c.round().clamp(0.0, 255.0) as u8)
        .collect()
}

/// Shrink a square field of single values the same way as `downsample`.
pub fn downsample_values(values: &[f32], src: usize, dst: usize) -> Vec<f32> {
    if src == dst {
        return values.to_vec();
    }
    area_average(values, 1, src, dst)
}

/// Positions for `count` square tiles laid out left to right, top to bottom,
/// `columns` per row, `gap` pixels apart. Returns width, height, and the
/// top-left corner of each tile.
pub fn layout(
    count: usize,
    size: usize,
    columns: usize,
    gap: usize,
) -> (usize, usize, Vec<(usize, usize)>) {
    assert!(count > 0 && columns > 0);
    let columns = columns.min(count);
    let rows = count.div_ceil(columns);
    let width = columns * size + (columns - 1) * gap;
    let height = rows * size + (rows - 1) * gap;
    let corners = (0..count)
        .map(|i| ((i % columns) * (size + gap), (i / columns) * (size + gap)))
        .collect();
    (width, height, corners)
}

/// Lay square RGB tiles out with `layout`. Gaps and unused cells are
/// transparent, so the terminal background shows through. Returns RGBA
/// pixels, width, and height.
pub fn tile(tiles: &[Vec<u8>], size: usize, columns: usize, gap: usize) -> (Vec<u8>, usize, usize) {
    assert!(tiles.iter().all(|t| t.len() == size * size * 3));
    let (width, height, corners) = layout(tiles.len(), size, columns, gap);
    let mut out = vec![0u8; width * height * 4];
    for (tile, &(left, top)) in tiles.iter().zip(&corners) {
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

/// Encode RGB (or RGBA when `alpha`) pixels as PNG, with `(keyword, text)` chunks.
pub fn encode_png(
    pixels: &[u8],
    width: usize,
    height: usize,
    alpha: bool,
    text: &[(&str, String)],
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
    for (keyword, value) in text {
        encoder
            .add_text_chunk(keyword.to_string(), value.clone())
            .map_err(|e| e.to_string())?;
    }
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
        let palette = [[255, 0, 255], [0, 255, 255]];
        let pixels = render(&[0.0, 0.25, 0.5, 0.9, -0.1], palette);
        let at = |i: usize| [pixels[i * 3], pixels[i * 3 + 1], pixels[i * 3 + 2]];
        // Below 0 and above 0.5 clamp to the palette colors exactly.
        assert_eq!(
            [at(0), at(2), at(3), at(4)],
            [palette[0], palette[1], palette[1], palette[0]]
        );
        // 0.25 is the halfway blend.
        assert_eq!(at(1), mix(palette, 0.5));
    }

    #[test]
    fn blends_stay_vivid_between_opposite_hues() {
        let chroma = |c: Color| {
            let [_, a, b] = oklab(c);
            a.hypot(b)
        };
        let rgb_mix = |p: Palette, t: f64| -> Color {
            std::array::from_fn(|k| {
                let (a, b) = (f64::from(p[0][k]), f64::from(p[1][k]));
                (a + (b - a) * t).round() as u8
            })
        };
        let (mut ours, mut plain) = (0.0, 0.0);
        for seed in 0..500 {
            let palette = random_palette(seed);
            let low = chroma(palette[0]).min(chroma(palette[1]));
            for i in 1..10 {
                let t = f64::from(i) / 10.0;
                // Measured worst case 0.45 (cyan-ish hues have little room in
                // sRGB); a plain RGB mix drops to about 0.01, i.e. gray.
                let blend = mix(palette, t);
                assert!(
                    chroma(blend) > 0.4 * low,
                    "seed={seed} t={t} {blend:?} {palette:?}"
                );
            }
            ours += chroma(mix(palette, 0.5)) / low;
            plain += chroma(rgb_mix(palette, 0.5)) / low;
        }
        // Measured means at the midpoint: about 0.86 versus 0.38.
        assert!(ours > 2.0 * plain, "{ours} vs {plain}");
    }

    #[test]
    fn oklab_round_trips() {
        for color in [
            [255, 0, 255],
            [0, 255, 255],
            [12, 200, 77],
            [255, 255, 255],
            [0, 0, 0],
        ] {
            assert_eq!(from_oklab(oklab(color)), color);
        }
    }
    #[test]
    fn palette_is_reproducible_vivid_and_distinct() {
        assert_eq!(random_palette(42), random_palette(42));
        let palettes: std::collections::HashSet<_> = (0..2000).map(random_palette).collect();
        assert!(palettes.len() > 1990);
        for [first, second] in palettes {
            assert!(color_difference(first, second) >= MIN_COLOR_DIFFERENCE);
            assert!((oklab(first)[0] - oklab(second)[0]).abs() >= MIN_LIGHTNESS_DIFFERENCE);
            for color in [first, second] {
                // Vivid: bright (max channel ≥ 0.9) and saturated (min/max ≤ 0.2).
                let (max, min) = (*color.iter().max().unwrap(), *color.iter().min().unwrap());
                assert!(max >= 229, "{color:?}");
                assert!(f64::from(min) <= 0.2 * f64::from(max) + 0.5, "{color:?}");
            }
        }
    }

    #[test]
    fn palettes_cover_the_whole_hue_circle() {
        // Each of 12 hue sectors appears as a palette color.
        let mut seen = [false; 12];
        for seed in 0..500 {
            for color in random_palette(seed) {
                let [_, a, b] = oklab(color);
                let angle = b.atan2(a).rem_euclid(std::f64::consts::TAU);
                seen[(angle / std::f64::consts::TAU * 12.0) as usize % 12] = true;
            }
        }
        assert!(seen.iter().all(|&s| s), "{seen:?}");
    }

    #[test]
    fn oklab_matches_reference_values() {
        let close = |x: [f64; 3], y: [f64; 3]| (0..3).all(|k| (x[k] - y[k]).abs() < 1e-3);
        assert!(close(oklab([255, 255, 255]), [1.0, 0.0, 0.0]));
        assert!(close(oklab([0, 0, 0]), [0.0, 0.0, 0.0]));
        // Reference values from Björn Ottosson's OKLab post.
        assert!(close(oklab([255, 0, 0]), [0.628, 0.225, 0.126]));
        assert!(close(oklab([0, 0, 255]), [0.452, -0.032, -0.312]));
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
        let png = encode_png(
            &[0; 3 * 6],
            3,
            2,
            false,
            &[("Comment", "hello".to_string())],
        )
        .unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let info = reader.info();
        assert_eq!(
            (info.width, info.height, info.color_type),
            (3, 2, png::ColorType::Rgb)
        );
        assert_eq!(info.uncompressed_latin1_text[0].keyword, "Comment");
        assert_eq!(info.uncompressed_latin1_text[0].text, "hello");

        let png = encode_png(&[0; 4 * 4], 2, 2, true, &[]).unwrap();
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
