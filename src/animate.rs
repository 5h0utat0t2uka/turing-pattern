//! Animated GIFs of a pattern forming.
//!
//! Every pixel is a blend of the two palette colors, so the color table is
//! just steps along that blend (no color quantization or dithering).
//! Frames are encoded as the changed area only, with unchanged pixels
//! transparent, and identical frames are merged.

use rayon::prelude::*;

use crate::MIN_SIMULATION_SIZE;
use crate::render::{Palette, blend_amount, downsample_values, layout, mix};
use crate::sim::{Grid, Pattern};

pub const DEFAULT_FRAMES: u32 = 120;
pub const DEFAULT_FPS: u32 = 24;
/// GIF delays are in hundredths of a second; most viewers slow down delays
/// below 2, so 50 fps is the fastest rate that plays as asked.
pub const MAX_FPS: u32 = 50;
/// The finished pattern stays on screen this long (1/100 s) before looping.
pub const HOLD_LAST: u16 = 150;

/// Blend steps in the color table, plus one transparent entry: 64 in all,
/// so LZW codes start at 6 bits. Over 2000 palettes the largest step between
/// neighbors is at most 0.020 in OKLab (about a just-noticeable difference);
/// 31 steps reached 0.040, enough for banding in smooth areas. On 256×256,
/// 120 frames, 255 steps gave 3.3–4.6 MB and 63 steps about half of that.
const BLENDS: usize = 63;
const TRANSPARENT: u8 = BLENDS as u8;

/// Simulation step shown by each frame: step 0 (the initial seeds) first,
/// `steps` last, spaced quadratically so the fast early formation gets more
/// frames than the slow settling at the end. Steps strictly increase.
pub fn frame_steps(steps: u32, frames: u32) -> Result<Vec<u32>, String> {
    if frames < 2 {
        return Err("an animation needs at least 2 frames".to_string());
    }
    if u64::from(frames) > u64::from(steps) + 1 {
        return Err(format!(
            "--frames {frames} needs at least {} --steps",
            frames - 1
        ));
    }
    let last = f64::from(frames - 1);
    let mut out: Vec<u32> = Vec::with_capacity(frames as usize);
    for k in 0..frames {
        let ideal = (f64::from(steps) * (f64::from(k) / last).powi(2)).round() as u32;
        // Later frames are pushed forward so no two show the same step; since
        // frames ≤ steps + 1 this never passes `steps`.
        let min = out.last().map_or(0, |&s| s + 1);
        let remaining = frames - 1 - k;
        out.push(ideal.max(min).min(steps - remaining));
    }
    Ok(out)
}

pub struct Animation<'a> {
    pub size: usize,
    pub seed: u64,
    pub steps: u32,
    /// One pattern, or several laid out side by side.
    pub patterns: &'a [Pattern],
    pub columns: usize,
    pub gap: usize,
    pub palette: Palette,
    pub frames: u32,
    pub fps: u32,
    /// Stored in the GIF as a comment.
    pub comment: &'a str,
}

/// The GIF color table: `BLENDS` steps from the first to the second palette
/// color (endpoints exact), then one transparent entry.
fn color_table(palette: Palette) -> Vec<u8> {
    let mut table = Vec::with_capacity(256 * 3);
    for i in 0..BLENDS {
        let color = match i {
            0 => palette[0],
            _ if i == BLENDS - 1 => palette[1],
            _ => mix(palette, i as f64 / (BLENDS - 1) as f64),
        };
        table.extend_from_slice(&color);
    }
    table.extend_from_slice(&[0, 0, 0]);
    table
}

fn blend_index(t: f32) -> u8 {
    (t * (BLENDS - 1) as f32).round() as u8
}

impl Animation<'_> {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.fps == 0 || self.fps > MAX_FPS {
            return Err(format!("--fps must be from 1 to {MAX_FPS}"));
        }
        let targets = frame_steps(self.steps, self.frames)?;
        let (width, height, corners) =
            layout(self.patterns.len(), self.size, self.columns, self.gap);
        let too_large = |_| "animation is too large for GIF (65535 pixels per side)".to_string();
        let (w16, h16) = (
            u16::try_from(width).map_err(too_large)?,
            u16::try_from(height).map_err(too_large)?,
        );

        let simulated = self.size.max(MIN_SIMULATION_SIZE);
        let mut grids: Vec<(Grid, Pattern)> = self
            .patterns
            .iter()
            .map(|&p| Grid::new(simulated, self.seed).map(|g| (g, p)))
            .collect::<Result<_, _>>()?;

        let mut out = Vec::new();
        let mut encoder = gif::Encoder::new(&mut out, w16, h16, &color_table(self.palette))
            .map_err(|e| e.to_string())?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;
        encoder
            .write_raw_extension(gif::AnyExtension(0xFE), &[self.comment.as_bytes()])
            .map_err(|e| e.to_string())?;

        let delay = ((100.0 / f64::from(self.fps)).round() as u16).max(2);
        let mut previous: Option<Vec<u8>> = None;
        // A frame is written once the next one is known, so identical frames
        // can be merged into a longer delay.
        let mut pending: Option<gif::Frame<'static>> = None;
        let mut done = 0;
        for &target in &targets {
            let advance = target - done;
            grids
                .par_iter_mut()
                .try_for_each(|(grid, pattern)| grid.advance(advance, *pattern))?;
            done = target;

            let mut canvas = vec![TRANSPARENT; width * height];
            for ((grid, _), &(left, top)) in grids.iter().zip(&corners) {
                let blends: Vec<f32> = grid.v.iter().map(|&v| blend_amount(v)).collect();
                let blends = downsample_values(&blends, simulated, self.size);
                for y in 0..self.size {
                    let row = &mut canvas[(top + y) * width + left..][..self.size];
                    for (dst, &t) in row.iter_mut().zip(&blends[y * self.size..][..self.size]) {
                        *dst = blend_index(t);
                    }
                }
            }

            let frame = match &previous {
                None => Some(full_frame(&canvas, w16, h16)),
                Some(before) => changed_area(before, &canvas, width, height),
            };
            match frame {
                Some(mut frame) => {
                    frame.delay = delay;
                    if let Some(ready) = pending.replace(frame) {
                        encoder.write_frame(&ready).map_err(|e| e.to_string())?;
                    }
                }
                None => {
                    if let Some(p) = pending.as_mut() {
                        p.delay = p.delay.saturating_add(delay);
                    }
                }
            }
            previous = Some(canvas);
        }
        if let Some(mut last) = pending {
            last.delay = last.delay.max(HOLD_LAST);
            encoder.write_frame(&last).map_err(|e| e.to_string())?;
        }
        encoder.into_inner().map_err(|e| e.to_string())?;
        Ok(out)
    }
}

fn full_frame(canvas: &[u8], width: u16, height: u16) -> gif::Frame<'static> {
    gif::Frame {
        width,
        height,
        transparent: Some(TRANSPARENT),
        dispose: gif::DisposalMethod::Keep,
        buffer: canvas.to_vec().into(),
        ..gif::Frame::default()
    }
}

/// The bounding box of pixels that differ from `before`, with unchanged
/// pixels inside it transparent (the previous frame shows through).
/// `None` when nothing changed.
fn changed_area(
    before: &[u8],
    after: &[u8],
    width: usize,
    height: usize,
) -> Option<gif::Frame<'static>> {
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if before[y * width + x] != after[y * width + x] {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    if x0 > x1 {
        return None;
    }
    let mut buffer = Vec::with_capacity((x1 - x0 + 1) * (y1 - y0 + 1));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (b, a) = (before[y * width + x], after[y * width + x]);
            buffer.push(if a == b { TRANSPARENT } else { a });
        }
    }
    Some(gif::Frame {
        left: x0 as u16,
        top: y0 as u16,
        width: (x1 - x0 + 1) as u16,
        height: (y1 - y0 + 1) as u16,
        transparent: Some(TRANSPARENT),
        dispose: gif::DisposalMethod::Keep,
        buffer: buffer.into(),
        ..gif::Frame::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;
    use crate::render::{Color, color_difference, random_palette};

    /// Decoded GIF: canvas size, color table, loop setting, and every frame
    /// composited onto the canvas (as the viewer shows it), with its delay.
    struct Decoded {
        width: usize,
        height: usize,
        table: Vec<u8>,
        infinite: bool,
        frames: Vec<(u16, Vec<u8>)>,
    }

    fn decode(data: &[u8]) -> Decoded {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::Indexed);
        let mut decoder = options.read_info(data).unwrap();
        let (width, height) = (decoder.width() as usize, decoder.height() as usize);
        let table = decoder.global_palette().unwrap().to_vec();
        let mut canvas = vec![TRANSPARENT; width * height];
        let mut frames = Vec::new();
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            assert_eq!(frame.dispose, gif::DisposalMethod::Keep);
            let (left, top, w) = (
                frame.left as usize,
                frame.top as usize,
                frame.width as usize,
            );
            for (i, &index) in frame.buffer.iter().enumerate() {
                if Some(index) != frame.transparent {
                    canvas[(top + i / w) * width + left + i % w] = index;
                }
            }
            frames.push((frame.delay, canvas.clone()));
        }
        let infinite = decoder.repeat() == gif::Repeat::Infinite;
        Decoded {
            width,
            height,
            table,
            infinite,
            frames,
        }
    }

    /// Indexes the encoder should show for one pattern after `steps`.
    fn expected(size: usize, seed: u64, steps: u32, pattern: Pattern) -> Vec<u8> {
        let simulated = size.max(MIN_SIMULATION_SIZE);
        let mut grid = Grid::new(simulated, seed).unwrap();
        grid.advance(steps, pattern).unwrap();
        let blends: Vec<f32> = grid.v.iter().map(|&v| blend_amount(v)).collect();
        downsample_values(&blends, simulated, size)
            .into_iter()
            .map(blend_index)
            .collect()
    }

    fn animation<'a>(size: usize, patterns: &'a [Pattern], frames: u32) -> Animation<'a> {
        Animation {
            size,
            seed: 9,
            steps: 300,
            patterns,
            columns: 4,
            gap: 2,
            palette: random_palette(9),
            frames,
            fps: 20,
            comment: "test comment",
        }
    }

    #[test]
    fn frame_steps_start_at_zero_end_at_steps_and_favor_the_start() {
        let steps = frame_steps(4000, 120).unwrap();
        assert_eq!(steps.len(), 120);
        assert_eq!((steps[0], steps[119]), (0, 4000));
        assert!(steps.windows(2).all(|w| w[0] < w[1]));
        // Quadratic spacing: about 71% of frames show the first half of the steps.
        assert!(steps.iter().filter(|&&s| s < 2000).count() > 80);
        // As many frames as steps + 1: every step once.
        assert_eq!(frame_steps(10, 11).unwrap(), (0..=10).collect::<Vec<_>>());
        assert!(frame_steps(10, 1).is_err());
        assert!(frame_steps(10, 12).is_err());
    }

    #[test]
    fn color_table_has_exact_endpoints_and_a_transparent_entry() {
        let palette = random_palette(3);
        let table = color_table(palette);
        assert_eq!(table.len(), 64 * 3);
        assert_eq!(table[..3], palette[0]);
        assert_eq!(table[(BLENDS - 1) * 3..][..3], palette[1]);
        assert_eq!(table[BLENDS * 3..][..3], mix(palette, 0.5).map(|_| 0));
    }

    #[test]
    fn changed_area_covers_only_what_changed() {
        let before = vec![1u8; 5 * 4];
        assert!(changed_area(&before, &before, 5, 4).is_none());
        let mut after = before.clone();
        after[5 + 1] = 7; // (1, 1)
        after[2 * 5 + 3] = 8; // (3, 2)
        let frame = changed_area(&before, &after, 5, 4).unwrap();
        assert_eq!(
            (frame.left, frame.top, frame.width, frame.height),
            (1, 1, 3, 2)
        );
        let t = TRANSPARENT;
        assert_eq!(&*frame.buffer, [7, t, t, t, t, 8]);
    }

    #[test]
    fn decoded_frames_match_the_simulation_exactly() {
        for size in [24, 70] {
            let patterns = [Pattern::Spots];
            let gif = animation(size, &patterns, 12).encode().unwrap();
            let decoded = decode(&gif);
            assert_eq!((decoded.width, decoded.height), (size, size));
            assert!(decoded.infinite);
            assert_eq!(decoded.table, color_table(random_palette(9)));
            // First frame: the initial seeds; last frame: the finished pattern.
            assert_eq!(decoded.frames[0].1, expected(size, 9, 0, Pattern::Spots));
            let (last_delay, last) = decoded.frames.last().unwrap();
            assert_eq!(*last, expected(size, 9, 300, Pattern::Spots));
            assert!(*last_delay >= HOLD_LAST);
            // 20 fps is 5/100 s per frame; merged frames add up.
            let total: u32 = decoded.frames.iter().map(|(d, _)| u32::from(*d)).sum();
            assert_eq!(total, 11 * 5 + u32::from(HOLD_LAST));
        }
    }

    #[test]
    fn last_frame_colors_match_the_still_image() {
        // 64 and above: no shrinking, so only the number of blend steps differs.
        // (Below 64 the still image averages colors and the GIF averages blend
        // amounts, which differ on soft edges.)
        for seed in [9, 513, 599] {
            let patterns = [Pattern::Holes];
            let mut settings = animation(70, &patterns, 6);
            settings.seed = seed;
            settings.palette = random_palette(seed);
            let decoded = decode(&settings.encode().unwrap());
            let still = generate(70, seed, 300, Pattern::Holes, random_palette(seed)).unwrap();
            let last = &decoded.frames.last().unwrap().1;
            let worst = last
                .iter()
                .zip(still.chunks(3))
                .map(|(&i, s)| {
                    let gif: Color = decoded.table[usize::from(i) * 3..][..3].try_into().unwrap();
                    color_difference(gif, s.try_into().unwrap())
                })
                .fold(0.0, f64::max);
            // At most half of the largest step (0.020), plus rounding.
            assert!(worst <= 0.012, "seed={seed} worst={worst}");
        }
    }

    #[test]
    fn all_patterns_are_laid_out_with_transparent_gaps() {
        let gif = animation(16, &Pattern::ALL, 4).encode().unwrap();
        let decoded = decode(&gif);
        let (width, height, corners) = layout(Pattern::ALL.len(), 16, 4, 2);
        assert_eq!((decoded.width, decoded.height), (width, height));
        let last = &decoded.frames.last().unwrap().1;
        for (&pattern, &(left, top)) in Pattern::ALL.iter().zip(&corners) {
            let want = expected(16, 9, 300, pattern);
            for y in 0..16 {
                assert_eq!(last[(top + y) * width + left..][..16], want[y * 16..][..16]);
            }
        }
        // A gap column and the unused cell stay transparent.
        assert!((0..height).all(|y| last[y * width + 16] == TRANSPARENT));
        assert_eq!(last[height * width - 1], TRANSPARENT);
    }

    #[test]
    fn encoding_is_deterministic_and_carries_the_comment() {
        let patterns = [Pattern::Maze];
        let a = animation(20, &patterns, 5).encode().unwrap();
        assert_eq!(a, animation(20, &patterns, 5).encode().unwrap());
        assert!(a.windows(12).any(|w| w == b"test comment"));
        assert_eq!(&a[..6], b"GIF89a");
    }

    #[test]
    fn invalid_settings_are_rejected() {
        let patterns = [Pattern::Maze];
        let mut bad = animation(8, &patterns, 5);
        bad.fps = 0;
        assert!(bad.encode().is_err());
        bad.fps = MAX_FPS + 1;
        assert!(bad.encode().is_err());
        assert!(animation(8, &patterns, 302).encode().is_err());
    }
}
