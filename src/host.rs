// SPDX-License-Identifier: GPL-3.0-or-later

//! The host: the skin's pictures, decoded with the `image` crate, and its text, measured and
//! drawn by the engine's [`walkit::truetype`] as Winamp draws it: a classic skin's text as
//! Windows draws it, a modern skin's smoothed text as Winamp draws that. The engine reads them
//! through [`Host`]; the renderer reads the same
//! [`Assets`], so each picture and font is decoded once. The free fonts that stand in for
//! Arial and Tahoma lie under the skin's files as `fonts/LiberationSans.ttf` and the like.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use walkit::draw::FontDef;
use walkit::files::SkinFiles;
use walkit::gdi::FontFace;
use walkit::host::SkinHost;
use walkit::truetype::{em, Font, Style};

/// The font for text that names none, and the one a missing font falls back to: the stand-in
/// for Arial, read from under the skin's files.
const DEFAULT_FONT: &str = "fonts/LiberationSans.ttf";

/// Its bold cut.
const DEFAULT_BOLD_FONT: &str = "fonts/LiberationSans-Bold.ttf";

/// A decoded picture: its size and its pixels as ARGB, rows top first.
pub struct Picture {
    pub w: i32,
    pub h: i32,
    pub argb: Vec<u32>,
}

impl Picture {
    /// The pixel at `x`, `y`, or `None` outside the picture.
    pub fn at(&self, x: i32, y: i32) -> Option<u32> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        Some(self.argb[(y * self.w + x) as usize])
    }
}

/// The font a run of text draws in, and whether the renderer thickens it for bold.
pub struct TextFace {
    pub font: Rc<Font>,
    pub fake_bold: bool,
}

/// The skin's files, and the pictures and fonts read from them so far.
pub struct Assets {
    files: Box<dyn SkinFiles>,
    pictures: HashMap<String, Option<Rc<Picture>>>,
    fonts: HashMap<String, Option<Rc<Font>>>,
}

impl Assets {
    pub fn new(files: Box<dyn SkinFiles>) -> Self {
        Assets {
            files,
            pictures: HashMap::new(),
            fonts: HashMap::new(),
        }
    }

    /// The picture at `file`, decoded the first time it is asked for. Paths are looked up
    /// without regard to case.
    pub fn picture(&mut self, file: &str) -> Option<Rc<Picture>> {
        let key = file.to_lowercase();
        if let Some(p) = self.pictures.get(&key) {
            return p.clone();
        }
        let decoded = self.files.read(file).and_then(|b| decode(&b)).map(Rc::new);
        self.pictures.insert(key, decoded.clone());
        decoded
    }

    /// The font `font` draws in, `bold` or not, in `style`, as the engine names its face;
    /// `None` only when not even the default font loads.
    pub fn face(&mut self, font: Option<&FontDef>, bold: bool, style: Style) -> Option<TextFace> {
        // smoothed bold is the regular letters drawn twice, even where a bold cut exists
        if style == Style::Modern && bold {
            let face = self.face(font, false, style)?;
            return Some(TextFace {
                font: face.font,
                fake_bold: true,
            });
        }
        let face = FontFace::of(font, bold);
        let (found, fake_bold) = match &face {
            // a Windows font the skin names without a file: the free font standing in for it
            FontFace::StandIn { .. } => {
                let file = face.file().unwrap_or_default();
                (self.skin_font(&file), false)
            }
            FontFace::SkinFile { font, fake_bold } => {
                let found = self.skin_font(&font.file);
                (found, *fake_bold)
            }
            FontFace::Default { fake_bold } => (None, *fake_bold),
        };
        if let Some(font) = found {
            return Some(TextFace { font, fake_bold });
        }
        // the default font, in its own bold cut
        let file = if fake_bold {
            DEFAULT_BOLD_FONT
        } else {
            DEFAULT_FONT
        };
        let font = self.skin_font(file)?;
        Some(TextFace {
            font,
            fake_bold: false,
        })
    }

    /// The bytes of `file` among the skin's files.
    pub fn read(&self, file: &str) -> Option<Rc<[u8]>> {
        self.files.read(file)
    }

    /// A TrueType file among the skin's files.
    fn skin_font(&mut self, file: &str) -> Option<Rc<Font>> {
        if file.is_empty() {
            return None;
        }
        let key = file.to_lowercase();
        if let Some(f) = self.fonts.get(&key) {
            return f.clone();
        }
        let font = self.files.read(file).and_then(Font::parse);
        self.fonts.insert(key, font.clone());
        font
    }
}

/// Decodes a PNG, BMP, GIF or JPEG into ARGB.
fn decode(bytes: &[u8]) -> Option<Picture> {
    let rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = rgba.dimensions();
    let argb = rgba
        .pixels()
        .map(|p| {
            let [r, g, b, a] = p.0;
            (u32::from(a) << 24) | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
        })
        .collect();
    Some(Picture {
        w: w as i32,
        h: h as i32,
        argb,
    })
}

/// What the engine asks of the tool. It shares [`Assets`] with the renderer and keeps
/// what the skin logs, for `--verbose`.
pub struct Host {
    pub assets: Rc<RefCell<Assets>>,
    pub log: Rc<RefCell<Vec<String>>>,
}

impl Host {
    /// How wide `text` draws in layout pixels in `style`: Winamp draws a font at three
    /// quarters of the skin's size ([`em`]).
    fn measure(
        &mut self,
        text: &str,
        font: Option<&FontDef>,
        size: i32,
        bold: bool,
        style: Style,
    ) -> f32 {
        let face = self.assets.borrow_mut().face(font, bold, style);
        match face {
            Some(face) => {
                // a font without a bold cut is drawn twice, a pixel apart
                let bold = i32::from(face.fake_bold);
                face.font.width(text, style, em(style, size), bold) as f32
            }
            None => text.chars().count() as f32 * size as f32 * 0.6,
        }
    }

    /// The font's line in `style`.
    fn line(
        &mut self,
        font: Option<&FontDef>,
        size: i32,
        bold: bool,
        style: Style,
    ) -> Option<(f32, f32)> {
        let face = self.assets.borrow_mut().face(font, bold, style)?;
        Some(face.font.line(style, em(style, size)))
    }
}

impl SkinHost for Host {
    fn image_size(&mut self, file: &str) -> Option<(i32, i32)> {
        let p = self.assets.borrow_mut().picture(file)?;
        Some((p.w, p.h))
    }

    fn pixel(&mut self, file: &str, x: i32, y: i32) -> Option<u32> {
        self.assets.borrow_mut().picture(file)?.at(x, y)
    }

    fn pixels(&mut self, file: &str, x: i32, y: i32, w: i32, h: i32) -> Option<Vec<u32>> {
        let p = self.assets.borrow_mut().picture(file)?;
        let mut out = Vec::with_capacity((w.max(0) * h.max(0)) as usize);
        for row in y..y + h {
            for col in x..x + w {
                out.push(p.at(col, row)?);
            }
        }
        Some(out)
    }

    fn text_width(&mut self, text: &str, font: Option<&FontDef>, size: i32, bold: bool) -> i32 {
        self.measure(text, font, size, bold, Style::Classic).ceil() as i32
    }

    fn text_advance(&mut self, text: &str, font: Option<&FontDef>, size: i32, bold: bool) -> f32 {
        self.measure(text, font, size, bold, Style::Classic)
    }

    fn smooth_text_advance(
        &mut self,
        text: &str,
        font: Option<&FontDef>,
        size: i32,
        bold: bool,
    ) -> Option<f32> {
        Some(self.measure(text, font, size, bold, Style::Modern))
    }

    fn line_metrics(
        &mut self,
        font: Option<&FontDef>,
        size: i32,
        bold: bool,
    ) -> Option<(f32, f32)> {
        self.line(font, size, bold, Style::Classic)
    }

    fn smooth_line_metrics(
        &mut self,
        font: Option<&FontDef>,
        size: i32,
        bold: bool,
    ) -> Option<(f32, f32)> {
        self.line(font, size, bold, Style::Modern)
    }

    fn font_file(&mut self, file: &str) -> Option<Rc<[u8]>> {
        self.assets.borrow().read(file)
    }

    fn log(&mut self, message: &str) {
        self.log.borrow_mut().push(message.to_string());
    }

    fn invalidate(&mut self) {}
}
