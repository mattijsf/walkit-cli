// SPDX-License-Identifier: GPL-3.0-or-later

//! A skin's files: the archive unzipped in memory, laid over a base and the stand-in fonts.

use std::io::{Cursor, Read};
use std::rc::Rc;

use walkit::files::{self, LayeredFiles, MemoryFiles, SkinFiles};

/// The largest file read out of an archive, and the most read in all.
const MAX_ENTRY: u64 = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;

/// The free fonts that stand in for Arial and Tahoma, with the paths the engine reads them at.
const FONTS: [(&str, &[u8]); 4] = [
    (
        "fonts/LiberationSans.ttf",
        include_bytes!("../fonts/LiberationSans.ttf"),
    ),
    (
        "fonts/LiberationSans-Bold.ttf",
        include_bytes!("../fonts/LiberationSans-Bold.ttf"),
    ),
    (
        "fonts/DejaVuSansCondensed.ttf",
        include_bytes!("../fonts/DejaVuSansCondensed.ttf"),
    ),
    (
        "fonts/DejaVuSansCondensed-Bold.ttf",
        include_bytes!("../fonts/DejaVuSansCondensed-Bold.ttf"),
    ),
];

/// The bitmaps Winamp draws a classic skin's three windows from. `nums_ex.bmp` and
/// `balance.bmp` are left out: `numbers.bmp` and `volume.bmp` stand in for them.
pub(crate) const CLASSIC_SHEETS: [&str; 12] = [
    "main.bmp",
    "cbuttons.bmp",
    "titlebar.bmp",
    "shufrep.bmp",
    "text.bmp",
    "volume.bmp",
    "monoster.bmp",
    "playpaus.bmp",
    "posbar.bmp",
    "numbers.bmp",
    "eqmain.bmp",
    "pledit.bmp",
];

/// The files of a zip archive in memory. A skin zipped inside a folder has that folder
/// stripped.
pub fn unzip(bytes: &[u8]) -> Result<MemoryFiles, String> {
    let mut zip =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not a zip archive: {e}"))?;
    let mut all = MemoryFiles::new();
    let mut names = Vec::new();
    let mut total = 0;
    for i in 0..zip.len() {
        // an entry that does not open or unpack is skipped, as Winamp skips a broken file
        let Ok(mut entry) = zip.by_index(i) else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        // the sizes an archive claims can lie: count what comes out
        let mut bytes = Vec::new();
        let Ok(read) = entry.by_ref().take(MAX_ENTRY + 1).read_to_end(&mut bytes) else {
            continue;
        };
        let read = read as u64;
        if read > MAX_ENTRY {
            continue;
        }
        total += read;
        if total > MAX_TOTAL {
            return Err("the archive is too large".into());
        }
        // Windows zip tools write backslashes
        let name = entry.name().replace('\\', "/");
        all.insert(&name, bytes);
        names.push(name);
    }
    let folder = files::skin_folder(names.iter().map(String::as_str)).unwrap_or_default();
    Ok(all.in_folder(&folder))
}

/// The Windows fonts skins name without a file, by file name, and the stand-in each replaces.
const WINDOWS_FONTS: [(&str, &str); 4] = [
    ("arial.ttf", "fonts/LiberationSans.ttf"),
    ("arialbd.ttf", "fonts/LiberationSans-Bold.ttf"),
    ("tahoma.ttf", "fonts/DejaVuSansCondensed.ttf"),
    ("tahomabd.ttf", "fonts/DejaVuSansCondensed-Bold.ttf"),
];

/// Where a font file of the caller's goes among the skin's files, by its name: in the place
/// of the stand-in for that Windows font. `None` for a file that replaces none.
pub fn font_path(name: &str) -> Option<&'static str> {
    let name = name.to_lowercase();
    WINDOWS_FONTS
        .iter()
        .find(|(windows, stand_in)| {
            name == *windows || stand_in.to_lowercase().ends_with(&format!("/{name}"))
        })
        .map(|(_, stand_in)| *stand_in)
}

/// The stand-in fonts as skin files.
fn fonts() -> MemoryFiles {
    let mut fonts = MemoryFiles::new();
    for (path, bytes) in FONTS {
        fonts.insert(path, bytes.to_vec());
    }
    fonts
}

/// A skin's files with the stand-in fonts where the engine looks for them, at `fonts/`. A
/// file of the skin's own by the same path comes first.
struct WithFonts {
    skin: MemoryFiles,
    fonts: MemoryFiles,
}

impl SkinFiles for WithFonts {
    fn read(&self, path: &str) -> Option<Rc<[u8]>> {
        self.skin.read(path).or_else(|| self.fonts.read(path))
    }

    fn names(&self) -> Vec<String> {
        let mut names = self.skin.names();
        names.extend(self.fonts.names());
        names
    }
}

/// What kind of skin an archive holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `main.bmp` and no `skin.xml`: a `.wsz`.
    Classic,
    /// `skin.xml`: a `.wal`.
    Modern,
}

/// A skin ready to load: its files, and what lies under them.
pub struct Skin {
    pub kind: Kind,
    archive: MemoryFiles,
    under: Option<MemoryFiles>,
    fonts: MemoryFiles,
}

impl Skin {
    /// Unzips `bytes`. `base` (a classic skin, unzipped) goes under a classic skin, `wasabi`
    /// (Winamp's wasabi folder, unzipped) under a modern one.
    pub fn open(
        bytes: &[u8],
        base: Option<&MemoryFiles>,
        wasabi: Option<&MemoryFiles>,
    ) -> Result<Skin, String> {
        let archive = unzip(bytes)?;
        let kind = if walkit::classic::is_classic(&archive) {
            Kind::Classic
        } else if archive.exists("skin.xml") {
            Kind::Modern
        } else {
            return Err("not a skin: the archive has no main.bmp and no skin.xml".into());
        };
        let under = match kind {
            Kind::Classic => base,
            Kind::Modern => wasabi,
        };
        Ok(Skin {
            kind,
            archive,
            under: under.cloned(),
            fonts: fonts(),
        })
    }

    /// Puts the caller's own fonts in the place of the stand-ins: `(file name, bytes)` pairs,
    /// named as Windows names them (`arial.ttf`, `arialbd.ttf`, `tahoma.ttf`,
    /// `tahomabd.ttf`) or as the stand-ins are. Other files are left out.
    pub fn with_fonts(mut self, fonts: &[(String, Vec<u8>)]) -> Skin {
        for (name, bytes) in fonts {
            if let Some(path) = font_path(name) {
                self.fonts.insert(path, bytes.clone());
            }
        }
        self
    }

    /// The skin's files with the fonts among them, over the base. The engine and the host
    /// each take one; the bytes are shared.
    pub fn files(&self) -> Box<dyn SkinFiles> {
        let skin = Box::new(WithFonts {
            skin: self.archive.clone(),
            fonts: self.fonts.clone(),
        });
        let base = self.under.clone().unwrap_or_default();
        Box::new(LayeredFiles::new(skin, Box::new(base)))
    }

    /// The bitmaps a classic skin leaves out with no base skin under it to take them from:
    /// what they draw stays empty.
    pub fn missing_sheets(&self) -> Vec<&'static str> {
        if self.kind != Kind::Classic || self.under.is_some() {
            return Vec::new();
        }
        CLASSIC_SHEETS
            .into_iter()
            .filter(|sheet| !self.archive.exists(sheet))
            .collect()
    }

    /// The parts of Winamp's wasabi folder a modern skin uses, such as
    /// `wasabi:standardframe:status`, with no `--wasabi` under it to take them from: what
    /// they draw is missing, often the whole window. A part the skin defines itself, with a
    /// `xuitag` of a copy of the folder's files, is not missing. In lower case, sorted, each
    /// once.
    pub fn missing_wasabi_parts(&self) -> Vec<String> {
        if self.kind != Kind::Modern || self.under.is_some() {
            return Vec::new();
        }
        let name_at = |text: &str, at: usize| -> String {
            text[at..]
                .chars()
                .take_while(|c| !c.is_whitespace() && !matches!(c, '/' | '>' | '"' | '\''))
                .collect()
        };
        let mut used = Vec::new();
        let mut defined = Vec::new();
        for text in self.xml_files() {
            let lower = text.to_ascii_lowercase();
            for (at, _) in lower.match_indices("<wasabi:") {
                used.push(name_at(&lower, at + 1));
            }
            for quote in ["xuitag=\"", "xuitag='"] {
                for (at, _) in lower.match_indices(quote) {
                    defined.push(name_at(&lower, at + quote.len()));
                }
            }
        }
        used.retain(|part| !defined.contains(part));
        used.sort();
        used.dedup();
        used
    }

    /// The Winamp plugins a modern skin takes files from, by the folder its includes reach
    /// into (`.../Plugins/classicPro/engine/load.xml` names `classicPro`). Such a skin is
    /// made for the plugin: its windows are in the plugin's files, which no host has.
    pub fn plugins(&self) -> Vec<String> {
        if self.kind != Kind::Modern {
            return Vec::new();
        }
        let mut plugins = Vec::new();
        for text in self.xml_files() {
            let lower = text.to_ascii_lowercase();
            for (at, _) in lower.match_indices("<include") {
                let tag = &text[at..];
                let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
                let path = tag.replace('\\', "/");
                let Some(start) = path.to_ascii_lowercase().find("/plugins/") else {
                    continue;
                };
                let rest = &path[start + "/plugins/".len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| *c != '/' && *c != '"' && *c != '\'')
                    .collect();
                if !name.is_empty() && !plugins.contains(&name) {
                    plugins.push(name);
                }
            }
        }
        plugins.sort();
        plugins
    }

    /// The paths of the files in the skin's own archive, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut names = self.archive.names();
        names.sort();
        names
    }

    /// A file of the skin's own archive, its path matched as the engine matches it.
    pub fn read(&self, path: &str) -> Option<Rc<[u8]>> {
        self.archive.read(path)
    }

    /// The text of the skin's own XML files.
    pub(crate) fn xml_files(&self) -> Vec<String> {
        self.archive
            .names()
            .into_iter()
            .filter(|name| name.to_lowercase().ends_with(".xml"))
            .filter_map(|name| self.archive.read(&name))
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .collect()
    }
}
