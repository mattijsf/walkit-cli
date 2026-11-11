// SPDX-License-Identifier: GPL-3.0-or-later

use walkit_cli::params::{parse_color, Params};

#[test]
fn the_museum_file_spells_out_the_defaults() {
    let text = include_str!("../params/museum.json");
    assert_eq!(Params::from_json(text).unwrap(), Params::default());
    // the file is what `walkit params` prints
    assert_eq!(text.replace("\r\n", "\n"), Params::default().to_json());
}

#[test]
fn a_file_names_only_what_it_changes() {
    let params = Params::from_json(
        r##"{ "output": { "scale": 2, "background": "#000000" },
              "player": { "volume": 50 },
              "playlist": { "tracks": [ { "title": "One", "seconds": 61 } ], "selected": [] } }"##,
    )
    .unwrap();
    let defaults = Params::default();
    assert_eq!(params.output.scale, 2);
    assert_eq!(params.player.volume, 50);
    assert_eq!(params.player.balance, defaults.player.balance);
    assert_eq!(params.equalizer, defaults.equalizer);
    assert_eq!(params.classic, defaults.classic);
    assert_eq!(params.playlist.tracks[0].display(), "One");
    assert_eq!(params.playlist.current, 0);
}

#[test]
fn mistakes_are_refused() {
    let refused = |text: &str| Params::from_json(text).is_err();
    assert!(
        refused(r#"{ "player": { "volumee": 50 } }"#),
        "a misspelled field"
    );
    assert!(refused(r#"{ "output": { "scale": 0 } }"#), "scale 0");
    assert!(
        refused(r#"{ "output": { "background": "blue" } }"#),
        "a color by name"
    );
    assert!(
        refused(r#"{ "classic": { "winamp": "3.0" } }"#),
        "an unknown Winamp"
    );
    assert!(
        refused(r#"{ "equalizer": { "bands": [50, 50] } }"#),
        "two bands"
    );
    assert!(
        refused(r#"{ "vis": { "mode": "fire" } }"#),
        "an unknown vis mode"
    );
    assert!(refused("{"), "broken JSON");
}

#[test]
fn colors_parse_with_and_without_alpha() {
    assert_eq!(parse_color("#102030"), Ok(0xff10_2030));
    assert_eq!(parse_color("#10203080"), Ok(0x8010_2030));
    assert!(parse_color("102030").is_err());
    assert!(parse_color("#1234").is_err());
}
