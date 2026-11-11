// SPDX-License-Identifier: GPL-3.0-or-later

//! Renders the two original skins in `tests/skins` through the library.

use walkit_cli::output;
use walkit_cli::params::{Params, Thumbnail};
use walkit_cli::shot::{shoot, Shot};
use walkit_cli::skin::{unzip, Kind, Skin};

const CLASSIC: &[u8] = include_bytes!("skins/walkit-classic.wsz");
const MODERN: &[u8] = include_bytes!("skins/walkit-modern.wal");

fn classic(params: &Params) -> Shot {
    let skin = Skin::open(CLASSIC, None, None).expect("the classic skin opens");
    assert_eq!(skin.kind, Kind::Classic);
    shoot(&skin, "walkit-classic", params).expect("it renders")
}

/// The pixel at `x`, `y` as RGBA.
fn pixel(shot: &Shot, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * shot.width + x) * 4) as usize;
    shot.rgba[i..i + 4].try_into().unwrap()
}

fn opaque(shot: &Shot) -> usize {
    shot.rgba.chunks(4).filter(|p| p[3] == 255).count()
}

#[test]
fn a_classic_skin_fills_the_museums_frame() {
    let shot = classic(&Params::default());
    assert_eq!((shot.width, shot.height), (275, 348));
    // the three windows are rectangles that cover the whole frame
    assert_eq!(opaque(&shot), 275 * 348);
}

#[test]
fn two_runs_draw_the_same_picture() {
    let a = classic(&Params::default());
    let b = classic(&Params::default());
    assert!(a.rgba == b.rgba);
}

#[test]
fn the_scale_multiplies_every_pixel() {
    let one = classic(&Params::default());
    let mut params = Params::default();
    params.output.scale = 2;
    let two = classic(&params);
    assert_eq!((two.width, two.height), (550, 696));
    for (x, y) in [(0, 0), (40, 30), (137, 200), (274, 347)] {
        let p = pixel(&one, x, y);
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(pixel(&two, x * 2 + dx, y * 2 + dy), p, "at {x},{y}");
        }
    }
}

#[test]
fn a_hidden_window_leaves_its_place_empty() {
    let mut params = Params::default();
    params.classic.equalizer.show = false;
    let shot = classic(&params);
    assert_eq!((shot.width, shot.height), (275, 348));
    assert_eq!(pixel(&shot, 100, 150), [0, 0, 0, 0]);
    assert_eq!(pixel(&shot, 100, 50)[3], 255);
    assert_eq!(pixel(&shot, 100, 300)[3], 255);
}

#[test]
fn a_frame_without_a_size_fits_the_windows() {
    let mut params = Params::default();
    params.classic.width = None;
    params.classic.height = None;
    params.classic.playlist.show = false;
    let shot = classic(&params);
    assert_eq!((shot.width, shot.height), (275, 232));
    // side by side, the playlist taller
    let mut params = Params::default();
    params.classic.width = None;
    params.classic.height = None;
    params.classic.equalizer.show = false;
    params.classic.playlist.x = 275;
    params.classic.playlist.y = 0;
    params.classic.playlist.height = 232;
    let shot = classic(&params);
    assert_eq!((shot.width, shot.height), (550, 232));
    assert_eq!(pixel(&shot, 400, 200)[3], 255);
    assert_eq!(pixel(&shot, 100, 200), [0, 0, 0, 0]);
}

#[test]
fn the_background_shows_where_no_window_does() {
    let mut params = Params::default();
    params.classic.equalizer.show = false;
    params.output.background = Some("#102030".into());
    let shot = classic(&params);
    assert_eq!(pixel(&shot, 100, 150), [0x10, 0x20, 0x30, 255]);
}

#[test]
fn the_state_changes_the_picture() {
    let base = classic(&Params::default());
    let differs = |change: &dyn Fn(&mut Params)| {
        let mut params = Params::default();
        change(&mut params);
        classic(&params).rgba != base.rgba
    };
    assert!(differs(&|p| p.player.volume = 10), "volume");
    assert!(differs(&|p| p.player.balance = -80), "balance");
    assert!(differs(&|p| p.player.position = 0.0), "position");
    assert!(differs(&|p| p.player.shuffle = false), "shuffle");
    assert!(differs(&|p| p.player.channels = 1), "channels");
    assert!(differs(&|p| p.equalizer.bands[4] = 0), "an equalizer band");
    assert!(differs(&|p| p.equalizer.auto = false), "equalizer auto");
    assert!(differs(&|p| p.vis.bars = vec![1.0; 19]), "vis bars");
    assert!(differs(&|p| p.playlist.selected = vec![0]), "selection");
    assert!(
        differs(&|p| p.playlist.tracks[1].title = "Other".into()),
        "a title"
    );
    assert!(
        differs(&|p| p.classic.winamp = "5.666".into()),
        "winamp version"
    );
}

#[test]
fn a_classic_skin_takes_missing_bitmaps_from_the_base() {
    // the classic skin without its equalizer
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(CLASSIC)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i).unwrap();
        if !entry.name().eq_ignore_ascii_case("eqmain.bmp") {
            out.raw_copy_file(entry).unwrap();
        }
    }
    let partial = out.finish().unwrap().into_inner();

    let alone = Skin::open(&partial, None, None).unwrap();
    assert_eq!(alone.missing_sheets(), ["eqmain.bmp"]);
    let alone = shoot(&alone, "partial", &Params::default()).unwrap();

    let base = unzip(CLASSIC).unwrap();
    let based = Skin::open(&partial, Some(&base), None).unwrap();
    assert!(based.missing_sheets().is_empty());
    let based = shoot(&based, "partial", &Params::default()).unwrap();

    // with the base under it the skin draws as the whole skin does
    let whole = classic(&Params::default());
    assert!(based.rgba == whole.rgba);
    assert!(alone.rgba != whole.rgba);
}

#[test]
fn a_modern_skin_renders_at_its_own_size() {
    let skin = Skin::open(MODERN, None, None).expect("the modern skin opens");
    assert_eq!(skin.kind, Kind::Modern);
    let shot = shoot(&skin, "walkit-modern", &Params::default()).expect("it renders");
    assert!(
        shot.width >= 200 && shot.height >= 100,
        "{} by {}",
        shot.width,
        shot.height
    );
    assert!(opaque(&shot) > (shot.width * shot.height / 2) as usize);
    let again = shoot(&skin, "walkit-modern", &Params::default()).unwrap();
    assert!(shot.rgba == again.rgba);
}

#[test]
fn an_archive_that_is_no_skin_is_refused() {
    assert!(Skin::open(b"not a zip", None, None).is_err());
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    out.start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    let bytes = out.finish().unwrap().into_inner();
    let error = Skin::open(&bytes, None, None).err().unwrap();
    assert!(error.contains("not a skin"), "{error}");
}

#[test]
fn thumbnails_keep_the_shape_or_take_the_size_given() {
    let shot = classic(&Params::default());
    let thumb = |width, height| {
        let t = Thumbnail {
            width,
            height,
            suffix: "-t".into(),
        };
        let (w, h, rgba) = output::thumbnail(&shot, &t).unwrap();
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        (w, h)
    };
    assert_eq!(thumb(138, None), (138, 175));
    assert_eq!(thumb(550, None), (550, 696));
    assert_eq!(thumb(100, Some(100)), (100, 100));
}

#[test]
fn the_png_decodes_to_the_same_pixels() {
    let shot = classic(&Params::default());
    let bytes = output::png(shot.width, shot.height, &shot.rgba).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(decoded.dimensions(), (275, 348));
    assert!(decoded.into_raw() == shot.rgba);
}
