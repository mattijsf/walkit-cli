// SPDX-License-Identifier: GPL-3.0-or-later

//! What a skin is: who made it, what is in it, and what the engine makes of it. The skin's
//! files are read by the engine's [`walkit::info`]; this adds what the archive tells and puts
//! it in the shape `walkit info` prints.
//!
//! [`Info`] is what `walkit info --json` prints for each skin. Its JSON Schema is
//! [`schemas/info.schema.json`](https://github.com/mattijsf/walkit-cli/blob/main/schemas/info.schema.json)
//! in the repository, made by [`crate::schema::info`].

use std::fmt::Write as _;

use md5::{Digest, Md5};
use schemars::JsonSchema;
use serde::Serialize;
use walkit::draw::Rgb;
use walkit::info::{self as engine, decode, is_museum_readme};

use crate::params::Params;
use crate::shot::survey;
use crate::skin::{Kind, Skin};

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
    let texts = engine::texts(skin.own_files())
        .into_iter()
        .map(|t| Text {
            file: t.file,
            text: t.text,
        })
        .collect();
    let (classic, modern) = match skin.kind {
        Kind::Classic => (Some(classic(skin)), None),
        Kind::Modern => (None, Some(modern(skin, name, params))),
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

fn classic(skin: &Skin) -> ClassicInfo {
    let c = engine::classic(skin.own_files());
    ClassicInfo {
        missing_sheets: c.missing_sheets,
        extras: c.extras,
        playlist: c.playlist.map(|p| Playlist {
            normal: hex(p.normal),
            current: hex(p.current),
            normal_bg: hex(p.normal_bg),
            selected_bg: hex(p.selected_bg),
            font: p.font,
        }),
        vis_colors: c.vis_colors.into_iter().map(hex).collect(),
        regions: c.regions,
        cursors: c.cursors,
    }
}

fn modern(skin: &Skin, name: &str, params: &Params) -> ModernInfo {
    let m = engine::modern(skin.own_files());
    let i = m.skin_info;
    let skin_info = SkinInfo {
        name: i.name,
        version: i.version,
        author: i.author,
        comment: i.comment,
        email: i.email,
        homepage: i.homepage,
        screenshot: i.screenshot,
        wacup_screenshot: i.wacup_screenshot,
    };
    let containers = m
        .containers
        .into_iter()
        .map(|c| Container {
            id: c.id,
            name: c.name,
            default_visible: c.default_visible,
        })
        .collect();
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
        format_version: m.format_version,
        containers,
        windows,
        color_themes: survey.color_themes,
        scripts: m.scripts,
        fonts: m.fonts,
        missing_parts,
        plugins: m.plugins,
    }
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
