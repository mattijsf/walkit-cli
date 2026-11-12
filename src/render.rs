// SPDX-License-Identifier: GPL-3.0-or-later

//! A software renderer for the engine's draw list. It draws into a buffer of ARGB pixels
//! that starts out transparent (or in one color), at a whole-number scale: every layout pixel
//! is `scale` device pixels across and down, and pictures scale by nearest pixels.

use walkit::draw::{DrawCmd, Gamma, Rect, Region, TextCmd};

use walkit::truetype::{em, Style};

use crate::host::Assets;

/// The image being drawn: `w` x `h` device pixels, rows top first, each `0xAARRGGBB` with
/// the color not multiplied by the alpha.
pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub pixels: Vec<u32>,
}

impl Canvas {
    /// A canvas filled with `background` (`0xAARRGGBB`; 0 is transparent).
    pub fn new(w: i32, h: i32, background: u32) -> Self {
        Canvas {
            w,
            h,
            pixels: vec![background; (w.max(0) * h.max(0)) as usize],
        }
    }

    /// The canvas cut to the pixels something drew on; `None` when nothing did.
    pub fn trimmed(&self) -> Option<Canvas> {
        let drawn = |x: i32, y: i32| self.pixels[(y * self.w + x) as usize] >> 24 != 0;
        let rows: Vec<i32> = (0..self.h)
            .filter(|&y| (0..self.w).any(|x| drawn(x, y)))
            .collect();
        let columns: Vec<i32> = (0..self.w)
            .filter(|&x| (0..self.h).any(|y| drawn(x, y)))
            .collect();
        let (top, bottom) = (*rows.first()?, *rows.last()?);
        let (left, right) = (*columns.first()?, *columns.last()?);
        let mut pixels = Vec::with_capacity(((right - left + 1) * (bottom - top + 1)) as usize);
        for y in top..=bottom {
            let row = (y * self.w) as usize;
            pixels.extend_from_slice(&self.pixels[row + left as usize..=row + right as usize]);
        }
        Some(Canvas {
            w: right - left + 1,
            h: bottom - top + 1,
            pixels,
        })
    }

    /// Lays `background` (`0xAARRGGBB`) under everything drawn.
    pub fn put_behind(&mut self, background: u32) {
        let back_a = background >> 24;
        for p in &mut self.pixels {
            let a = *p >> 24;
            let keep = back_a * (255 - a) / 255;
            let out_a = a + keep;
            if out_a == 0 {
                continue;
            }
            let mix = |shift: u32| {
                let s = (*p >> shift) & 0xff;
                let d = (background >> shift) & 0xff;
                ((s * a + d * keep) / out_a) << shift
            };
            *p = (out_a << 24) | mix(16) | mix(8) | mix(0);
        }
    }

    /// The pixels as RGBA bytes, as PNG stores them.
    pub fn rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for p in &self.pixels {
            let a = (p >> 24) as u8;
            // a pixel nothing drew on is plain transparent black
            if a == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, a]);
            }
        }
        out
    }
}

/// Which device pixels of a shaped window show: one flag per pixel of its box.
struct Mask {
    area: Rect,
    bits: Vec<bool>,
}

impl Mask {
    fn empty(area: Rect) -> Self {
        Mask {
            area,
            bits: vec![false; (area.width.max(0) * area.height.max(0)) as usize],
        }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        self.area
            .contains(x, y)
            .then(|| ((y - self.area.top) * self.area.width + (x - self.area.left)) as usize)
    }

    fn get(&self, x: i32, y: i32) -> bool {
        self.index(x, y).is_some_and(|i| self.bits[i])
    }

    fn set(&mut self, x: i32, y: i32) {
        if let Some(i) = self.index(x, y) {
            self.bits[i] = true;
        }
    }

    /// Combines `other` into this mask, pixel by pixel.
    fn merge(&mut self, other: &Mask, op: impl Fn(bool, bool) -> bool) {
        for (a, b) in self.bits.iter_mut().zip(&other.bits) {
            *a = op(*a, *b);
        }
    }
}

/// Where the commands of one window land: its corner in layout pixels, the scale, the clip
/// stack in device pixels and the shape while one is open.
struct Pen {
    origin: (i32, i32),
    scale: i32,
    clips: Vec<Rect>,
    mask: Option<Mask>,
}

impl Pen {
    /// A rectangle of the window in device pixels.
    fn device(&self, r: Rect) -> Rect {
        let s = self.scale;
        Rect::new(
            (self.origin.0 + r.left) * s,
            (self.origin.1 + r.top) * s,
            r.width * s,
            r.height * s,
        )
    }

    fn clip(&self) -> Rect {
        *self.clips.last().expect("the window's own clip")
    }

    /// Blends `argb` over the canvas at `x`, `y` with `alpha` 0..255 on top of its own alpha,
    /// when the pixel lies in the clip and in the window's shape.
    /// Blends one pixel of a letter, `cover` of 255 dark, in `color`. A letter drawn with a
    /// gamma (smoothed classic text, as Windows blends it) mixes with that gamma over an
    /// opaque pixel; any other is laid on like a picture's pixel.
    #[allow(clippy::too_many_arguments)]
    fn blend_letter(
        &self,
        canvas: &mut Canvas,
        x: i32,
        y: i32,
        color: u32,
        cover: u8,
        gamma: f32,
        alpha: i32,
    ) {
        let i = (y * canvas.w + x) as usize;
        let opaque = canvas.pixels.get(i).is_some_and(|p| (p >> 24) == 0xff);
        if gamma == 1.0 || !opaque {
            let argb = (u32::from(cover) << 24) | (color & 0x00ff_ffff);
            self.blend(canvas, x, y, argb, alpha);
            return;
        }
        if !self.clip().contains(x, y) || self.mask.as_ref().is_some_and(|m| !m.get(x, y)) {
            return;
        }
        let a = (u32::from(cover) * alpha.clamp(0, 255) as u32 / 255) as u8;
        let under = canvas.pixels[i];
        let mixed = walkit::truetype::blend(color & 0x00ff_ffff, under & 0x00ff_ffff, a, gamma);
        canvas.pixels[i] = 0xff00_0000 | mixed;
    }

    fn blend(&self, canvas: &mut Canvas, x: i32, y: i32, argb: u32, alpha: i32) {
        if !self.clip().contains(x, y) || self.mask.as_ref().is_some_and(|m| !m.get(x, y)) {
            return;
        }
        let a = ((argb >> 24) as i32 * alpha / 255).clamp(0, 255) as u32;
        if a == 0 {
            return;
        }
        let i = (y * canvas.w + x) as usize;
        let under = canvas.pixels[i];
        let under_a = under >> 24;
        // source over destination, the destination's own alpha counted in
        let keep = under_a * (255 - a) / 255;
        let out_a = a + keep;
        let mix = |shift: u32| {
            let s = (argb >> shift) & 0xff;
            let d = (under >> shift) & 0xff;
            ((s * a + d * keep) / out_a) << shift
        };
        canvas.pixels[i] = (out_a << 24) | mix(16) | mix(8) | mix(0);
    }
}

/// Draws every window of a frame, bottom to top, each at its place in the stack.
pub fn draw(
    canvas: &mut Canvas,
    assets: &mut Assets,
    windows: &[(Rect, Vec<DrawCmd>)],
    scale: i32,
) {
    let screen = Rect::new(0, 0, canvas.w, canvas.h);
    for (rect, commands) in windows {
        let mut pen = Pen {
            origin: (rect.left, rect.top),
            scale,
            clips: Vec::new(),
            mask: None,
        };
        // the commands are in the window's own pixels: nothing draws outside its box
        let own = pen.device(Rect::new(0, 0, rect.width, rect.height));
        pen.clips.push(own.cut_to(&screen));
        for command in commands {
            draw_command(canvas, assets, &mut pen, command);
        }
    }
}

fn draw_command(canvas: &mut Canvas, assets: &mut Assets, pen: &mut Pen, command: &DrawCmd) {
    match command {
        DrawCmd::Image {
            bitmap,
            dst,
            alpha,
            gamma,
            ..
        } => {
            let Some(pic) = assets.picture(&bitmap.file) else {
                return;
            };
            // the part of the picture, or the rest of it from its corner
            let src = Rect::new(
                bitmap.x,
                bitmap.y,
                bitmap.w.unwrap_or(pic.w - bitmap.x),
                bitmap.h.unwrap_or(pic.h - bitmap.y),
            );
            let tint = gamma.filter(|g| !g.is_identity());
            stretch(canvas, pen, *dst, src, *alpha, |x, y| {
                pic.at(x, y).map(|p| tinted(p, tint))
            });
        }
        DrawCmd::Fill { rect, color, alpha } => {
            let d = pen.device(*rect).cut_to(&pen.clip());
            for y in d.top..d.bottom() {
                for x in d.left..d.right() {
                    pen.blend(canvas, x, y, 0xff00_0000 | color.0, *alpha);
                }
            }
        }
        DrawCmd::Pixels {
            rect,
            w,
            h,
            argb,
            alpha,
            ..
        } => {
            let src = Rect::new(0, 0, *w, *h);
            stretch(canvas, pen, *rect, src, *alpha, |x, y| {
                argb.get((y * w + x) as usize).copied()
            });
        }
        DrawCmd::Text(text) => draw_text(canvas, assets, pen, text),
        DrawCmd::Clip(rect) => {
            let inner = pen.device(*rect).cut_to(&pen.clip());
            pen.clips.push(inner);
        }
        DrawCmd::Unclip => {
            // the window's own clip stays
            if pen.clips.len() > 1 {
                pen.clips.pop();
            }
        }
        DrawCmd::WindowStart(region) => {
            let area = pen.clips[0];
            pen.mask = Some(rasterize(region, area, assets, pen));
        }
        DrawCmd::WindowEnd => pen.mask = None,
        // the engine spells bitmap text out as clips and images before the host sees it
        DrawCmd::BitmapText(_) => {}
    }
}

/// `argb` in the color theme's tint, if any.
fn tinted(argb: u32, gamma: Option<Gamma>) -> u32 {
    gamma.map_or(argb, |g| g.apply(argb))
}

/// Draws the `src` part of a picture over `dst` (layout pixels) with nearest pixels; `sample`
/// reads one pixel of the picture.
fn stretch(
    canvas: &mut Canvas,
    pen: &Pen,
    dst: Rect,
    src: Rect,
    alpha: i32,
    sample: impl Fn(i32, i32) -> Option<u32>,
) {
    let d = pen.device(dst);
    if d.width <= 0 || d.height <= 0 || src.width <= 0 || src.height <= 0 {
        return;
    }
    let visible = d.cut_to(&pen.clip());
    for y in visible.top..visible.bottom() {
        let sy = src.top + (y - d.top) * src.height / d.height;
        for x in visible.left..visible.right() {
            let sx = src.left + (x - d.left) * src.width / d.width;
            if let Some(p) = sample(sx, sy) {
                pen.blend(canvas, x, y, p, alpha);
            }
        }
    }
}

/// Draws a run of text where the engine placed it: from `x` along the `baseline`, clipped to
/// its box. The shadow goes first, under the letters.
fn draw_text(canvas: &mut Canvas, assets: &mut Assets, pen: &mut Pen, text: &TextCmd) {
    // smoothed text as Winamp draws a modern skin's, the rest as Windows draws it
    let style = if text.smooth {
        Style::Modern
    } else {
        Style::Classic
    };
    let Some(face) = assets.face(text.font.as_ref(), text.bold, style) else {
        return;
    };
    let s = pen.scale;
    let px = em(style, text.size) * s as f32;
    // Windows places text on whole pixels
    let left = ((pen.origin.0 as f32 + text.x) * s as f32).round() as i32;
    let baseline = ((pen.origin.1 as f32 + text.baseline) * s as f32).round() as i32;
    pen.clips.push(pen.device(text.rect).cut_to(&pen.clip()));
    let mut runs = Vec::new();
    if let Some(shadow) = text.shadow {
        runs.push((
            left + text.shadow_dx * s,
            baseline + text.shadow_dy * s,
            shadow.0,
        ));
    }
    runs.push((left, baseline, text.color.0));
    // a font without a bold cut is drawn twice, a pixel apart
    let strokes: &[i32] = if face.fake_bold { &[0, s] } else { &[0] };
    let letters = face
        .font
        .run(&text.text, style, px, if face.fake_bold { s } else { 0 });
    for (x0, y0, color) in runs {
        for &dx in strokes {
            for (at, glyph) in &letters {
                let (gx, gy) = (x0 + dx + at + glyph.left, y0 - glyph.top);
                for row in 0..glyph.h {
                    for col in 0..glyph.w {
                        let cover = glyph.coverage[row * glyph.w + col];
                        if cover > 0 {
                            let (x, y) = (gx + col as i32, gy + row as i32);
                            pen.blend_letter(canvas, x, y, color, cover, glyph.gamma, text.alpha);
                        }
                    }
                }
            }
        }
    }
    pen.clips.pop();
}

/// The window's shape as a mask over `area` (device pixels): a picture's opaque pixels, a
/// box, or a base kept to some regions, with others added and others cut away.
fn rasterize(region: &Region, area: Rect, assets: &mut Assets, pen: &Pen) -> Mask {
    let mut mask = Mask::empty(area);
    match region {
        Region::Box(r) => {
            let d = pen.device(*r).cut_to(&area);
            for y in d.top..d.bottom() {
                for x in d.left..d.right() {
                    mask.set(x, y);
                }
            }
        }
        Region::Pic(img) => {
            let Some(pic) = assets.picture(&img.bitmap.file) else {
                return mask;
            };
            let b = &img.bitmap;
            let src = Rect::new(
                b.x,
                b.y,
                b.w.unwrap_or(pic.w - b.x),
                b.h.unwrap_or(pic.h - b.y),
            );
            let d = pen.device(img.dst);
            if d.width <= 0 || d.height <= 0 || src.width <= 0 || src.height <= 0 {
                return mask;
            }
            let visible = d.cut_to(&area);
            for y in visible.top..visible.bottom() {
                let sy = src.top + (y - d.top) * src.height / d.height;
                for x in visible.left..visible.right() {
                    let sx = src.left + (x - d.left) * src.width / d.width;
                    if pic.at(sx, sy).is_some_and(DrawCmd::is_cut_pixel) {
                        mask.set(x, y);
                    }
                }
            }
        }
        Region::Op {
            base,
            add,
            keep,
            cut,
        } => {
            if let Some(base) = base {
                mask = rasterize(base, area, assets, pen);
            }
            if !keep.is_empty() {
                let mut kept = Mask::empty(area);
                for k in keep {
                    kept.merge(&rasterize(k, area, assets, pen), |a, b| a || b);
                }
                mask.merge(&kept, |a, b| a && b);
            }
            for a in add {
                mask.merge(&rasterize(a, area, assets, pen), |a, b| a || b);
            }
            for c in cut {
                mask.merge(&rasterize(c, area, assets, pen), |a, b| a && !b);
            }
        }
    }
    mask
}
