// SPDX-License-Identifier: GPL-3.0-or-later

//! What a skin is: who made it, what is in it, and what the engine makes of it.
//!
//! [`Info`] is what `walkit info --json` prints for each skin. Its JSON Schema is
//! [`schemas/info.schema.json`](https://github.com/mattijsf/walkit-cli/blob/main/schemas/info.schema.json)
//! in the repository, made by [`crate::schema::info`].

use std::fmt::Write as _;
use std::rc::Rc;

use md5::{Digest, Md5};
use schemars::JsonSchema;
use serde::Serialize;
use walkit::classic::{pledit, region, viscolor};
use walkit::draw::Rgb;
use walkit::xml::{self, XmlNode};

use crate::params::Params;
use crate::shot::survey;
use crate::skin::{Kind, Skin, CLASSIC_SHEETS};

/// The bitmaps a classic skin may add to the twelve Winamp draws its windows from.
const CLASSIC_EXTRAS: [&str; 8] = [
    "nums_ex.bmp",
    "eq_ex.bmp",
    "balance.bmp",
    "gen.bmp",
    "genex.bmp",
    "video.bmp",
    "mb.bmp",
    "avs.bmp",
];

/// Everything known about one skin.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Info {
    /// The skin's name: its file name without the ending.
    pub name: String,
    pub md5: String,
    pub bytes: usize,
    pub kind: Kind,
    /// The folder in the archive the skin is in, as the archive writes it; empty at the root.
    pub folder: String,
    /// The folders of the other skins in the same archive, which are left out.
    pub other_skins: Vec<String>,
    /// The earliest and the latest date of the archive's files, as `YYYY-MM-DD HH:MM:SS`.
    pub dates: Option<[String; 2]>,
    /// The files of the skin, by path in its folder.
    pub files: Vec<String>,
    /// The readme, picked as the Winamp Skin Museum picks it; `None` when no file qualifies.
    pub readme: Option<Readme>,
    /// The author's text files: readmes, licences, notes.
    pub texts: Vec<Text>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classic: Option<ClassicInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modern: Option<ModernInfo>,
}

/// The skin's readme.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Readme {
    /// Its path in the archive, as the archive writes it (with any folder the skin is in).
    pub file: String,
    pub text: String,
}

/// A text file of the author's.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Text {
    pub file: String,
    pub text: String,
}

/// What a classic skin holds.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ClassicInfo {
    /// Of the twelve bitmaps the windows are drawn from, those the skin has not.
    pub missing_sheets: Vec<String>,
    /// The optional bitmaps it has (`nums_ex.bmp`, `gen.bmp`, ...).
    pub extras: Vec<String>,
    /// The playlist's colours and font from `pledit.txt`; `None` without one.
    pub playlist: Option<Playlist>,
    /// The 24 vis colours from `viscolor.txt`; empty without one.
    pub vis_colors: Vec<String>,
    /// The windows `region.txt` gives a shape, such as `normal` or `equalizer`.
    pub regions: Vec<String>,
    /// The cursor files.
    pub cursors: Vec<String>,
}

/// `pledit.txt`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Playlist {
    pub normal: String,
    pub current: String,
    pub normal_bg: String,
    pub selected_bg: String,
    pub font: String,
}

/// What a modern skin holds and how it loads.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ModernInfo {
    /// `<skininfo>`.
    pub skin_info: SkinInfo,
    /// The version of the skin format, from the root element of `skin.xml`.
    pub format_version: Option<String>,
    /// The containers the skin defines.
    pub containers: Vec<Container>,
    /// The windows on screen once it has loaded, the main one first.
    pub windows: Vec<Window>,
    pub color_themes: Vec<String>,
    /// The compiled scripts.
    pub scripts: Vec<String>,
    /// The TrueType font files.
    pub fonts: Vec<String>,
    /// The tags in the windows on screen that nothing defines, so they draw nothing.
    pub missing_parts: Vec<MissingPart>,
    /// The Winamp plugins the skin takes files from.
    pub plugins: Vec<String>,
}

/// `<skininfo>`: what the author says of the skin.
#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct SkinInfo {
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub comment: Option<String>,
    pub email: Option<String>,
    pub homepage: Option<String>,
    pub screenshot: Option<String>,
    /// The screenshot WACUP shows (`<wacupscreenshot>`).
    pub wacup_screenshot: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Container {
    pub id: String,
    pub name: Option<String>,
    /// Whether it opens when the skin loads.
    pub default_visible: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Window {
    pub container: String,
    pub layout: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MissingPart {
    pub tag: String,
    pub container: String,
}

/// Inspects `skin`, opened from `bytes`, which scripts know as `name`. A modern skin is loaded
/// into the engine, staged as `params` describe.
pub fn inspect(skin: &Skin, bytes: &[u8], name: &str, params: &Params) -> Info {
    let files = skin.names();
    let archive = archive(bytes);
    let texts = files
        .iter()
        .filter(|f| is_text(f))
        .filter_map(|f| {
            let text = decode(&skin.read(f)?);
            (!text.trim().is_empty()).then(|| Text {
                file: f.clone(),
                text,
            })
        })
        .collect();
    let (classic, modern) = match skin.kind {
        Kind::Classic => (Some(classic(skin, &files)), None),
        Kind::Modern => (None, Some(modern(skin, &files, name, params))),
    };
    Info {
        name: name.to_string(),
        md5: format!("{:x}", Md5::digest(bytes)),
        bytes: bytes.len(),
        kind: skin.kind,
        folder: archive.folder,
        other_skins: archive.other_skins,
        dates: archive.dates,
        files,
        readme: archive.readme,
        texts,
        classic,
        modern,
    }
}

fn classic(skin: &Skin, files: &[String]) -> ClassicInfo {
    let has = |name: &str| skin.read(name).is_some();
    let text = |name: &str| skin.read(name).map(|b| decode(&b));
    let playlist = text("pledit.txt").map(|t| {
        let s = pledit::parse(&t);
        Playlist {
            normal: hex(s.normal),
            current: hex(s.current),
            normal_bg: hex(s.normal_bg),
            selected_bg: hex(s.selected_bg),
            font: s.font,
        }
    });
    let vis_colors = text("viscolor.txt")
        .map(|t| viscolor::parse(&t).iter().map(|c| hex(*c)).collect())
        .unwrap_or_default();
    let mut regions: Vec<String> = text("region.txt")
        .map(|t| region::parse(&t).into_keys().collect())
        .unwrap_or_default();
    regions.sort();
    ClassicInfo {
        missing_sheets: CLASSIC_SHEETS
            .iter()
            .filter(|s| !has(s))
            .map(|s| s.to_string())
            .collect(),
        extras: CLASSIC_EXTRAS
            .iter()
            .filter(|s| has(s))
            .map(|s| s.to_string())
            .collect(),
        playlist,
        vis_colors,
        regions,
        cursors: ending(files, &[".cur", ".ani"]),
    }
}

fn modern(skin: &Skin, files: &[String], name: &str, params: &Params) -> ModernInfo {
    let mut nodes = Vec::new();
    for text in skin.xml_files() {
        for root in xml::parse(&text) {
            walk(&root, &mut nodes);
        }
    }
    let format_version = skin
        .read("skin.xml")
        .map(|b| decode(&b))
        .and_then(|t| {
            xml::parse(&t)
                .into_iter()
                .find(|r| r.name == "winampabstractionlayer" || r.name == "wasabixml")
        })
        .and_then(|r| r.get("version").map(str::to_string));
    let mut skin_info = SkinInfo::default();
    if let Some(info) = nodes.iter().find(|n| n.name == "skininfo") {
        let field = |key: &str| {
            let n = info.children.iter().find(|c| c.name == key)?;
            let value = if n.text.is_empty() {
                n.get("text").unwrap_or_default()
            } else {
                n.text.as_str()
            };
            (!value.is_empty()).then(|| value.to_string())
        };
        skin_info = SkinInfo {
            name: field("name"),
            version: field("version"),
            author: field("author"),
            comment: field("comment"),
            email: field("email"),
            homepage: field("homepage"),
            screenshot: field("screenshot"),
            wacup_screenshot: field("wacupscreenshot"),
        };
    }
    let mut containers: Vec<Container> = Vec::new();
    for node in nodes.iter().filter(|n| n.name == "container") {
        let Some(id) = node.get("id").filter(|id| !id.is_empty()) else {
            continue;
        };
        if containers.iter().any(|c| c.id.eq_ignore_ascii_case(id)) {
            continue;
        }
        containers.push(Container {
            id: id.to_string(),
            name: node.get("name").map(str::to_string),
            default_visible: node.get("default_visible").is_none_or(|v| v.trim() != "0"),
        });
    }
    let survey = survey(skin, name, params);
    let windows = survey
        .windows
        .iter()
        .map(|w| Window {
            container: w.container.clone(),
            layout: w.layout.clone(),
            width: w.size.0,
            height: w.size.1,
        })
        .collect();
    let mut missing_parts: Vec<MissingPart> = Vec::new();
    for u in survey.unknown {
        if !missing_parts
            .iter()
            .any(|m| m.tag == u.tag && m.container == u.container)
        {
            missing_parts.push(MissingPart {
                tag: u.tag,
                container: u.container,
            });
        }
    }
    ModernInfo {
        skin_info,
        format_version,
        containers,
        windows,
        color_themes: survey.color_themes,
        scripts: ending(files, &[".maki"]),
        fonts: ending(files, &[".ttf", ".otf"]),
        missing_parts,
        plugins: skin.plugins(),
    }
}

/// The names the museum never takes for a readme: what Winamp and its plugins read, and
/// files download sites add. The museum's pattern anchors only its last name at the end.
const NOT_README: [&str; 13] = [
    "dialogs.txt",
    "genex.txt",
    "genexinfo.txt",
    "gen_gslyrics.txt",
    "region.txt",
    "pledit.txt",
    "viscolor.txt",
    "winampmb.txt",
    "\"gen_ex help.txt",
    "mbinner.txt",
    "winampskins.info.txt",
    "albumlist.txt",
    "covertag.txt",
];

/// Whether the museum takes the archive entry `name` for a readme: `getReadme` in the
/// webamp repository's `packages/skin-database/data/SkinModel.ts` tests the entry's full
/// name against `/(file_id\.diz)|(\.txt)$/i` and not against its list of other files.
pub fn is_museum_readme(name: &str) -> bool {
    let name = name.to_lowercase();
    let readme = name.contains("file_id.diz") || name.ends_with(".txt");
    let not =
        NOT_README.iter().any(|n| name.contains(n)) || name.ends_with("1001winampskins.com.txt");
    readme && !not
}

/// Whether the file at `path` is a text of the author's: a `.txt`, `.nfo` or `.diz` that
/// neither Winamp nor a plugin reads.
fn is_text(path: &str) -> bool {
    let name = base_name(path);
    let text = [".txt", ".nfo", ".diz"].iter().any(|e| name.ends_with(e));
    let read_by_winamp = NOT_README.contains(&name.as_str())
        || ["viscolor.txt", "gen_ex help.txt", "1001winampskins.com.txt"].contains(&name.as_str());
    text && !read_by_winamp
}

/// What the archive itself tells, beyond the skin's files.
struct Archive {
    folder: String,
    other_skins: Vec<String>,
    dates: Option<[String; 2]>,
    readme: Option<Readme>,
}

/// Reads the archive's entries in its own order: the folders that hold a skin, the files'
/// dates, and the first entry the museum takes for a readme.
fn archive(bytes: &[u8]) -> Archive {
    let mut out = Archive {
        folder: String::new(),
        other_skins: Vec::new(),
        dates: None,
        readme: None,
    };
    let Ok(mut zip) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return out;
    };
    let mut names = Vec::new();
    let mut dates = Vec::new();
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        if let Some(d) = entry
            .last_modified()
            .filter(|d| d.month() > 0 && d.day() > 0)
        {
            dates.push(format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                d.year(),
                d.month(),
                d.day(),
                d.hour(),
                d.minute(),
                d.second()
            ));
        }
        if out.readme.is_none() && is_museum_readme(entry.name()) {
            let mut text = Vec::new();
            if std::io::Read::read_to_end(&mut std::io::Read::take(&mut entry, 1 << 20), &mut text)
                .is_ok()
            {
                out.readme = Some(Readme {
                    file: entry.name().to_string(),
                    text: decode(&text),
                });
            }
        }
        names.push(name);
    }
    dates.sort();
    if let (Some(first), Some(last)) = (dates.first(), dates.last()) {
        out.dates = Some([first.clone(), last.clone()]);
    }
    let mut folders = crate::skin::skin_folders(&names);
    if !folders.is_empty() {
        out.folder = folders.remove(0);
        out.other_skins = folders;
    }
    out
}

/// Every element under `node`, itself first.
fn walk(node: &Rc<XmlNode>, out: &mut Vec<Rc<XmlNode>>) {
    out.push(node.clone());
    for c in &node.children {
        walk(c, out);
    }
}

/// The files whose names end in one of `endings`.
fn ending(files: &[String], endings: &[&str]) -> Vec<String> {
    files
        .iter()
        .filter(|f| {
            let lower = f.to_ascii_lowercase();
            endings.iter().any(|e| lower.ends_with(e))
        })
        .cloned()
        .collect()
}

/// The last part of a path, in lower case.
fn base_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase()
}

/// What Windows' Western code page (1252) has at 0x80 to 0x9f, where Latin-1 has control
/// characters; the five it leaves out stay as they are.
const CP1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// Text as skins write it: UTF-8, or else Windows' Western code page (1252).
fn decode(bytes: &[u8]) -> String {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes
            .iter()
            .map(|b| match b {
                0x80..=0x9f => CP1252_HIGH[(b - 0x80) as usize],
                b => *b as char,
            })
            .collect(),
    };
    text.trim_start_matches('\u{FEFF}')
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn hex(c: Rgb) -> String {
    format!("#{:06x}", c.0 & 0xff_ffff)
}

impl Info {
    /// The info as lines to read.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let mut line = |label: &str, value: &str| {
            if !value.is_empty() {
                let _ = writeln!(out, "  {label:<14}{value}");
            }
        };
        line("kind", self.kind.name());
        line("folder", &self.folder);
        line("other skins", &self.other_skins.join(", "));
        line("md5", &self.md5);
        line(
            "size",
            &format!("{} bytes, {} files", self.bytes, self.files.len()),
        );
        if let Some([first, last]) = &self.dates {
            line(
                "file dates",
                &if first == last {
                    first.clone()
                } else {
                    format!("{first} to {last}")
                },
            );
        }
        if let Some(m) = &self.modern {
            let i = &m.skin_info;
            for (label, value) in [
                ("title", &i.name),
                ("version", &i.version),
                ("author", &i.author),
                ("comment", &i.comment),
                ("email", &i.email),
                ("homepage", &i.homepage),
                ("screenshot", &i.screenshot),
                ("wacup shot", &i.wacup_screenshot),
                ("format", &m.format_version),
            ] {
                line(label, value.as_deref().unwrap_or_default());
            }
            let containers: Vec<String> = m
                .containers
                .iter()
                .map(|c| {
                    let mut s = c.id.clone();
                    if let Some(name) = c.name.as_ref().filter(|n| **n != c.id) {
                        s.push_str(&format!(" \"{name}\""));
                    }
                    if !c.default_visible {
                        s.push_str(" (closed)");
                    }
                    s
                })
                .collect();
            line("containers", &containers.join(", "));
            let windows: Vec<String> = m
                .windows
                .iter()
                .map(|w| format!("{}/{} {}x{}", w.container, w.layout, w.width, w.height))
                .collect();
            line("windows", &windows.join(", "));
            line("color themes", &m.color_themes.join(", "));
            line("scripts", &count(&m.scripts));
            line("fonts", &m.fonts.join(", "));
            let missing: Vec<String> = m
                .missing_parts
                .iter()
                .map(|p| format!("{} in {}", p.tag, p.container))
                .collect();
            line("missing parts", &missing.join(", "));
            line("plugins", &m.plugins.join(", "));
        }
        if let Some(c) = &self.classic {
            line("missing sheets", &c.missing_sheets.join(", "));
            line("extras", &c.extras.join(", "));
            if let Some(p) = &c.playlist {
                line(
                    "playlist",
                    &format!(
                        "text {}, current {}, background {}, selected {}, font {}",
                        p.normal, p.current, p.normal_bg, p.selected_bg, p.font
                    ),
                );
            }
            line("vis colors", &c.vis_colors.join(" "));
            line("regions", &c.regions.join(", "));
            line("cursors", &count(&c.cursors));
        }
        if let Some(r) = &self.readme {
            let _ = writeln!(out, "\n  readme (from {}):", r.file);
            for l in r.text.trim_end().lines() {
                let _ = writeln!(out, "    {}", l.trim_end());
            }
        }
        for t in &self.texts {
            if self.readme.as_ref().is_some_and(|r| r.text == t.text) {
                continue;
            }
            let _ = writeln!(out, "\n  {}:", t.file);
            for l in t.text.trim_end().lines() {
                let _ = writeln!(out, "    {}", l.trim_end());
            }
        }
        out
    }
}

/// How many of a kind of file there are.
fn count(files: &[String]) -> String {
    match files.len() {
        0 => String::new(),
        n => n.to_string(),
    }
}
