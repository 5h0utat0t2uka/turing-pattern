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

pub fn encode_png(pixels: &[u8], size: usize) -> Result<Vec<u8>, String> {
    let size = u32::try_from(size).map_err(|_| "size is too large for PNG".to_string())?;
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgb);
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
    fn png_has_signature_and_dimensions() {
        let png = encode_png(&[0; 3 * 4], 2).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (2, 2));
    }
}
