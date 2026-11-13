// SPDX-License-Identifier: GPL-3.0-or-later

//! The parameter file: what a preview shows and how. Every field has a default, and the
//! defaults are the Winamp Skin Museum's screenshot state (see the README for where each
//! value comes from), so a file only names what it changes.
//!
//! The file's JSON Schema is
//! [`schemas/params.schema.json`](https://github.com/mattijsf/walkit-cli/blob/main/schemas/params.schema.json)
//! in the repository, made by [`crate::schema::params`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Everything a preview is made from besides the skin itself.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Params {
    pub output: Output,
    pub classic: Classic,
    pub modern: Modern,
    pub player: PlayerParams,
    pub equalizer: Equalizer,
    pub vis: Vis,
    pub playlist: Playlist,
}

impl Params {
    /// Reads a parameter file (JSON). A `$schema` naming the file's schema is passed over.
    pub fn from_json(text: &str) -> Result<Params, String> {
        let mut value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if let Some(object) = value.as_object_mut() {
            object.remove("$schema");
        }
        let params: Params = serde_json::from_value(value).map_err(|e| e.to_string())?;
        params.check()?;
        Ok(params)
    }

    /// The parameters as a JSON file with every field spelled out, naming its schema.
    pub fn to_json(&self) -> String {
        let text = serde_json::to_string_pretty(self).expect("parameters serialize");
        let body = text.strip_prefix('{').unwrap_or(&text);
        format!(
            "{{\n  \"$schema\": \"{}\",{body}\n",
            crate::schema::url("params")
        )
    }

    fn check(&self) -> Result<(), String> {
        if !(1..=8).contains(&self.output.scale) {
            return Err("output.scale must be 1 to 8".into());
        }
        if let Some(color) = &self.output.background {
            parse_color(color)?;
        }
        if walkit::classic::style::ClassicStyle::from_version(&self.classic.winamp).is_none() {
            return Err("classic.winamp must be \"2.91\" or \"5.666\"".into());
        }
        if self.equalizer.bands.len() != 10 {
            return Err("equalizer.bands must hold 10 values".into());
        }
        for thumb in &self.output.thumbnails {
            if thumb.width == 0 || thumb.suffix.is_empty() {
                return Err("a thumbnail needs a width and a suffix".into());
            }
        }
        Ok(())
    }
}

/// `#rrggbb` or `#rrggbbaa` as `0xAARRGGBB`.
pub fn parse_color(text: &str) -> Result<u32, String> {
    let bad = || format!("not a color (#rrggbb or #rrggbbaa): {text}");
    let hex = text.strip_prefix('#').ok_or_else(bad)?;
    let value = u32::from_str_radix(hex, 16).map_err(|_| bad())?;
    match hex.len() {
        6 => Ok(0xff00_0000 | value),
        // rrggbbaa: the alpha moves to the front
        8 => Ok(value.rotate_right(8)),
        _ => Err(bad()),
    }
}

/// The image written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Output {
    /// Device pixels per skin pixel, a whole number; pictures scale by nearest pixels.
    pub scale: u32,
    /// What shows where no window does: `null` for transparent, or `#rrggbb`.
    pub background: Option<String>,
    /// Smaller (or larger) copies written next to the preview.
    pub thumbnails: Vec<Thumbnail>,
}

impl Default for Output {
    fn default() -> Self {
        Output {
            scale: 1,
            background: None,
            thumbnails: Vec::new(),
        }
    }
}

/// A resized copy of the preview, written as `<name><suffix>.png`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Thumbnail {
    /// Its width in pixels.
    pub width: u32,
    /// Its height; left out, the preview's shape is kept.
    #[serde(default)]
    pub height: Option<u32>,
    /// What its file name adds to the preview's, such as `-thumb`.
    pub suffix: String,
}

/// How a classic skin (`.wsz`) is set out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Classic {
    /// The image's size in skin pixels; `null` fits it to the windows.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The Winamp whose classic windows are shown: "2.91" or "5.666".
    pub winamp: String,
    pub main: Window,
    pub equalizer: Window,
    pub playlist: PlaylistWindow,
    /// The playlist font's size, pixels to the em (Winamp's "playlist font size" preference);
    /// `null` is the Winamp version's own: 10 for 2.91, 12 for 5.666.
    pub playlist_font_size: Option<i32>,
    /// Whether the time counts down.
    pub time_remaining: bool,
}

impl Default for Classic {
    fn default() -> Self {
        Classic {
            width: Some(275),
            height: Some(348),
            winamp: "2.91".into(),
            main: Window {
                show: true,
                x: 0,
                y: 0,
            },
            equalizer: Window {
                show: true,
                x: 0,
                y: 116,
            },
            playlist: PlaylistWindow {
                show: true,
                x: 0,
                y: 232,
                width: 275,
                height: 116,
            },
            playlist_font_size: None,
            time_remaining: false,
        }
    }
}

/// A classic window: whether it shows, and where its top left corner goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub show: bool,
    pub x: i32,
    pub y: i32,
}

/// The classic playlist window, which also has a size (Winamp sizes it in steps of 25 by 29
/// from 275 by 116).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaylistWindow {
    pub show: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// How a modern skin (`.wal`) is set out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Modern {
    /// The screen the skin's scripts see, in skin pixels.
    pub screen: (i32, i32),
    /// How long the skin runs before the picture is taken, in milliseconds: time for the
    /// animations it starts with to finish.
    pub settle_ms: i64,
    /// A color theme of the skin by name; `null` keeps the skin's own.
    pub color_theme: Option<String>,
    /// Whether the image is cut to what the skin draws: a window's box is often larger than
    /// its shape.
    pub trim: bool,
}

impl Default for Modern {
    fn default() -> Self {
        Modern {
            screen: (1440, 900),
            settle_ms: 1000,
            color_theme: None,
            trim: true,
        }
    }
}

/// What the transport does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum StatusParam {
    Playing,
    Paused,
    Stopped,
}

/// The player's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PlayerParams {
    pub status: StatusParam,
    /// Where the song is, in seconds.
    pub position: f64,
    /// 0 to 100.
    pub volume: i32,
    /// -100 (left) to 100 (right).
    pub balance: i32,
    pub shuffle: bool,
    pub repeat: bool,
    pub kbps: i32,
    pub khz: i32,
    /// 1 for mono, 2 for stereo.
    pub channels: i32,
}

impl Default for PlayerParams {
    fn default() -> Self {
        PlayerParams {
            status: StatusParam::Playing,
            position: 3.0,
            volume: 78,
            balance: 0,
            shuffle: true,
            repeat: true,
            kbps: 128,
            khz: 44,
            channels: 2,
        }
    }
}

/// The equalizer. Sliders run 0 (-12 dB) to 100 (+12 dB).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Equalizer {
    pub on: bool,
    pub auto: bool,
    pub preamp: i32,
    /// 60, 170, 310, 600, 1K, 3K, 6K, 12K, 14K and 16K.
    pub bands: Vec<i32>,
}

impl Default for Equalizer {
    fn default() -> Self {
        Equalizer {
            on: true,
            auto: true,
            preamp: 56,
            bands: vec![52, 74, 83, 91, 80, 54, 23, 19, 34, 75],
        }
    }
}

/// What the vis area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VisModeParam {
    Analyzer,
    Scope,
    Off,
}

/// The visualizer: a still spectrum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Vis {
    pub mode: VisModeParam,
    /// The analyzer's bars left to right, each 0 to 16 pixels tall. A classic skin shows 19;
    /// fewer leave the rest empty.
    pub bars: Vec<f32>,
    /// Peak marks over the bars.
    pub peaks: bool,
}

impl Default for Vis {
    fn default() -> Self {
        Vis {
            mode: VisModeParam::Analyzer,
            bars: vec![
                11.75, 11.0625, 8.5, 7.3125, 6.75, 6.4375, 6.25, 5.875, 5.625, 5.25, 5.125, 4.875,
                4.8125, 4.375, 3.625, 1.5625,
            ],
            peaks: false,
        }
    }
}

/// One playlist entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackParam {
    /// Shown as "artist - title"; an empty artist leaves the title alone.
    #[serde(default)]
    pub artist: String,
    pub title: String,
    /// Its length in seconds.
    pub seconds: i32,
}

impl TrackParam {
    /// The entry as the playlist and the main window name it.
    pub fn display(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} - {}", self.artist, self.title)
        }
    }
}

/// The playlist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Playlist {
    pub tracks: Vec<TrackParam>,
    /// The entry that plays, counted from 0.
    pub current: i32,
    /// The entries shown selected, counted from 0.
    pub selected: Vec<i32>,
}

impl Default for Playlist {
    fn default() -> Self {
        let track = |artist: &str, title: &str, seconds| TrackParam {
            artist: artist.into(),
            title: title.into(),
            seconds,
        };
        Playlist {
            tracks: vec![
                track("DJ Mike Llama", "Llama Whipping Intro", 5),
                track("Marilyn Manson", "Rock Is Dead", 191),
                track("Propellerheads", "Spybreak! (Short One)", 240),
                track("Ministry", "Bad Blood", 300),
            ],
            current: 0,
            selected: vec![2],
        }
    }
}
