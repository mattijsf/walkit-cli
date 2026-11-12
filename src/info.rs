// SPDX-License-Identifier: GPL-3.0-or-later

//! What a skin is: who made it, what is in it, and what the engine makes of it.

use std::fmt::Write as _;
use std::rc::Rc;

use md5::{Digest, Md5};
use serde::Serialize;
use walkit::classic::{pledit, region, viscolor};
use walkit::draw::Rgb;
use walkit::xml::{self, XmlNode};

use crate::params::Params;
use crate::shot::survey;
use crate::skin::{Kind, Skin, CLASSIC_SHEETS};

/// The bitmaps a classic skin may add to the twelve Winamp draws its windows from.
const CLASSIC_EXTRAS: [&str; 7] = [
    "nums_ex.bmp",
    "balance.bmp",
    "gen.bmp",
    "genex.bmp",
    "video.bmp",
    "mb.bmp",
    "avs.bmp",
];

/// The text files Winamp reads settings from; any other is the author's.
const SETTINGS_TEXTS: [&str; 3] = ["pledit.txt", "viscolor.txt", "region.txt"];

/// Everything known about one skin.
#[derive(Debug, Serialize)]
pub struct Info {
    /// The skin's name: its file name without the ending.
    pub name: String,
    pub md5: String,
    pub bytes: usize,
    /// `classic` or `modern`.
    pub kind: &'static str,
    /// The files in the archive, by path.
    pub files: Vec<String>,
    /// The author's text files: readmes, licences, notes.
    pub texts: Vec<Text>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classic: Option<ClassicInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modern: Option<ModernInfo>,
}

/// A text file of the author's.
#[derive(Debug, Serialize)]
pub struct Text {
    pub file: String,
    pub text: String,
}

/// What a classic skin holds.
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
pub struct Playlist {
    pub normal: String,
    pub current: String,
    pub normal_bg: String,
    pub selected_bg: String,
    pub font: String,
}

/// What a modern skin holds and how it loads.
#[derive(Debug, Serialize)]
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
#[derive(Debug, Default, Serialize)]
pub struct SkinInfo {
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub comment: Option<String>,
    pub email: Option<String>,
    pub homepage: Option<String>,
    pub screenshot: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Container {
    pub id: String,
    pub name: Option<String>,
    /// Whether it opens when the skin loads.
    pub default_visible: bool,
}

#[derive(Debug, Serialize)]
pub struct Window {
    pub container: String,
    pub layout: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Serialize)]
pub struct MissingPart {
    pub tag: String,
    pub container: String,
}

/// Inspects `skin`, opened from `bytes`, which scripts know as `name`. A modern skin is loaded
/// into the engine, staged as `params` describe.
pub fn inspect(skin: &Skin, bytes: &[u8], name: &str, params: &Params) -> Info {
    let files = skin.names();
    let texts = files
        .iter()
        .filter(|f| f.to_ascii_lowercase().ends_with(".txt"))
        .filter(|f| !SETTINGS_TEXTS.contains(&base_name(f).as_str()))
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
        kind: match skin.kind {
            Kind::Classic => "classic",
            Kind::Modern => "modern",
        },
        files,
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

/// Text as skins write it: UTF-8, or else Windows' Western code page, read as Latin-1.
fn decode(bytes: &[u8]) -> String {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|b| *b as char).collect(),
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
        line("kind", self.kind);
        line("md5", &self.md5);
        line(
            "size",
            &format!("{} bytes, {} files", self.bytes, self.files.len()),
        );
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
        for t in &self.texts {
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
