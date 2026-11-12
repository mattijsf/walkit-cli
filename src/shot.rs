// SPDX-License-Identifier: GPL-3.0-or-later

//! Takes the picture: loads a skin into the engine, stages the state the parameters name,
//! and draws its windows into one image.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use walkit::classic::style::ClassicStyle;
use walkit::classic::vis::{VisMode, VisStyle};
use walkit::draw::{DrawCmd, Rect};
use walkit::host::{Clock, SkinSetup, UnknownObject, WindowInfo};
use walkit::SkinRuntime;

use crate::host::{Assets, Host};
use crate::params::{parse_color, Params, VisModeParam};
use crate::player::StillPlayer;
use crate::render::{self, Canvas};
use crate::skin::{Kind, Skin};

/// A classic window's size; the playlist's is a parameter.
const CLASSIC_WINDOW: (i32, i32) = (275, 116);

/// How far the clock steps while a modern skin settles: a frame of the engine's animations.
const STEP_MS: i64 = 25;

/// A finished preview: RGBA bytes, rows top first.
pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// What the skin logged while it loaded and ran.
    pub log: Vec<String>,
}

/// A skin loaded into the engine and staged in the state the parameters name.
struct Staged {
    runtime: SkinRuntime,
    assets: Rc<RefCell<Assets>>,
    log: Rc<RefCell<Vec<String>>>,
    windows: Windows,
}

/// Loads `skin`, which scripts know as `name`, and stages the state `params` describe.
fn stage(skin: &Skin, name: &str, params: &Params) -> Staged {
    let assets = Rc::new(RefCell::new(Assets::new(skin.files())));
    let log = Rc::new(RefCell::new(Vec::new()));
    let host = Host {
        assets: assets.clone(),
        log: log.clone(),
    };
    // the clock only moves when the tool steps it, so every run draws the same frame
    let now = Rc::new(Cell::new(0i64));
    let clock: Clock = {
        let now = now.clone();
        Rc::new(move || now.get())
    };
    let mut runtime = SkinRuntime::new(
        skin.files(),
        Box::new(host),
        Box::new(StillPlayer::new(params)),
        SkinSetup {
            skin_name: name.to_string(),
            ..SkinSetup::default()
        },
        clock,
    );
    let windows = match skin.kind {
        Kind::Classic => stage_classic(&mut runtime, params),
        Kind::Modern => stage_modern(&mut runtime, params, &now),
    };
    Staged {
        runtime,
        assets,
        log,
        windows,
    }
}

/// What the engine makes of a skin once it is loaded and staged as for a picture.
pub struct Survey {
    /// The windows on screen, the main one first.
    pub windows: Vec<WindowInfo>,
    /// The skin's colour themes, in its order.
    pub color_themes: Vec<String>,
    /// The objects in the windows on screen that stand in for tags nothing defines.
    pub unknown: Vec<UnknownObject>,
    /// What the skin logged while it loaded and ran.
    pub log: Vec<String>,
}

/// Loads `skin` as [`shoot`] does and reports what the engine made of it.
pub fn survey(skin: &Skin, name: &str, params: &Params) -> Survey {
    let Staged {
        mut runtime, log, ..
    } = stage(skin, name, params);
    let windows = runtime
        .window_ids()
        .into_iter()
        .map(|id| runtime.window_info(id))
        .collect();
    let log = log.borrow().clone();
    Survey {
        windows,
        color_themes: runtime.color_themes(),
        unknown: runtime.unknown_objects(),
        log,
    }
}

/// Renders `skin`, which scripts know as `name`, in the state `params` describe.
pub fn shoot(skin: &Skin, name: &str, params: &Params) -> Result<Shot, String> {
    let Staged {
        assets,
        log,
        windows,
        ..
    } = stage(skin, name, params);
    let (width, height, windows) = match skin.kind {
        Kind::Classic => frame(windows, params.classic.width, params.classic.height, false),
        Kind::Modern => frame(windows, None, None, true),
    }?;
    let scale = params.output.scale as i32;
    let mut canvas = Canvas::new(width * scale, height * scale, 0);
    render::draw(&mut canvas, &mut assets.borrow_mut(), &windows, scale);
    if skin.kind == Kind::Modern && params.modern.trim {
        canvas = canvas.trimmed().ok_or("the skin draws nothing")?;
    }
    if let Some(color) = &params.output.background {
        canvas.put_behind(parse_color(color)?);
    }
    let log = log.borrow().clone();
    Ok(Shot {
        width: canvas.w as u32,
        height: canvas.h as u32,
        rgba: canvas.rgba(),
        log,
    })
}

type Windows = Vec<(Rect, Vec<DrawCmd>)>;

/// The image's size in skin pixels and the windows in it. A size left out is the windows'
/// extent; `fit` also moves the windows so the leftmost and topmost touch the edges.
fn frame(
    mut windows: Windows,
    width: Option<u32>,
    height: Option<u32>,
    fit: bool,
) -> Result<(i32, i32, Windows), String> {
    if windows.is_empty() {
        return Err("the skin shows no window".into());
    }
    if fit {
        let left = windows.iter().map(|w| w.0.left).min().unwrap_or(0);
        let top = windows.iter().map(|w| w.0.top).min().unwrap_or(0);
        for (rect, _) in &mut windows {
            rect.left -= left;
            rect.top -= top;
        }
    }
    let right = windows.iter().map(|w| w.0.right()).max().unwrap_or(0);
    let bottom = windows.iter().map(|w| w.0.bottom()).max().unwrap_or(0);
    let w = width.map_or(right, |w| w as i32);
    let h = height.map_or(bottom, |h| h as i32);
    if w <= 0 || h <= 0 {
        return Err("the skin's windows are empty".into());
    }
    if w > 8192 || h > 8192 {
        return Err(format!("the skin's windows span {w} by {h} pixels"));
    }
    Ok((w, h, windows))
}

/// A classic skin: the three windows where the parameters put them, each moved on its own as
/// on Winamp's desktop.
fn stage_classic(runtime: &mut SkinRuntime, params: &Params) -> Windows {
    let c = &params.classic;
    // room for every window where it is put
    let mut extent = (c.width.unwrap_or(0) as i32, c.height.unwrap_or(0) as i32);
    let mut reach = |show: bool, x: i32, y: i32, w: i32, h: i32| {
        if show {
            extent = (extent.0.max(x + w), extent.1.max(y + h));
        }
    };
    reach(
        c.main.show,
        c.main.x,
        c.main.y,
        CLASSIC_WINDOW.0,
        CLASSIC_WINDOW.1,
    );
    reach(
        c.equalizer.show,
        c.equalizer.x,
        c.equalizer.y,
        CLASSIC_WINDOW.0,
        CLASSIC_WINDOW.1,
    );
    reach(
        c.playlist.show,
        c.playlist.x,
        c.playlist.y,
        c.playlist.width,
        c.playlist.height,
    );
    runtime.set_viewport(extent.0.max(1), extent.1.max(1));
    runtime.load();
    if let Some(style) = ClassicStyle::from_version(&c.winamp) {
        runtime.set_classic_style(style);
    }
    if let Some(px) = c.playlist_font_size {
        runtime.set_classic_list_font_size(px);
    }
    for (container, show) in [("equalizer", c.equalizer.show), ("pledit", c.playlist.show)] {
        if show {
            runtime.open_window(container);
        } else {
            runtime.close_window(container);
        }
    }
    runtime.tick();
    let mut presentation = runtime.presentation();
    presentation.free_windows = true;
    presentation.viewport = (extent.0.max(1), extent.1.max(1));
    runtime.set_presentation(&presentation);
    if let Some(id) = runtime.window_of("pledit").filter(|_| c.playlist.show) {
        runtime.resize_window(id, c.playlist.width, c.playlist.height);
        runtime.place_window(id, c.playlist.x, c.playlist.y);
    }
    if let Some(id) = runtime.window_of("equalizer").filter(|_| c.equalizer.show) {
        runtime.place_window(id, c.equalizer.x, c.equalizer.y);
    }
    if let Some(id) = runtime.main_window() {
        runtime.place_window(id, c.main.x, c.main.y);
    }
    runtime.set_classic_time_remaining(c.time_remaining);
    runtime.set_classic_eq_auto(params.equalizer.auto);
    runtime.set_classic_vis_mode(match params.vis.mode {
        VisModeParam::Analyzer => VisMode::Analyzer,
        VisModeParam::Scope => VisMode::Scope,
        VisModeParam::Off => VisMode::Off,
    });
    runtime.set_classic_vis_style(VisStyle {
        peaks: params.vis.peaks,
        ..VisStyle::default()
    });
    runtime.set_classic_playlist_selection(&params.playlist.selected);
    runtime.tick();
    let mut windows = runtime.draw_windows();
    if !c.main.show {
        // the main window cannot close: it is left out of the picture
        let main = runtime.main_window().map(|id| runtime.window_rect(id));
        windows.retain(|(rect, _)| Some(*rect) != main);
    }
    windows
}

/// A modern skin: the windows it opens by itself, in its default layout at its own size,
/// after the animations it starts with have run.
fn stage_modern(runtime: &mut SkinRuntime, params: &Params, now: &Cell<i64>) -> Windows {
    let m = &params.modern;
    runtime.set_viewport(m.screen.0.max(1), m.screen.1.max(1));
    runtime.load();
    if let Some(theme) = &m.color_theme {
        runtime.select_color_theme(theme);
    }
    runtime.set_playlist_selection(params.playlist.selected.first().copied());
    runtime.tick();
    while now.get() < m.settle_ms {
        now.set(now.get() + STEP_MS);
        runtime.tick();
    }
    runtime.draw_windows()
}
