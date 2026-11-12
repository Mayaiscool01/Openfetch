//! Optional animation engine: colour effects for any logo plus user-supplied
//! frame-based ASCII animations, rendered in place with ratatui.

use crate::bitmap::{Bitmap, TimedBitmap};
use crate::config::{expand_tilde, AnimationConfig};
use crate::logo::{Art, Parsed};
use crate::output::{Line, Span};
use crate::theme::{ColorDepth, ColorSpec};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as RLine, Span as RSpan, Text};
use ratatui::widgets::Paragraph;
use ratatui::{Terminal, TerminalOptions, Viewport};
use std::io;
use std::str::FromStr;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// A hue gradient sweeping diagonally across the logo.
    Rainbow,
    /// A bright band travelling across the logo's own colours.
    Wave,
    /// The logo's colours breathing in and out.
    Pulse,
    /// The logo turns around its vertical axis like a coin.
    Spin,
    /// Frames from a GIF, or from `animation.file` / `animation.frames`.
    Custom,
}

impl FromStr for Effect {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "rainbow" => Ok(Effect::Rainbow),
            "wave" => Ok(Effect::Wave),
            "pulse" => Ok(Effect::Pulse),
            "spin" => Ok(Effect::Spin),
            "custom" | "frames" | "gif" => Ok(Effect::Custom),
            other => Err(format!(
                "unknown animation effect '{other}' (use rainbow, wave, pulse, spin or custom)"
            )),
        }
    }
}

/// Convert an HSV colour (h in degrees) to RGB.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let f = |n: f32| ((n + m) * 255.0).round() as u8;
    (f(r), f(g), f(b))
}

/// Split a text file into frames; frames are separated by a line that is `%%`.
pub fn parse_frames(text: &str) -> Vec<Parsed> {
    let mut frames = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.trim() == "%%" {
            frames.push(std::mem::take(&mut current));
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    frames.push(current);
    frames
        .iter()
        .map(|f| Parsed::parse(f))
        .filter(|p| p.height() > 0)
        .collect()
}

/// Build an animation for `name` (`auto`, `rainbow`, `wave`, `pulse`, `spin`,
/// `custom`). `auto` plays GIF frames, then configured custom frames, and
/// otherwise falls back to rainbow.
pub fn build(
    name: &str,
    art: &Art,
    palette: &[ColorSpec],
    cfg: &AnimationConfig,
) -> Result<Animation, String> {
    let has_custom = cfg.file.is_some() || cfg.frames.is_some();
    let effect = if name.trim().eq_ignore_ascii_case("auto") {
        match art {
            Art::Images(f) if f.len() > 1 => Effect::Custom,
            Art::Cells(_) if has_custom => Effect::Custom,
            _ => Effect::Rainbow,
        }
    } else {
        name.parse()?
    };
    let source = match (effect, art) {
        (Effect::Custom, Art::Cells(_)) => Source::Cells(load_custom_frames(cfg)?),
        (_, Art::Images(f)) => Source::Images(f.clone()),
        (_, Art::Cells(p)) => Source::Cells(vec![p.clone()]),
    };
    Ok(Animation::from_source(
        effect,
        source,
        palette.to_vec(),
        cfg.fps,
        cfg.speed,
    ))
}

/// Load custom frames from the config (`file` and/or inline `frames`).
pub fn load_custom_frames(cfg: &AnimationConfig) -> Result<Vec<Parsed>, String> {
    let mut frames = Vec::new();
    if let Some(f) = &cfg.file {
        let path = expand_tilde(f);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read animation file {}: {e}", path.display()))?;
        frames.extend(parse_frames(&text));
    }
    if let Some(list) = &cfg.frames {
        frames.extend(
            list.iter()
                .map(|f| Parsed::parse(f))
                .filter(|p| p.height() > 0),
        );
    }
    if frames.is_empty() {
        return Err("effect 'custom' needs animation.file or animation.frames".into());
    }
    Ok(frames)
}

/// Where the frames come from.
pub enum Source {
    /// Text/block-art logos (one frame, or several for custom animations).
    Cells(Vec<Parsed>),
    /// Pixel images with per-frame delays (PNG/JPEG = one frame, GIF = many).
    Images(Vec<TimedBitmap>),
}

pub struct Animation {
    pub effect: Effect,
    source: Source,
    /// First image frame, for the pixel effects (spin and tints on pictures).
    base: Bitmap,
    palette: Vec<ColorSpec>,
    pub fps: u32,
    pub speed: f32,
    width: usize,
    height: usize,
}

/// Seconds per full turn of the spin effect.
const SPIN_PERIOD: f32 = 3.0;

fn tint(effect: Effect, t: f32, w: usize, x: usize, y: usize, c: (u8, u8, u8)) -> (u8, u8, u8) {
    let base = ColorSpec::Rgb(c.0, c.1, c.2);
    let out = match effect {
        Effect::Rainbow => {
            let (r, g, b) = hsv_to_rgb(x as f32 * 7.0 + y as f32 * 7.0 - t * 90.0, 0.65, 1.0);
            base.mix(ColorSpec::Rgb(r, g, b), 0.6)
        }
        Effect::Wave => {
            let pos = (t * 22.0).rem_euclid(w as f32 + 30.0) - 15.0;
            let d = (x as f32 + y as f32 * 0.4 - pos).abs();
            base.mix(
                ColorSpec::Rgb(255, 255, 255),
                (1.0 - d / 8.0).clamp(0.0, 1.0) * 0.85,
            )
        }
        _ => base.scale(((t * 2.2).sin() * 0.5 + 0.5) * 0.55 + 0.45),
    };
    out.to_rgb()
}

/// Squeeze a text logo around its vertical axis (see [`Bitmap::spin`]).
/// Returns the turned cells and the brightness to draw them with.
fn spin_cells(p: &Parsed, width: usize, angle: f32) -> (Parsed, f32) {
    let cos = angle.cos();
    let scale = cos.abs().max(0.12);
    let centre = (width as f32 - 1.0) / 2.0;
    let facing = 0.55 + 0.45 * cos.abs();
    let shade = if cos < 0.0 { facing * 0.75 } else { facing };
    let blank = crate::logo::Cell { ch: ' ', slot: 1 };
    let rows = p
        .rows
        .iter()
        .map(|row| {
            (0..width)
                .map(|x| {
                    let mut sx = centre + (x as f32 - centre) / scale;
                    if cos < 0.0 {
                        sx = (width as f32 - 1.0) - sx;
                    }
                    // one cell of slack so an edge-on logo never vanishes
                    if sx < -1.0 || sx > width as f32 {
                        return blank;
                    }
                    let sx = sx.round().clamp(0.0, width as f32 - 1.0) as usize;
                    row.get(sx).copied().unwrap_or(blank)
                })
                .collect()
        })
        .collect();
    (Parsed { rows }, shade)
}

impl Animation {
    #[cfg(test)]
    pub fn new(
        effect: Effect,
        frames: Vec<Parsed>,
        palette: Vec<ColorSpec>,
        fps: u32,
        speed: f32,
    ) -> Animation {
        Animation::from_source(effect, Source::Cells(frames), palette, fps, speed)
    }

    pub fn from_source(
        effect: Effect,
        source: Source,
        palette: Vec<ColorSpec>,
        fps: u32,
        speed: f32,
    ) -> Animation {
        let (width, height, base) = match &source {
            Source::Cells(frames) => {
                assert!(!frames.is_empty(), "an animation needs at least one frame");
                (
                    frames.iter().map(Parsed::width).max().unwrap_or(0),
                    frames.iter().map(Parsed::height).max().unwrap_or(0),
                    Bitmap::default(),
                )
            }
            Source::Images(frames) => {
                assert!(!frames.is_empty(), "an animation needs at least one frame");
                (
                    frames.iter().map(|(b, _)| b.w).max().unwrap_or(0),
                    frames.iter().map(|(b, _)| b.rows()).max().unwrap_or(0),
                    frames[0].0.clone(),
                )
            }
        };
        Animation {
            effect,
            source,
            base,
            palette,
            fps: fps.clamp(1, 60),
            speed: if speed.is_finite() && speed > 0.0 {
                speed
            } else {
                1.0
            },
            width,
            height,
        }
    }

    #[cfg(test)]
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Number of frames the animation steps through (1 for pure colour effects).
    #[cfg(test)]
    pub fn frame_count(&self) -> usize {
        match &self.source {
            Source::Cells(f) => f.len(),
            Source::Images(f) => f.len(),
        }
    }

    /// Logo lines at animation time `t` (seconds, already speed-scaled).
    /// Every frame has identical dimensions so the layout never jumps.
    pub fn frame(&self, t: f32) -> Vec<Line> {
        let mut lines = match (self.effect, &self.source) {
            (Effect::Custom, Source::Cells(frames)) => {
                frames[(t * self.fps as f32) as usize % frames.len()].render(&self.palette)
            }
            (Effect::Custom, Source::Images(frames)) => {
                let total: f32 = frames.iter().map(|(_, d)| *d).sum();
                let mut at = t.rem_euclid(total.max(0.001));
                let mut pick = &frames[0].0;
                for (b, d) in frames {
                    pick = b;
                    if at < *d {
                        break;
                    }
                    at -= *d;
                }
                pick.render()
            }
            (Effect::Spin, Source::Images(_)) => self
                .base
                .spin(t / SPIN_PERIOD * std::f32::consts::TAU)
                .render(),
            (Effect::Spin, Source::Cells(frames)) => {
                // Text logos turn cell by cell so their characters survive.
                let (turned, shade) = spin_cells(
                    &frames[0],
                    self.width,
                    t / SPIN_PERIOD * std::f32::consts::TAU,
                );
                turned.render_with(|_, _, cell| {
                    Some(
                        Parsed::slot_color(&self.palette, cell.slot)
                            .unwrap_or(ColorSpec::Ansi(7))
                            .scale(shade),
                    )
                })
            }
            (effect, Source::Images(_)) => {
                let w = self.base.w;
                self.base
                    .map(|x, y, c| tint(effect, t, w, x, y, c))
                    .render()
            }
            (effect, Source::Cells(frames)) => {
                let w = self.width;
                frames[0].render_with(|x, y, cell| {
                    let slot = Parsed::slot_color(&self.palette, cell.slot);
                    Some(match effect {
                        Effect::Rainbow => {
                            let hue = x as f32 * 7.0 + y as f32 * 14.0 - t * 90.0;
                            let (r, g, b) = hsv_to_rgb(hue, 0.65, 1.0);
                            ColorSpec::Rgb(r, g, b)
                        }
                        Effect::Wave => {
                            let color = slot.unwrap_or(ColorSpec::Ansi(7));
                            let pos = (t * 22.0).rem_euclid(w as f32 + 30.0) - 15.0;
                            let d = (x as f32 + y as f32 * 0.8 - pos).abs();
                            let k = (1.0 - d / 8.0).clamp(0.0, 1.0);
                            color.mix(ColorSpec::Rgb(255, 255, 255), k * 0.85)
                        }
                        _ => {
                            let color = slot.unwrap_or(ColorSpec::Ansi(7));
                            let breathe = ((t * 2.2).sin() * 0.5 + 0.5) * 0.55 + 0.45;
                            color.scale(breathe)
                        }
                    })
                })
            }
        };
        lines.resize(self.height, Vec::new());
        for l in &mut lines {
            let have: usize = l.iter().map(|s| s.text.chars().count()).sum();
            if have < self.width {
                l.push(Span::plain(" ".repeat(self.width - have)));
            }
        }
        lines
    }
}

pub fn ratatui_color(c: ColorSpec, depth: ColorDepth) -> Color {
    match c.downgrade(depth) {
        ColorSpec::Ansi(n) | ColorSpec::Indexed(n) => Color::Indexed(n),
        ColorSpec::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

pub fn to_ratatui(lines: &[Line], depth: ColorDepth) -> Text<'static> {
    Text::from(
        lines
            .iter()
            .map(|line| {
                RLine::from(
                    line.iter()
                        .map(|s| {
                            let mut style = Style::default();
                            if depth != ColorDepth::None {
                                if let Some(c) = s.color {
                                    style = style.fg(ratatui_color(c, depth));
                                }
                                if let Some(c) = s.bg {
                                    style = style.bg(ratatui_color(c, depth));
                                }
                                if s.bold {
                                    style = style.add_modifier(Modifier::BOLD);
                                }
                            }
                            RSpan::styled(s.text.clone(), style)
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
    )
}

/// Restores the terminal even if rendering panics or fails.
struct RawGuard;

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Run the animation inline (below the current prompt) until a key is pressed
/// or `duration` seconds have passed (0 = no limit). The last frame stays in
/// the scrollback, like a normal fetch run.
pub fn run(
    anim: &Animation,
    duration: f32,
    depth: ColorDepth,
    compose: &dyn Fn(Vec<Line>) -> Vec<Line>,
) -> io::Result<()> {
    let first = compose(anim.frame(0.0));
    let term_h = crossterm::terminal::size()
        .map(|(_, h)| h as usize)
        .unwrap_or(24);
    let height = first.len().min(term_h.saturating_sub(1)).max(1) as u16;

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        default_hook(info);
    }));
    enable_raw_mode()?;
    let _guard = RawGuard;
    let backend = CrosstermBackend::new(io::stdout());
    let mut term = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )?;

    let start = Instant::now();
    let tick = Duration::from_millis(1000 / anim.fps as u64);
    loop {
        let elapsed = start.elapsed().as_secs_f32();
        let lines = compose(anim.frame(elapsed * anim.speed));
        let text = to_ratatui(&lines, depth);
        term.draw(|f| f.render_widget(Paragraph::new(text), f.size()))?;
        if duration > 0.0 && elapsed >= duration {
            break;
        }
        if event::poll(tick)? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    break;
                }
            }
        }
    }

    // Leave the cursor on the line below the final frame.
    let area = term.get_frame().size();
    term.set_cursor(0, area.bottom().saturating_sub(1))?;
    term.show_cursor()?;
    drop(term);
    drop(_guard);
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Vec<ColorSpec> {
        vec![ColorSpec::Rgb(200, 0, 0), ColorSpec::Rgb(0, 0, 200)]
    }

    fn logo() -> Parsed {
        Parsed::parse("$1ab\n$2cdef\n")
    }

    fn plain(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn effect_names() {
        assert_eq!("Rainbow".parse(), Ok(Effect::Rainbow));
        assert_eq!("wave".parse(), Ok(Effect::Wave));
        assert_eq!("pulse".parse(), Ok(Effect::Pulse));
        assert_eq!("custom".parse(), Ok(Effect::Custom));
        assert_eq!("gif".parse(), Ok(Effect::Custom));
        assert_eq!("spin".parse(), Ok(Effect::Spin));
        assert!("sparkle".parse::<Effect>().is_err());
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), (0, 255, 0));
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), (0, 0, 255));
        assert_eq!(hsv_to_rgb(360.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(-120.0, 1.0, 1.0), (0, 0, 255));
        assert_eq!(hsv_to_rgb(77.0, 0.0, 1.0), (255, 255, 255));
    }

    #[test]
    fn frames_are_split_on_marker() {
        let frames = parse_frames("aa\nbb\n%%\ncc\n%%\n\n");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].height(), 2);
        assert_eq!(frames[1].height(), 1);
    }

    #[test]
    fn effects_keep_text_and_size_constant() {
        for effect in [Effect::Rainbow, Effect::Wave, Effect::Pulse] {
            let a = Animation::new(effect, vec![logo()], palette(), 15, 1.0);
            assert_eq!(a.size(), (4, 2));
            for t in [0.0, 0.7, 3.3, 100.0] {
                let f = a.frame(t);
                assert_eq!(
                    plain(&f),
                    vec!["ab  ".to_string(), "cdef".to_string()],
                    "{effect:?}@{t}"
                );
            }
        }
    }

    #[test]
    fn effects_change_colours_over_time() {
        for effect in [Effect::Rainbow, Effect::Wave, Effect::Pulse] {
            let a = Animation::new(effect, vec![logo()], palette(), 15, 1.0);
            assert_ne!(a.frame(0.0), a.frame(0.4), "{effect:?} should animate");
        }
    }

    #[test]
    fn custom_frames_cycle_and_are_padded_to_equal_size() {
        let frames = parse_frames("$1ab\n%%\n$1xyz\n$1uvw\n");
        let a = Animation::new(Effect::Custom, frames, palette(), 10, 1.0);
        assert_eq!(a.size(), (3, 2));
        assert_eq!(plain(&a.frame(0.0)), vec!["ab ", "   "]);
        assert_eq!(plain(&a.frame(0.1)), vec!["xyz", "uvw"]);
        assert_eq!(plain(&a.frame(0.2)), vec!["ab ", "   "]);
    }

    #[test]
    fn custom_frames_from_config() {
        let cfg = AnimationConfig {
            frames: Some(vec!["a".into(), "b".into()]),
            ..Default::default()
        };
        assert_eq!(load_custom_frames(&cfg).unwrap().len(), 2);
        assert!(load_custom_frames(&AnimationConfig::default()).is_err());
        let bad = AnimationConfig {
            file: Some("/no/such/file".into()),
            ..Default::default()
        };
        assert!(load_custom_frames(&bad).is_err());
    }

    fn image_frames() -> Vec<TimedBitmap> {
        let mut a = Bitmap::new(6, 4);
        let mut b = Bitmap::new(6, 4);
        for x in 0..6 {
            for y in 0..4 {
                a.set(x, y, Some((200, 50, 50)));
                b.set(x, y, Some((50, 50, 200)));
            }
        }
        vec![(a, 0.5), (b, 0.25)]
    }

    #[test]
    fn spin_turns_block_art_and_keeps_the_size() {
        let art = Parsed::parse("$1████████\n$1████████\n");
        let a = Animation::new(Effect::Spin, vec![art], palette(), 15, 1.0);
        assert_eq!(a.size(), (8, 2));
        let lit = |t: f32| -> usize {
            a.frame(t)
                .iter()
                .flatten()
                .filter(|s| s.color.is_some())
                .map(|s| s.text.chars().count())
                .sum()
        };
        assert!(lit(0.0) > lit(0.75), "edge-on (quarter turn) is thinner");
        assert_eq!(lit(0.0), lit(3.0), "one full turn every 3 seconds");
        for t in [0.0, 0.3, 0.75, 1.4, 2.9] {
            let f = a.frame(t);
            assert_eq!(f.len(), 2);
            assert!(f
                .iter()
                .all(|l| l.iter().map(|s| s.text.chars().count()).sum::<usize>() == 8));
        }
    }

    #[test]
    fn spin_keeps_the_characters_of_ascii_logos() {
        let a = Animation::new(
            Effect::Spin,
            vec![Parsed::parse("$1abcdefgh\n")],
            palette(),
            15,
            1.0,
        );
        assert_eq!(plain(&a.frame(0.0)), vec!["abcdefgh"]);
        let back = plain(&a.frame(1.5)); // half a turn: mirrored
        assert_eq!(back, vec!["hgfedcba"]);
        let edge = plain(&a.frame(0.75));
        assert!(
            edge[0].trim().len() <= 3 && !edge[0].trim().is_empty(),
            "{edge:?}"
        );
        assert!(edge[0].trim().chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn gif_frames_follow_their_delays() {
        let a = Animation::from_source(
            Effect::Custom,
            Source::Images(image_frames()),
            vec![],
            15,
            1.0,
        );
        assert_eq!(a.frame_count(), 2);
        let colour = |t: f32| a.frame(t)[0][0].color;
        assert_eq!(colour(0.0), Some(ColorSpec::Rgb(200, 50, 50)));
        assert_eq!(colour(0.49), Some(ColorSpec::Rgb(200, 50, 50)));
        assert_eq!(colour(0.6), Some(ColorSpec::Rgb(50, 50, 200)));
        assert_eq!(
            colour(0.8),
            Some(ColorSpec::Rgb(200, 50, 50)),
            "loops after 0.75 s"
        );
    }

    #[test]
    fn colour_effects_tint_images() {
        let frames = vec![image_frames().remove(0)];
        for effect in [Effect::Rainbow, Effect::Wave, Effect::Pulse] {
            let a = Animation::from_source(effect, Source::Images(frames.clone()), vec![], 15, 1.0);
            assert_ne!(a.frame(0.0), a.frame(0.4), "{effect:?}");
            assert_eq!(a.size(), (6, 2));
        }
    }

    #[test]
    fn build_picks_the_right_source() {
        let cells = Art::Cells(logo());
        let imgs = Art::Images(image_frames());
        let cfg = AnimationConfig::default();
        assert_eq!(
            build("auto", &cells, &palette(), &cfg).unwrap().effect,
            Effect::Rainbow
        );
        assert_eq!(
            build("auto", &imgs, &palette(), &cfg).unwrap().effect,
            Effect::Custom
        );
        assert_eq!(
            build("spin", &imgs, &palette(), &cfg).unwrap().effect,
            Effect::Spin
        );
        let with_frames = AnimationConfig {
            frames: Some(vec!["a".into(), "b".into()]),
            ..Default::default()
        };
        let a = build("auto", &cells, &palette(), &with_frames).unwrap();
        assert_eq!((a.effect, a.frame_count()), (Effect::Custom, 2));
        assert!(
            build("custom", &cells, &palette(), &cfg).is_err(),
            "custom needs frames"
        );
        assert!(build("sparkle", &cells, &palette(), &cfg).is_err());
    }

    #[test]
    fn speed_and_fps_are_sanitised() {
        let a = Animation::new(Effect::Pulse, vec![logo()], palette(), 0, f32::NAN);
        assert_eq!(a.fps, 1);
        assert_eq!(a.speed, 1.0);
        let b = Animation::new(Effect::Pulse, vec![logo()], palette(), 9999, -2.0);
        assert_eq!(b.fps, 60);
        assert_eq!(b.speed, 1.0);
    }
}
