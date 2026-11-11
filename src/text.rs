// SPDX-License-Identifier: GPL-3.0-or-later

//! TrueType text as Windows draws it for Winamp: the font's own hinting fits each letter to
//! the pixel grid, letters advance by whole pixels without kerning, and the font's `gasp`
//! table says at which sizes the letters are smoothed. Arial between 9 and 17 pixels, the
//! playlist's font, is not smoothed at all.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingOptions, OutlinePen, Target};
use skrifa::raw::tables::gasp::GaspRangeBehavior;
use skrifa::raw::tables::kern::SubtableKind;
use skrifa::raw::types::Tag;
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// How far outside the outline a pixel's center may lie and still count as on it. Hinted
/// points sit on a grid of 64ths of a pixel, so a center on the outline is met exactly.
const EDGE: f32 = 1e-5;

/// Smoothed letters are sampled this many times per pixel, across and down.
const SAMPLES: i32 = 4;

/// A drawn letter: where its bitmap sits from the pen on the baseline, its coverage with one
/// byte per pixel, and how far the pen moves on.
pub struct Glyph {
    /// Pixels from the pen to the bitmap's left edge.
    pub left: i32,
    /// Pixels from the baseline up to the bitmap's top edge.
    pub top: i32,
    pub w: usize,
    pub h: usize,
    pub coverage: Vec<u8>,
    pub advance: i32,
}

/// What a font does at one pixel size.
struct Sized {
    /// The font's hinting at this size, or `None` where it asks for none or has none.
    hinter: Option<HintingInstance>,
    /// Whether letters are smoothed at this size.
    smooth: bool,
    glyphs: HashMap<char, Rc<Glyph>>,
}

/// A TrueType font and the letters drawn from it so far.
pub struct Font {
    data: Rc<[u8]>,
    sizes: RefCell<HashMap<u32, Sized>>,
    kerns: RefCell<HashMap<(char, char, u32), i32>>,
}

impl Font {
    /// The font in `data`, or `None` when it is not a TrueType or OpenType file.
    pub fn parse(data: Rc<[u8]>) -> Option<Rc<Font>> {
        FontRef::new(&data).ok()?;
        Some(Rc::new(Font {
            data,
            sizes: RefCell::default(),
            kerns: RefCell::default(),
        }))
    }

    /// The letter `c` at `px` pixels per em, drawn once.
    pub fn glyph(&self, c: char, px: u32) -> Rc<Glyph> {
        let mut sizes = self.sizes.borrow_mut();
        let font = self.font();
        let sized = sizes.entry(px).or_insert_with(|| size(&font, px));
        if let Some(glyph) = sized.glyphs.get(&c) {
            return glyph.clone();
        }
        let glyph = Rc::new(draw(&font, sized, c, px));
        sized.glyphs.insert(c, glyph.clone());
        glyph
    }

    /// How far `text` advances at `px` pixels per em, with kerning.
    pub fn advance(&self, text: &str, px: u32) -> i32 {
        let mut width = 0;
        let mut last = None;
        for c in text.chars() {
            if let Some(prev) = last {
                width += self.kerning(prev, c, px);
            }
            width += self.glyph(c, px).advance;
            last = Some(c);
        }
        width
    }

    /// How many pixels `right` moves toward `left` (negative) or away from it at `px` pixels
    /// per em, from the font's `kern` table, in whole pixels.
    pub fn kerning(&self, left: char, right: char, px: u32) -> i32 {
        *self
            .kerns
            .borrow_mut()
            .entry((left, right, px))
            .or_insert_with(|| {
                let font = self.font();
                let map = font.charmap();
                let (Some(l), Some(r)) = (map.map(left), map.map(right)) else {
                    return 0;
                };
                let Ok(kern) = font.kern() else {
                    return 0;
                };
                let units: i32 = kern
                    .subtables()
                    .flatten()
                    .filter(|table| table.is_horizontal() && !table.is_cross_stream())
                    .filter_map(|table| match table.kind().ok()? {
                        SubtableKind::Format0(pairs) => pairs.kerning(l, r),
                        _ => None,
                    })
                    .sum();
                let em = font.head().map_or(1000, |head| head.units_per_em());
                (units as f32 * px as f32 / f32::from(em)).round() as i32
            })
    }

    /// The font's ascent and descent at `px` pixels per em, both positive.
    pub fn line(&self, px: u32) -> (f32, f32) {
        let m = self
            .font()
            .metrics(Size::new(px as f32), LocationRef::default());
        (m.ascent, -m.descent)
    }

    fn font(&self) -> FontRef<'_> {
        // checked in `parse`
        FontRef::new(&self.data).expect("a font that parsed before")
    }
}

/// Reads from the font's `gasp` table what Windows does at `px`: fit to the grid, smooth,
/// both or neither. A font without the table gets both.
fn size(font: &FontRef, px: u32) -> Sized {
    let (mut fit, mut smooth) = (true, true);
    if let Ok(gasp) = font.gasp() {
        let range = gasp
            .gasp_ranges()
            .iter()
            .find(|r| u32::from(r.range_max_ppem()) >= px);
        if let Some(range) = range {
            let behavior = range.range_gasp_behavior();
            fit = behavior.contains(GaspRangeBehavior::GASP_GRIDFIT);
            smooth = behavior.contains(GaspRangeBehavior::GASP_DOGRAY);
        }
    }
    let hinter = fit
        .then(|| {
            let options = HintingOptions {
                engine: Engine::Interpreter,
                target: Target::Mono,
            };
            HintingInstance::new(
                &font.outline_glyphs(),
                Size::new(px as f32),
                LocationRef::default(),
                options,
            )
            .ok()
        })
        .flatten();
    Sized {
        hinter,
        smooth,
        glyphs: HashMap::new(),
    }
}

/// Draws `c`: its outline, hinted where the size asks for it, filled into a bitmap.
fn draw(font: &FontRef, sized: &Sized, c: char, px: u32) -> Glyph {
    let size = Size::new(px as f32);
    let id = font.charmap().map(c).unwrap_or(GlyphId::NOTDEF);
    let linear = font
        .glyph_metrics(size, LocationRef::default())
        .advance_width(id)
        .unwrap_or(0.0);
    let mut advance = linear.round() as i32;
    // Windows takes a letter's width from the font's tables before it asks the hinting
    let device = device_width(font, id, px);
    let mut path = Path::new(px);
    if let Some(outline) = font.outline_glyphs().get(id) {
        let hinted = sized.hinter.as_ref().and_then(|hinter| {
            let mut hinted = Path::new(px);
            let metrics = outline
                .draw(DrawSettings::hinted(hinter, false), &mut hinted)
                .ok()?;
            Some((hinted, metrics.advance_width))
        });
        match hinted {
            Some((hinted, width)) => {
                path = hinted;
                if let Some(width) = width {
                    advance = width.round() as i32;
                }
            }
            None => {
                let settings = DrawSettings::unhinted(size, LocationRef::default());
                if outline.draw(settings, &mut path).is_err() {
                    path = Path::new(px);
                }
            }
        }
    }
    let mut glyph = path.fill(sized.smooth);
    glyph.advance = device.unwrap_or(advance);
    glyph
}

/// The width the font's tables give glyph `id` at `px` pixels per em, where they give one:
/// the `hdmx` table's device width for that size, or, from the size the `LTSH` table says
/// the glyph scales evenly, its scaled width rounded. `None` leaves the width to the
/// hinting.
fn device_width(font: &FontRef, id: GlyphId, px: u32) -> Option<i32> {
    let index = id.to_u32() as usize;
    if let Some(record) = u8::try_from(px)
        .ok()
        .and_then(|size| font.hdmx().ok()?.record_for_size(size))
    {
        if let Some(width) = record.widths().get(index) {
            return Some(i32::from(*width));
        }
    }
    // LTSH: a version, the glyph count, then one size per glyph
    let ltsh = font.table_data(Tag::new(b"LTSH"))?;
    let from = u32::from(*ltsh.as_bytes().get(4 + index)?);
    if from == 0 || px < from {
        return None;
    }
    let linear = font
        .glyph_metrics(Size::new(px as f32), LocationRef::default())
        .advance_width(id)?;
    Some(linear.round() as i32)
}

/// An outline as closed runs of straight edges, in pixels with y up.
struct Path {
    contours: Vec<Vec<(f32, f32)>>,
    /// How many straight edges stand in for a curve.
    steps: u32,
}

impl Path {
    fn new(px: u32) -> Path {
        Path {
            contours: Vec::new(),
            steps: (px / 3).clamp(6, 32),
        }
    }

    fn last(&self) -> (f32, f32) {
        self.contours
            .last()
            .and_then(|c| c.last())
            .copied()
            .unwrap_or_default()
    }

    fn push(&mut self, point: (f32, f32)) {
        if let Some(contour) = self.contours.last_mut() {
            contour.push(point);
        }
    }

    /// The edges that cross scan lines, as `(x0, y0, x1, y1)` after `turn`, which maps a
    /// point to the sweep's own axes.
    fn edges(&self, turn: impl Fn((f32, f32)) -> (f32, f32)) -> Vec<[f32; 4]> {
        let mut edges = Vec::new();
        for contour in &self.contours {
            for (i, &from) in contour.iter().enumerate() {
                let (x0, y0) = turn(from);
                let (x1, y1) = turn(contour[(i + 1) % contour.len()]);
                if y0 != y1 {
                    edges.push([x0, y0, x1, y1]);
                }
            }
        }
        edges
    }

    /// Fills the outline. Unsmoothed, a pixel is on when its center lies inside, and a stem
    /// thinner than a pixel that passes between two centers still turns one pixel on, as
    /// Windows keeps thin strokes from dropping out. Smoothed, each pixel is sampled
    /// [`SAMPLES`] times across and down.
    fn fill(&self, smooth: bool) -> Glyph {
        let points = || self.contours.iter().flatten();
        let fold = |f: fn(f32, f32) -> f32, pick: fn(&(f32, f32)) -> f32, start: f32| {
            points().map(pick).fold(start, f)
        };
        let empty = Glyph {
            left: 0,
            top: 0,
            w: 0,
            h: 0,
            coverage: Vec::new(),
            advance: 0,
        };
        if points().next().is_none() {
            return empty;
        }
        // a pixel of margin, for a dropped-out stem on the box's edge
        let left = fold(f32::min, |p| p.0, f32::MAX).floor() as i32 - 1;
        let right = fold(f32::max, |p| p.0, f32::MIN).ceil() as i32 + 1;
        let bottom = fold(f32::min, |p| p.1, f32::MAX).floor() as i32 - 1;
        let top = fold(f32::max, |p| p.1, f32::MIN).ceil() as i32 + 1;
        let (w, h) = ((right - left) as usize, (top - bottom) as usize);
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return empty;
        }
        let n = if smooth { SAMPLES } else { 1 };
        let scale = n as f32;
        // sample space: x right from the box's left edge, y down from its top edge
        let across = self.edges(|(x, y)| ((x - left as f32) * scale, (top as f32 - y) * scale));
        let (sw, sh) = (w * n as usize, h * n as usize);
        let mut samples = vec![false; sw * sh];
        let mut drops = Vec::new();
        let rows = runs(&across, sh);
        for (row, line) in rows.iter().enumerate() {
            for &(from, to) in line {
                match centers(from, to, sw) {
                    Some(Centers::Covered(first, last)) => {
                        samples[row * sw + first..=row * sw + last].fill(true);
                    }
                    Some(Centers::Between(before)) if continues(&rows, row, from, to) => {
                        // the left one of the two
                        drops.push((before, row, before, row, before + 1, row));
                    }
                    _ => {}
                }
            }
        }
        if !smooth {
            // the same sweep along the columns finds thin flat strokes
            let down = self.edges(|(x, y)| (top as f32 - y, x - left as f32));
            let columns = runs(&down, sw);
            for (col, line) in columns.iter().enumerate() {
                for &(from, to) in line {
                    if let Some(Centers::Between(before)) = centers(from, to, sh) {
                        if continues(&columns, col, from, to) {
                            // the lower one of the two
                            drops.push((col, before + 1, col, before, col, before + 1));
                        }
                    }
                }
            }
            // a dropped-out stroke lights its pixel only when neither neighbor is lit
            for (x, y, ax, ay, bx, by) in drops {
                if !samples[ay * sw + ax] && !samples[by * sw + bx] {
                    samples[y * sw + x] = true;
                }
            }
        }
        let mut coverage = vec![0u8; w * h];
        let per_pixel = (n * n) as u32;
        for (i, cover) in coverage.iter_mut().enumerate() {
            let (px, py) = (i % w * n as usize, i / w * n as usize);
            let mut lit = 0;
            for sy in 0..n as usize {
                for sx in 0..n as usize {
                    lit += u32::from(samples[(py + sy) * sw + px + sx]);
                }
            }
            *cover = (lit * 255 / per_pixel) as u8;
        }
        Glyph {
            left,
            top,
            w,
            h,
            coverage,
            advance: 0,
        }
    }
}

/// The runs inside the outline along each of `lines` scan lines through the sample centers,
/// by the nonzero rule, as `(from, to)` along the line.
fn runs(edges: &[[f32; 4]], lines: usize) -> Vec<Vec<(f32, f32)>> {
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    (0..lines)
        .map(|line| {
            let y = line as f32 + 0.5;
            crossings.clear();
            for &[x0, y0, x1, y1] in edges {
                let (low, high, dir) = if y0 < y1 { (y0, y1, 1) } else { (y1, y0, -1) };
                if y >= low && y < high {
                    crossings.push((x0 + (y - y0) * (x1 - x0) / (y1 - y0), dir));
                }
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut found = Vec::new();
            let mut winding = 0;
            let mut start = 0.0;
            for &(x, dir) in &crossings {
                let was = winding;
                winding += dir;
                if was == 0 && winding != 0 {
                    start = x;
                } else if was != 0 && winding == 0 {
                    found.push((start, x));
                }
            }
            found
        })
        .collect()
}

/// The samples a run lights along a line of `len` samples.
enum Centers {
    /// The first and last sample whose centers the run covers.
    Covered(usize, usize),
    /// The run passes between the centers of this sample and the next.
    Between(usize),
}

fn centers(from: f32, to: f32, len: usize) -> Option<Centers> {
    // centers at i + 0.5; a center on the outline counts as inside
    let first = (from - 0.5 - EDGE).ceil().max(0.0) as usize;
    let last = (to - 0.5 + EDGE).floor();
    if last >= first as f32 {
        return (first < len).then(|| Centers::Covered(first, (last as usize).min(len - 1)));
    }
    if to <= from || first == 0 || first >= len {
        return None;
    }
    Some(Centers::Between(first - 1))
}

/// Whether a run between two centers lights a pixel: the stroke it cuts through goes on
/// across both neighboring lines, or the run is at least half a pixel long. The thin tip
/// of a stroke that ends between two lines is left dark.
fn continues(runs: &[Vec<(f32, f32)>], line: usize, from: f32, to: f32) -> bool {
    if to - from >= 0.5 {
        return true;
    }
    let near = |line: &Vec<(f32, f32)>| line.iter().any(|&(a, b)| b >= from - 1.0 && a <= to + 1.0);
    line > 0 && line + 1 < runs.len() && near(&runs[line - 1]) && near(&runs[line + 1])
}

impl OutlinePen for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.contours.push(vec![(x, y)]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push((x, y));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        for i in 1..=self.steps {
            let t = i as f32 / self.steps as f32;
            let u = 1.0 - t;
            self.push((
                u * u * x0 + 2.0 * u * t * cx + t * t * x,
                u * u * y0 + 2.0 * u * t * cy + t * t * y,
            ));
        }
    }

    fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        for i in 1..=self.steps {
            let t = i as f32 / self.steps as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.push((
                a * x0 + b * ax + c * bx + d * x,
                a * y0 + b * ay + c * by + d * y,
            ));
        }
    }

    fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Rc<Font> {
        let bytes: &[u8] = include_bytes!("../fonts/LiberationSans.ttf");
        Font::parse(Rc::from(bytes)).expect("the stand-in font parses")
    }

    #[test]
    fn small_text_is_not_smoothed() {
        let font = font();
        for c in "Winamp 0123".chars() {
            let glyph = font.glyph(c, 10);
            assert!(
                glyph
                    .coverage
                    .iter()
                    .all(|&cover| cover == 0 || cover == 255),
                "{c:?} has smoothed pixels at 10 pixels"
            );
        }
    }

    #[test]
    fn large_text_is_smoothed() {
        let glyph = font().glyph('o', 24);
        assert!(glyph.coverage.iter().any(|&cover| cover > 0 && cover < 255));
    }

    #[test]
    fn letters_have_ink_and_spaces_have_none() {
        let font = font();
        let letter = font.glyph('W', 10);
        assert!(letter.coverage.contains(&255));
        assert!(letter.advance > 0);
        let space = font.glyph(' ', 10);
        assert!(space.coverage.iter().all(|&cover| cover == 0));
        assert!(space.advance > 0);
    }

    #[test]
    fn a_line_is_as_wide_as_its_letters_and_kerning() {
        let font = font();
        let letters: i32 = "AV".chars().map(|c| font.glyph(c, 13).advance).sum();
        assert_eq!(font.advance("AV", 13), letters + font.kerning('A', 'V', 13));
        assert_eq!(font.advance("", 13), 0);
    }

    #[test]
    fn a_thin_stroke_between_two_rows_lights_one_of_them() {
        // a bar a quarter of a pixel thick, between the centers of two rows
        let mut path = Path::new(10);
        path.move_to(0.0, 0.875);
        path.line_to(4.0, 0.875);
        path.line_to(4.0, 1.125);
        path.line_to(0.0, 1.125);
        let glyph = path.fill(false);
        let rows = glyph
            .coverage
            .chunks(glyph.w)
            .filter(|row| row.contains(&255))
            .count();
        assert_eq!(rows, 1);
    }

    #[test]
    fn what_is_not_a_font_does_not_parse() {
        assert!(Font::parse(Rc::from(&b"not a font"[..])).is_none());
    }
}
