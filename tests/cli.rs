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

#[test]
fn info_tells_what_a_modern_skin_says_of_itself() {
    let result = run(&["info", MODERN]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    let out = text(&result.stdout);
    for line in [
        "kind          modern",
        "title         walkit modern",
        "author        walkit",
        "containers    main \"Main Window\", pledit \"Playlist Editor\"",
        "windows       main/normal 300x112",
    ] {
        assert!(out.contains(line), "{line} in\n{out}");
    }
    assert!(!out.contains("missing parts"), "{out}");
}

#[test]
fn info_reads_a_classic_skins_settings() {
    let result = run(&["info", "--json", CLASSIC]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    let info: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(info["kind"], "classic");
    let classic = &info["classic"];
    assert_eq!(classic["missing_sheets"], serde_json::json!([]));
    assert_eq!(info["folder"], "");
    assert_eq!(classic["playlist"]["font"], "Arial");
    assert_eq!(classic["vis_colors"].as_array().unwrap().len(), 24);
}

#[test]
fn info_names_the_parts_a_window_lacks() {
    let dir = Scratch::new("info-wasabi");
    let skin = dir.path("frame.wal");
    let mut out = zip::ZipWriter::new(std::fs::File::create(&skin).unwrap());
    out.start_file("skin.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    out.write_all(
        br#"<WasabiXML><skininfo><name>Frame</name><author>Someone</author></skininfo>
        <container id="main" default_visible="1"><layout id="normal" w="100" h="40">
        <Wasabi:StandardFrame:Status x="0" y="0" w="0" h="0" relatw="1" relath="1"/>
        </layout></container></WasabiXML>"#,
    )
    .unwrap();
    out.start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    out.write_all(b"Made by someone.\r\nEnjoy.").unwrap();
    out.finish().unwrap();
    let result = run(&["info", "--json", skin.to_str().unwrap(), MODERN]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    let infos: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let frame = &infos[0];
    assert_eq!(frame["modern"]["skin_info"]["author"], "Someone");
    assert_eq!(
        frame["modern"]["missing_parts"],
        serde_json::json!([{"tag": "wasabi:standardframe:status", "container": "main"}])
    );
    assert_eq!(
        frame["texts"],
        serde_json::json!([{"file": "readme.txt", "text": "Made by someone.\nEnjoy."}])
    );
    assert_eq!(infos[1]["name"], "walkit-modern");
}

#[test]
fn info_picks_the_readme_as_the_museum_does() {
    let dir = Scratch::new("info-readme");
    let skin = dir.path("notes.wsz");
    let classic = std::fs::read(CLASSIC).unwrap();
    let mut from = zip::ZipArchive::new(std::io::Cursor::new(classic)).unwrap();
    let mut out = zip::ZipWriter::new(std::fs::File::create(&skin).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    // settings files come first and are passed over; the first other text file is the readme
    for (name, bytes) in [
        ("Notes/pledit.txt", &b"[Text]\r\nFont=Arial"[..]),
        ("Notes/genex.txt", b"not a readme"),
        ("Notes/About.TXT", b"\xa9 2004 Someone \x96 enjoy"),
        ("Notes/readme.txt", b"second"),
    ] {
        out.start_file(name, options).unwrap();
        out.write_all(bytes).unwrap();
    }
    for i in 0..from.len() {
        let entry = from.by_index(i).unwrap();
        let name = format!("Notes/{}", entry.name());
        if !name.ends_with(".txt") {
            out.raw_copy_file_rename(entry, name).unwrap();
        }
    }
    out.finish().unwrap();
    let result = run(&["info", "--json", skin.to_str().unwrap()]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    let info: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        info["readme"],
        serde_json::json!({"file": "Notes/About.TXT", "text": "© 2004 Someone – enjoy"})
    );
    let result = run(&["info", skin.to_str().unwrap()]);
    assert!(
        text(&result.stdout).contains("readme (from Notes/About.TXT):"),
        "{}",
        text(&result.stdout)
    );
}

#[test]
fn info_tells_where_in_the_archive_the_skin_is() {
    let dir = Scratch::new("info-folders");
    let skin = dir.path("pack.wal");
    let modern = std::fs::read(MODERN).unwrap();
    let mut from = zip::ZipArchive::new(std::io::Cursor::new(modern)).unwrap();
    let mut out = zip::ZipWriter::new(std::fs::File::create(&skin).unwrap());
    let options = zip::write::SimpleFileOptions::default()
        .last_modified_time(zip::DateTime::from_date_and_time(2004, 5, 6, 7, 8, 10).unwrap());
    // two skins in one archive: the first folder with a skin.xml is the one read
    for folder in ["Blue/", "Red/"] {
        for i in 0..from.len() {
            let entry = from.by_index(i).unwrap();
            let name = format!("{folder}{}", entry.name());
            out.raw_copy_file_rename(entry, name).unwrap();
        }
    }
    out.start_file("Blue/genex.txt", options).unwrap();
    out.write_all(b"settings").unwrap();
    out.start_file("Blue/whatsnew.nfo", options).unwrap();
    out.write_all(b"news").unwrap();
    out.finish().unwrap();
    let result = run(&["info", "--json", skin.to_str().unwrap()]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    let info: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(info["folder"], "Blue/");
    assert_eq!(info["other_skins"], serde_json::json!(["Red/"]));
    assert_eq!(info["dates"][0], "2004-05-06 07:08:10");
    assert_eq!(
        info["texts"],
        serde_json::json!([{"file": "whatsnew.nfo", "text": "news"}])
    );
    let result = run(&["info", skin.to_str().unwrap()]);
    let out = text(&result.stdout);
    assert!(out.contains("other skins   Red/"), "{out}");
}

/// An archive of the original modern skin in each of `folders` ("" for the root).
fn pack(path: &Path, folders: &[&str]) {
    let modern = std::fs::read(MODERN).unwrap();
    let mut from = zip::ZipArchive::new(std::io::Cursor::new(modern)).unwrap();
    let mut out = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for folder in folders {
        for i in 0..from.len() {
            let entry = from.by_index(i).unwrap();
            let name = format!("{folder}{}", entry.name());
            out.raw_copy_file_rename(entry, name).unwrap();
        }
    }
    out.finish().unwrap();
}

#[test]
fn a_pack_of_skins_says_it_is_no_single_skin() {
    let dir = Scratch::new("pack");
    let skin = dir.path("pack.wal");
    pack(&skin, &["Blue/", "Red/"]);
    let result = run(&[
        "render",
        skin.to_str().unwrap(),
        "-o",
        dir.path("out.png").to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    let warning = text(&result.stderr);
    assert!(
        warning.contains("not a single skin but a pack of 2 skins to unpack by hand; only \"Blue\" is read, without \"Red\""),
        "{warning}"
    );
    // a skin at the root is the archive's skin, as Winamp reads it; a folder beside it is
    // only more files
    let skin = dir.path("root.wal");
    pack(&skin, &["2003/", ""]);
    let result = run(&[
        "render",
        skin.to_str().unwrap(),
        "-o",
        dir.path("root.png").to_str().unwrap(),
    ]);
    assert!(result.status.success());
    assert!(
        !text(&result.stderr).contains("warning"),
        "{}",
        text(&result.stderr)
    );
    let result = run(&["info", "--json", skin.to_str().unwrap()]);
    let info: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(info["folder"], "");
    assert_eq!(info["other_skins"], serde_json::json!(["2003/"]));
}

/// The files of an archive, by name, as bytes.
fn entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut files = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        files.push((entry.name().to_string(), bytes));
    }
    files.sort();
    files
}

#[test]
fn a_skin_unpacked_and_packed_again_has_the_same_files_and_picture() {
    let dir = Scratch::new("repack");
    let unpacked = dir.path("unpacked");
    let result = run(&["unpack", "tests/skins", "-d", unpacked.to_str().unwrap()]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert!(text(&result.stdout).contains("2 unpacked, 0 failed"));
    assert!(unpacked.join("walkit-classic/main.bmp").is_file());
    // a desktop's own files are not packed
    std::fs::write(unpacked.join("walkit-modern/.DS_Store"), b"junk").unwrap();
    // the folder of skins packs each skin in it, named and ended after what it holds
    let packed = dir.path("packed");
    let result = run(&[
        "pack",
        unpacked.to_str().unwrap(),
        "-d",
        packed.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert!(text(&result.stdout).contains("2 packed, 0 failed"));
    for skin in [CLASSIC, MODERN] {
        let name = Path::new(skin).file_name().unwrap();
        assert_eq!(
            entries(&packed.join(name)),
            entries(Path::new(skin)),
            "{skin}"
        );
        let a = dir.path("a.png");
        let b = dir.path("b.png");
        run(&["render", skin, "-o", a.to_str().unwrap()]);
        let again = packed.join(name);
        run(&["render", again.to_str().unwrap(), "-o", b.to_str().unwrap()]);
        let result = run(&[
            "compare",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--min-same",
            "100",
        ]);
        assert!(result.status.success(), "{}", text(&result.stdout));
    }
    // the same files pack to the same bytes, and a skin that is there is kept
    let first = std::fs::read(packed.join("walkit-classic.wsz")).unwrap();
    let skin = unpacked.join("walkit-classic");
    let result = run(&[
        "pack",
        skin.to_str().unwrap(),
        "-d",
        packed.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert!(text(&result.stderr).contains("--force replaces it"));
    let result = run(&[
        "pack",
        skin.to_str().unwrap(),
        "-d",
        packed.to_str().unwrap(),
        "-f",
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert_eq!(
        std::fs::read(packed.join("walkit-classic.wsz")).unwrap(),
        first
    );
    // a folder that is there is only written into with --force
    let result = run(&["unpack", CLASSIC, "-d", unpacked.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    assert!(text(&result.stderr).contains("--force writes into it"));
    let result = run(&["unpack", CLASSIC, "-d", unpacked.to_str().unwrap(), "-f"]);
    assert!(result.status.success(), "{}", text(&result.stderr));
}

#[test]
fn a_skin_in_a_folder_packs_as_it_unpacked() {
    let dir = Scratch::new("repack-folder");
    let skin = dir.path("Blue.wal");
    pack(&skin, &["Blue/"]);
    let unpacked = dir.path("unpacked");
    let result = run(&[
        "unpack",
        skin.to_str().unwrap(),
        "-d",
        unpacked.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert!(unpacked.join("Blue/Blue/skin.xml").is_file());
    // the skin is in the folder's only folder: it is still one skin, and modern
    let folder = unpacked.join("Blue");
    let result = run(&[
        "pack",
        folder.to_str().unwrap(),
        "-d",
        dir.path("out").to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert_eq!(entries(&dir.path("out/Blue.wal")), entries(&skin));
    // --ext names what it is written as
    let result = run(&[
        "pack",
        folder.to_str().unwrap(),
        "-d",
        dir.path("out").to_str().unwrap(),
        "--ext",
        "zip",
    ]);
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert!(dir.path("out/Blue.zip").is_file());
    // a folder with no skin fails
    let empty = dir.path("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let result = run(&[
        "pack",
        empty.to_str().unwrap(),
        "-d",
        dir.path("out").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert!(text(&result.stderr).contains("no skin in it"));
}

#[test]
fn an_archive_that_would_write_outside_its_folder_unpacks_nothing() {
    let dir = Scratch::new("zip-slip");
    let skin = dir.path("evil.wsz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&skin).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("main.bmp", options).unwrap();
    zip.write_all(b"BM").unwrap();
    zip.start_file("../../escaped.txt", options).unwrap();
    zip.write_all(b"out").unwrap();
    zip.finish().unwrap();
    let out = dir.path("out");
    let result = run(&[
        "unpack",
        skin.to_str().unwrap(),
        "-d",
        out.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert!(
        text(&result.stderr).contains("would land outside the folder"),
        "{}",
        text(&result.stderr)
    );
    assert!(!out.join("evil").exists());
    assert!(!dir.path("escaped.txt").exists());
}
