//! Pixel bitmaps drawn with half-block characters. Used for custom images and
//! GIFs as logos, and for effects that need real pixels (spin).
//!
//! Every terminal cell shows two vertically stacked square-ish pixels:
//! `▀` with foreground = top pixel and background = bottom pixel.

#[cfg(test)]
use crate::logo::Parsed;
use crate::output::{Line, Span};
use crate::theme::ColorSpec;
use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, RgbaImage};
use std::path::Path;

pub type Rgb = (u8, u8, u8);

/// A bitmap: `h` is the number of pixel rows (two per terminal row).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bitmap {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Option<Rgb>>,
}

/// One animation frame and how long to show it, in seconds.
pub type TimedBitmap = (Bitmap, f32);

impl Bitmap {
    pub fn new(w: usize, h: usize) -> Bitmap {
        Bitmap {
            w,
            h,
            px: vec![None; w * h],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<Rgb> {
        if x < self.w && y < self.h {
            self.px[y * self.w + x]
        } else {
            None
        }
    }

    pub fn set(&mut self, x: usize, y: usize, c: Option<Rgb>) {
        if x < self.w && y < self.h {
            self.px[y * self.w + x] = c;
        }
    }

    /// Build from a grid of palette slots (0 = empty), as used by the
    /// built-in block logos.
    pub fn from_slots(grid: &[Vec<u8>], palette: &[ColorSpec]) -> Bitmap {
        let w = grid.iter().map(Vec::len).max().unwrap_or(0);
        let mut bm = Bitmap::new(w, grid.len() + grid.len() % 2);
        for (y, row) in grid.iter().enumerate() {
            for (x, slot) in row.iter().enumerate() {
                if *slot > 0 {
                    let c = if palette.is_empty() {
                        (229, 229, 229)
                    } else {
                        palette[(*slot as usize - 1) % palette.len()].to_rgb()
                    };
                    bm.set(x, y, Some(c));
                }
            }
        }
        bm
    }

    /// Terminal rows needed to draw this bitmap.
    pub fn rows(&self) -> usize {
        self.h.div_ceil(2)
    }

    /// Turn text/block-art logo cells back into pixels. `▀`, `▄` and `█` map
    /// to their exact pixels; any other visible glyph fills the whole cell.
    #[cfg(test)]
    pub fn from_parsed(p: &Parsed, palette: &[ColorSpec]) -> Bitmap {
        let mut bm = Bitmap::new(p.width(), p.height() * 2);
        for (y, row) in p.rows.iter().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                let (top, bottom) = match cell.ch {
                    ' ' => continue,
                    '▀' => (true, false),
                    '▄' => (false, true),
                    _ => (true, true),
                };
                let color = Parsed::slot_color(palette, cell.slot)
                    .unwrap_or(ColorSpec::Ansi(7))
                    .to_rgb();
                if top {
                    bm.set(x, y * 2, Some(color));
                }
                if bottom {
                    bm.set(x, y * 2 + 1, Some(color));
                }
            }
        }
        bm
    }

    /// Scale an RGBA image to `cols` terminal columns (pixels stay square).
    /// Transparent pixels stay empty.
    pub fn from_image(img: &RgbaImage, cols: usize, max_rows: usize) -> Bitmap {
        let (iw, ih) = (img.width().max(1) as f64, img.height().max(1) as f64);
        let mut w = cols.max(2) as f64;
        let mut h = (w * ih / iw).round().max(2.0);
        let max_h = (max_rows.max(1) * 2) as f64;
        if h > max_h {
            w = (w * max_h / h).round().max(2.0);
            h = max_h;
        }
        let h = ((h as u32) & !1).max(2); // even number of pixel rows
        let scaled = image::imageops::resize(img, w as u32, h, FilterType::Triangle);
        let mut bm = Bitmap::new(scaled.width() as usize, scaled.height() as usize);
        for (x, y, p) in scaled.enumerate_pixels() {
            if p[3] >= 40 {
                bm.set(x as usize, y as usize, Some((p[0], p[1], p[2])));
            }
        }
        bm
    }

    /// Draw with half-blocks.
    pub fn render(&self) -> Vec<Line> {
        (0..self.rows())
            .map(|r| {
                let mut line: Line = Vec::new();
                for x in 0..self.w {
                    let top = self.get(x, r * 2);
                    let bottom = self.get(x, r * 2 + 1);
                    let span = match (top, bottom) {
                        (None, None) => Span::plain(" "),
                        (Some(t), None) => Span::new("▀", rgb(t)),
                        (None, Some(b)) => Span::new("▄", rgb(b)),
                        (Some(t), Some(b)) if t == b => Span::new("█", rgb(t)),
                        (Some(t), Some(b)) => Span::new("▀", rgb(t)).on(rgb(b)),
                    };
                    match line.last_mut() {
                        Some(last)
                            if last.color == span.color
                                && last.bg == span.bg
                                && last.text.chars().next() == span.text.chars().next() =>
                        {
                            last.text.push_str(&span.text)
                        }
                        _ => line.push(span),
                    }
                }
                line
            })
            .collect()
    }

    /// Recolour every visible pixel with `f(x, y, colour)`.
    pub fn map(&self, f: impl Fn(usize, usize, Rgb) -> Rgb) -> Bitmap {
        let mut out = self.clone();
        for y in 0..self.h {
            for x in 0..self.w {
                if let Some(c) = self.get(x, y) {
                    out.set(x, y, Some(f(x, y, c)));
                }
            }
        }
        out
    }

    /// Rotate around the vertical axis by `angle` radians (a flat "coin spin").
    /// The back side is mirrored and shaded darker; edge-on shows a thin line.
    pub fn spin(&self, angle: f32) -> Bitmap {
        let cos = angle.cos();
        let scale = cos.abs().max(0.12);
        let centre = (self.w as f32 - 1.0) / 2.0;
        let facing = 0.55 + 0.45 * cos.abs();
        let shade = if cos < 0.0 { facing * 0.75 } else { facing };
        let mut out = Bitmap::new(self.w, self.h);
        for x in 0..self.w {
            let mut sx = centre + (x as f32 - centre) / scale;
            if cos < 0.0 {
                sx = (self.w as f32 - 1.0) - sx;
            }
            // one pixel of slack so an edge-on logo never vanishes
            if sx < -1.0 || sx > self.w as f32 {
                continue;
            }
            let sx = sx.round().clamp(0.0, self.w as f32 - 1.0) as usize;
            for y in 0..self.h {
                if let Some((r, g, b)) = self.get(sx, y) {
                    let f = |v: u8| (v as f32 * shade).round() as u8;
                    out.set(x, y, Some((f(r), f(g), f(b))));
                }
            }
        }
        out
    }
}

fn rgb((r, g, b): Rgb) -> ColorSpec {
    ColorSpec::Rgb(r, g, b)
}

/// Load a PNG/JPEG (one frame) or GIF (all frames) as bitmaps.
pub fn load_image(path: &Path, cols: usize, max_rows: usize) -> Result<Vec<TimedBitmap>, String> {
    let is_gif = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gif"));
    if is_gif {
        let file = std::fs::File::open(path)
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let decoder = GifDecoder::new(std::io::BufReader::new(file))
            .map_err(|e| format!("cannot decode {}: {e}", path.display()))?;
        let frames = decoder
            .into_frames()
            .collect_frames()
            .map_err(|e| format!("cannot decode {}: {e}", path.display()))?;
        if frames.is_empty() {
            return Err(format!("{} has no frames", path.display()));
        }
        let mut out: Vec<TimedBitmap> = frames
            .iter()
            .map(|f| {
                let (n, d) = f.delay().numer_denom_ms();
                let secs = if d == 0 {
                    0.1
                } else {
                    n as f32 / d as f32 / 1000.0
                };
                (
                    Bitmap::from_image(f.buffer(), cols, max_rows),
                    secs.max(0.02),
                )
            })
            .collect();
        // keep the bitmap size identical across frames
        let (w, h) = out
            .iter()
            .fold((0, 0), |(w, h), (b, _)| (w.max(b.w), h.max(b.h)));
        for (b, _) in &mut out {
            if b.w != w || b.h != h {
                let mut padded = Bitmap::new(w, h);
                for y in 0..b.h {
                    for x in 0..b.w {
                        padded.set(x, y, b.get(x, y));
                    }
                }
                *b = padded;
            }
        }
        Ok(out)
    } else {
        let img = image::open(path)
            .map_err(|e| format!("cannot load image {}: {e}", path.display()))?
            .to_rgba8();
        Ok(vec![(Bitmap::from_image(&img, cols, max_rows), 0.1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn text(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn decodes_block_art_to_pixels_and_back() {
        let p = Parsed::parse("$1▀▄█ x");
        let bm = Bitmap::from_parsed(&p, &[ColorSpec::Rgb(10, 20, 30)]);
        assert_eq!((bm.w, bm.h), (5, 2));
        let c = Some((10, 20, 30));
        assert_eq!((bm.get(0, 0), bm.get(0, 1)), (c, None));
        assert_eq!((bm.get(1, 0), bm.get(1, 1)), (None, c));
        assert_eq!((bm.get(2, 0), bm.get(2, 1)), (c, c));
        assert_eq!((bm.get(3, 0), bm.get(3, 1)), (None, None));
        assert_eq!(
            (bm.get(4, 0), bm.get(4, 1)),
            (c, c),
            "plain glyphs fill the cell"
        );
        assert_eq!(text(&bm.render()), vec!["▀▄█ █"]);
    }

    #[test]
    fn builds_from_slot_grids() {
        let pal = [ColorSpec::Rgb(1, 1, 1), ColorSpec::Rgb(2, 2, 2)];
        let bm = Bitmap::from_slots(&[vec![1, 0, 2], vec![2, 0, 3]], &pal);
        assert_eq!((bm.w, bm.h), (3, 2));
        assert_eq!(bm.get(0, 0), Some((1, 1, 1)));
        assert_eq!(bm.get(1, 0), None);
        assert_eq!(
            bm.get(2, 1),
            Some((1, 1, 1)),
            "slots wrap around the palette"
        );
        assert_eq!(
            Bitmap::from_slots(&[vec![1]], &[]).h,
            2,
            "odd heights are padded"
        );
    }

    #[test]
    fn two_colour_cells_use_background() {
        let mut bm = Bitmap::new(1, 2);
        bm.set(0, 0, Some((255, 0, 0)));
        bm.set(0, 1, Some((0, 0, 255)));
        let line = &bm.render()[0];
        assert_eq!(line[0].text, "▀");
        assert_eq!(line[0].color, Some(ColorSpec::Rgb(255, 0, 0)));
        assert_eq!(line[0].bg, Some(ColorSpec::Rgb(0, 0, 255)));
    }

    #[test]
    fn scales_images_with_square_pixels_and_transparency() {
        let mut img = RgbaImage::new(40, 20);
        for y in 0..20 {
            for x in 0..40 {
                let alpha = if x < 20 { 255 } else { 0 };
                img.put_pixel(x, y, Rgba([200, 100, 50, alpha]));
            }
        }
        let bm = Bitmap::from_image(&img, 20, 30);
        assert_eq!((bm.w, bm.h), (20, 10), "aspect ratio is kept");
        assert!(bm.get(2, 5).is_some());
        assert!(bm.get(18, 5).is_none(), "transparent half stays empty");
        let tall = RgbaImage::from_pixel(10, 100, Rgba([1, 2, 3, 255]));
        let bm = Bitmap::from_image(&tall, 20, 5);
        assert!(bm.h <= 10 && bm.h % 2 == 0, "{}x{}", bm.w, bm.h);
    }

    #[test]
    fn spin_squeezes_mirrors_and_shades() {
        let mut bm = Bitmap::new(10, 2);
        for x in 0..10 {
            bm.set(x, 0, Some((200, 200, 200)));
        }
        bm.set(0, 1, Some((255, 0, 0))); // marker at the left edge
        assert_eq!(bm.spin(0.0), bm, "angle 0 is unchanged");
        let edge = bm.spin(std::f32::consts::FRAC_PI_2);
        let lit = (0..10).filter(|x| edge.get(*x, 0).is_some()).count();
        assert!((1..=4).contains(&lit), "edge-on is a thin sliver: {lit}");
        let half = bm.spin(std::f32::consts::FRAC_PI_3);
        let lit = (0..10).filter(|x| half.get(*x, 0).is_some()).count();
        assert!(lit < 10 && lit > 2);
        assert!(half.get(0, 0).is_none() && half.get(9, 0).is_none());
        let back = bm.spin(std::f32::consts::PI);
        assert!(back.get(0, 1).is_none(), "back side is mirrored");
        let (r, _, _) = back.get(9, 1).unwrap();
        assert!(r < 255, "back face is shaded");
    }

    #[test]
    fn map_recolours_visible_pixels_only() {
        let mut bm = Bitmap::new(2, 2);
        bm.set(0, 0, Some((10, 10, 10)));
        let out = bm.map(|_, _, (r, g, b)| (r + 1, g + 1, b + 1));
        assert_eq!(out.get(0, 0), Some((11, 11, 11)));
        assert_eq!(out.get(1, 1), None);
    }

    #[test]
    fn loads_png_and_reports_errors() {
        let dir = std::env::temp_dir().join(format!("openfetch-bitmap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        RgbaImage::from_pixel(8, 8, Rgba([9, 9, 9, 255]))
            .save(&png)
            .unwrap();
        let frames = load_image(&png, 6, 10).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0.w, 6);
        assert!(load_image(&dir.join("missing.png"), 6, 10).is_err());
        std::fs::write(dir.join("bad.png"), b"not an image").unwrap();
        assert!(load_image(&dir.join("bad.png"), 6, 10).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_animated_gif_frames_with_equal_sizes() {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame};
        let dir = std::env::temp_dir().join(format!("openfetch-gif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.gif");
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut enc = GifEncoder::new(file);
            for shade in [50u8, 200u8] {
                let img = RgbaImage::from_pixel(12, 12, Rgba([shade, 0, 0, 255]));
                enc.encode_frame(Frame::from_parts(
                    img,
                    0,
                    0,
                    Delay::from_numer_denom_ms(200, 1),
                ))
                .unwrap();
            }
        }
        let frames = load_image(&path, 8, 10).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(
            (frames[0].0.w, frames[0].0.h),
            (frames[1].0.w, frames[1].0.h)
        );
        assert!((frames[0].1 - 0.2).abs() < 0.001, "{}", frames[0].1);
        assert_ne!(frames[0].0.get(1, 1), frames[1].0.get(1, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
