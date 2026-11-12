// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs the built command-line tool.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CLASSIC: &str = "tests/skins/walkit-classic.wsz";
const MODERN: &str = "tests/skins/walkit-modern.wal";

/// An empty folder for one test, removed when the test is done.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("walkit-cli-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_walkit"))
        .args(args)
        .output()
        .expect("the tool runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn size(path: &Path) -> (u32, u32) {
    image::image_dimensions(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn one_skin_goes_to_the_file_named() {
    let dir = Scratch::new("single");
    let out = dir.path("preview.png");
    let result = run(&["render", CLASSIC, "-o", out.to_str().unwrap()]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert_eq!(size(&out), (275, 348));
    assert!(text(&result.stdout).contains("classic, 275 by 348"));
}

#[test]
fn a_batch_goes_on_past_a_bad_skin_and_fails_at_the_end() {
    let dir = Scratch::new("batch");
    let bad = dir.path("broken.wsz");
    std::fs::write(&bad, b"not a zip").unwrap();
    let out = dir.path("out");
    let result = run(&[
        "render",
        CLASSIC,
        bad.to_str().unwrap(),
        "no-such-file.wsz",
        MODERN,
        "-d",
        out.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(size(&out.join("walkit-classic.png")), (275, 348));
    assert!(out.join("walkit-modern.png").exists());
    assert!(!out.join("broken.png").exists());
    let errors = text(&result.stderr);
    assert!(errors.contains("broken.wsz"), "{errors}");
    assert!(errors.contains("no-such-file.wsz"), "{errors}");
    assert!(text(&result.stdout).contains("2 rendered, 2 failed"));
}

#[test]
fn a_folder_and_a_list_name_skins_and_twins_get_numbers() {
    let dir = Scratch::new("folder");
    let skins = dir.path("skins");
    std::fs::create_dir_all(skins.join("more")).unwrap();
    std::fs::copy(CLASSIC, skins.join("a.wsz")).unwrap();
    std::fs::copy(MODERN, skins.join("b.wal")).unwrap();
    std::fs::copy(CLASSIC, skins.join("more").join("a.wsz")).unwrap();
    std::fs::write(skins.join("notes.txt"), "not a skin").unwrap();
    let list = dir.path("list.txt");
    std::fs::write(&list, format!("# one more\n{CLASSIC}\n")).unwrap();
    let out = dir.path("out");
    let result = run(&[
        "render",
        skins.to_str().unwrap(),
        "--recursive",
        "--list",
        list.to_str().unwrap(),
        "-d",
        out.to_str().unwrap(),
        "--quiet",
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert!(result.stdout.is_empty());
    let mut names: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["a-2.png", "a.png", "b.png", "walkit-classic.png"]);
}

#[test]
fn md5_names_and_thumbnails() {
    let dir = Scratch::new("md5");
    let params = dir.path("params.json");
    std::fs::write(
        &params,
        r#"{ "output": { "thumbnails": [ { "width": 138, "suffix": "-small" } ] } }"#,
    )
    .unwrap();
    let out = dir.path("out");
    let result = run(&[
        "render",
        CLASSIC,
        "--name",
        "md5",
        "-p",
        params.to_str().unwrap(),
        "-d",
        out.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    let mut names: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), 2);
    let stem = names[0].strip_suffix("-small.png").expect("the thumbnail");
    assert_eq!(names[1], format!("{stem}.png"));
    assert_eq!(stem.len(), 32);
    assert!(stem.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(size(&out.join(&names[0])), (138, 175));
}

#[test]
fn wrong_use_exits_with_2() {
    let dir = Scratch::new("usage");
    let out = dir.path("one.png");
    let two = run(&["render", CLASSIC, MODERN, "-o", out.to_str().unwrap()]);
    assert_eq!(two.status.code(), Some(2));
    let none = run(&["render"]);
    assert_eq!(none.status.code(), Some(2));
    let params = dir.path("bad.json");
    std::fs::write(&params, r#"{ "nonsense": 1 }"#).unwrap();
    let bad = run(&["render", CLASSIC, "-p", params.to_str().unwrap()]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(text(&bad.stderr).contains("nonsense"));
    let base = run(&["render", CLASSIC, "--base", "no-such-base.wsz"]);
    assert_eq!(base.status.code(), Some(2));
}

#[test]
fn a_fonts_folder_replaces_the_stand_ins() {
    let dir = Scratch::new("fonts");
    let fonts = dir.path("fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    // any other font under Arial's name: the playlist's letters change
    std::fs::copy(
        "fonts/DejaVuSansCondensed-Bold.ttf",
        fonts.join("Arial.ttf"),
    )
    .unwrap();
    let (plain, own) = (dir.path("plain.png"), dir.path("own.png"));
    assert!(run(&["render", CLASSIC, "-o", plain.to_str().unwrap()])
        .status
        .success());
    let result = run(&[
        "render",
        CLASSIC,
        "-o",
        own.to_str().unwrap(),
        "--fonts",
        fonts.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert_ne!(std::fs::read(&plain).unwrap(), std::fs::read(&own).unwrap());

    let empty = dir.path("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let none = run(&["render", CLASSIC, "--fonts", empty.to_str().unwrap()]);
    assert_eq!(none.status.code(), Some(2));
    assert!(text(&none.stderr).contains("arial.ttf"));
}

#[test]
fn params_prints_the_museum_file() {
    let result = run(&["params"]);
    assert!(result.status.success());
    let expected = std::fs::read_to_string("params/museum.json").unwrap();
    assert_eq!(text(&result.stdout), expected.replace("\r\n", "\n"));
}

#[test]
fn compare_measures_two_images() {
    let dir = Scratch::new("compare");
    let a = dir.path("a.png");
    let b = dir.path("b.png");
    let params = dir.path("params.json");
    std::fs::write(&params, r#"{ "player": { "volume": 20 } }"#).unwrap();
    assert!(run(&["render", CLASSIC, "-o", a.to_str().unwrap()])
        .status
        .success());
    assert!(run(&[
        "render",
        CLASSIC,
        "-o",
        b.to_str().unwrap(),
        "-p",
        params.to_str().unwrap()
    ])
    .status
    .success());
    let same = run(&[
        "compare",
        a.to_str().unwrap(),
        a.to_str().unwrap(),
        "--min-same",
        "100",
    ]);
    assert!(same.status.success());
    assert!(text(&same.stdout).contains("100.00% the same"));
    let diff = dir.path("diff.png");
    let other = run(&[
        "compare",
        a.to_str().unwrap(),
        b.to_str().unwrap(),
        "--min-same",
        "100",
        "--diff",
        diff.to_str().unwrap(),
    ]);
    assert_eq!(other.status.code(), Some(1));
    assert_eq!(size(&diff), (275, 348));
}

/// Serves `body` once on a local port and gives the address.
fn serve_once(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    format!("http://127.0.0.1:{port}")
}

#[test]
fn a_skin_downloads_from_an_address() {
    let dir = Scratch::new("url");
    let address = serve_once(std::fs::read(CLASSIC).unwrap());
    let out = dir.path("out");
    let result = run(&[
        "render",
        &format!("{address}/skins/Remote%20Skin.wsz"),
        "-d",
        out.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert_eq!(size(&out.join("Remote Skin.png")), (275, 348));
}

#[test]
fn a_skin_made_for_a_plugin_says_so() {
    let dir = Scratch::new("plugin");
    let skin = dir.path("theme.wal");
    let mut out = zip::ZipWriter::new(std::fs::File::create(&skin).unwrap());
    out.start_file("skin.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    out.write_all(
        br#"<WasabiXML><include file="@COLORTHEMESPATH@/../../Plugins/classicPro/engine/load.xml"/></WasabiXML>"#,
    )
    .unwrap();
    out.finish().unwrap();
    let result = run(&[
        "render",
        skin.to_str().unwrap(),
        "-o",
        dir.path("out.png").to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    let error = text(&result.stderr);
    assert!(error.contains("the skin shows no window"), "{error}");
    assert!(error.contains("made for the classicPro plugin"), "{error}");
}

#[test]
fn a_flat_picture_names_the_wasabi_parts_the_skin_lacks() {
    let dir = Scratch::new("wasabi");
    let skin = dir.path("frame.wal");
    let mut out = zip::ZipWriter::new(std::fs::File::create(&skin).unwrap());
    out.start_file("skin.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    out.write_all(
        br#"<WasabiXML><container id="main" default_visible="1"><layout id="normal" w="100" h="40">
        <Wasabi:StandardFrame:Status x="0" y="0" w="0" h="0" relatw="1" relath="1"/>
        </layout></container></WasabiXML>"#,
    )
    .unwrap();
    out.finish().unwrap();
    let result = run(&[
        "render",
        skin.to_str().unwrap(),
        "-o",
        dir.path("out.png").to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    let warning = text(&result.stderr);
    assert!(
        warning.contains("the picture is a single colour"),
        "{warning}"
    );
    assert!(
        warning.contains("(wasabi:standardframe:status)"),
        "{warning}"
    );
    // the original skin draws a real picture and needs no warning
    let result = run(&[
        "render",
        MODERN,
        "-o",
        dir.path("ours.png").to_str().unwrap(),
    ]);
    assert!(result.status.success());
    assert!(
        !text(&result.stderr).contains("warning"),
        "{}",
        text(&result.stderr)
    );
}
