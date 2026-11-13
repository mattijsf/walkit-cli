# walkit-cli

A command-line tool that renders preview images and thumbnails of Winamp skins: classic
skins (`.wsz`) and modern skins (`.wal`). It runs the skin in the
[walkit](https://github.com/mattijsf/walkit) engine, stages a fixed player state, draws the
windows in software and writes a PNG. No browser and no Winamp are involved, so it runs
headless, takes a few hundredths of a second per classic skin, and gives the same bytes on every run.

It was written with the [Winamp Skin Museum](https://skins.webamp.org/) in mind. With no
parameter file, a classic skin comes out in the state and layout of the museum's own
screenshots: 275 by 348 pixels, main window, equalizer and playlist stacked, transparent
where the skin is.

```
walkit render Zelda_Amp_3.wsz
```

## Install

Download a binary for Linux, macOS or Windows from the
[releases](https://github.com/mattijsf/walkit-cli/releases), or install it from
[crates.io](https://crates.io/crates/walkit-cli) with a stable [Rust](https://rustup.rs)
toolchain:

```
cargo install walkit-cli
```

The latest commit on `main` installs with
`cargo install --git https://github.com/mattijsf/walkit-cli`.

From a checkout, `cargo build --release` leaves the binary at
`target/release/walkit`.

The binary is self-contained. The fonts it needs are built in.

> [!IMPORTANT]
> For images that match Winamp on Windows letter for letter, pass the real Arial with
> `--fonts`. The built-in fonts are free stand-ins: text comes out as crisp as Winamp's, but
> the letters are shaped a little differently. See [Fonts](#fonts).

## Use

### One skin

```
walkit render skin.wsz                    # writes ./skin.png
walkit render skin.wsz -o preview.png
walkit render https://example.org/skin.wal -o preview.png
```

### Many skins

Inputs are files, http(s) addresses and folders, in any mix. A folder stands for the `.wsz`,
`.wal` and `.zip` files in it (`--recursive` includes the folders under it). `--list` reads
more inputs from a file, one per line; `-` reads them from standard input. Empty lines and
lines starting with `#` are skipped.

```
walkit render skins/ -d previews/
walkit render --list skins.txt -d previews/ --name md5
find . -name '*.wsz' | walkit render --list - -d previews/ --jobs 8
```

Each preview is named after its skin: `Zelda_Amp_3.wsz` becomes `Zelda_Amp_3.png`, and a
second skin of the same name in the batch becomes `Zelda_Amp_3-2.png`. With `--name md5` the
file is named after the md5 of the skin's bytes, which is how the museum names its
screenshots (`screenshots/<md5>.png`).

A skin that fails (a broken archive, a download that does not answer, an archive that is not
a skin) is reported on standard error and the batch goes on. The exit code tells how it went:

| Code | Meaning |
| --- | --- |
| 0 | every skin rendered |
| 1 | at least one skin failed |
| 2 | the command itself was wrong: no inputs, a bad parameter file, a base that does not load |

### Base skin and wasabi base

Many classic skins leave bitmaps out, most often the equalizer or the playlist. Winamp draws
those parts from its base skin. Pass a complete classic skin with `--base` and the tool
does the same; without it those parts stay empty and a warning names the missing bitmaps.
Winamp's base skin is in the museum, so this works:

```
walkit render skins/ -d previews/ \
  --base https://r2.webampskins.org/skins/5e4f10275dcb1fb211d4a8b4f1bda236.wsz
```

Some modern skins borrow frames, scrollbars and buttons from the wasabi folder that Winamp
lays under every modern skin. Pass a zip of that folder with `--wasabi`. The walkit docs
explain how to make it: [the wasabi base](https://mattijsf.github.io/walkit/wasabi-base/).
Winamp 3's own default skin is one of them: without the folder its window comes out as one
flat colour. When a picture is a single colour, or a skin draws nothing, the warning or
failure names the wasabi parts the skin uses and does not define itself.

Skins made for the classicPro plugin (most "cPro" skins) are colour themes: their windows
are in the plugin's files in Winamp's `Plugins` folder, not in the skin. They fail with "the
skin shows no window", and the message says which plugin they are made for.

Both options take a file or an address. Neither file ships with this repository: they are
Winamp's art.

### Thumbnails

The museum stores one screenshot per skin and scales it in the browser, so no thumbnails are
written by default. A parameter file can ask for resized copies:

```json
{ "output": { "thumbnails": [ { "width": 138, "suffix": "-thumb" } ] } }
```

This writes `skin-thumb.png` next to `skin.png`. A thumbnail that is larger than the preview
is scaled with nearest pixels and stays sharp; a smaller one is scaled with a Lanczos filter.
Those are the two filters the museum uses for its link preview images. Leave `height` out to
keep the preview's shape. For sharp enlargements at a whole factor, `output.scale` renders
the skin itself larger.

### Comparing images

```
walkit compare ours.png reference.png --bands 116 --diff diff.png
```

prints the share of identical pixels, for the whole image and per band of rows (116 rows is
one classic window), and can write a picture of where the two differ. `--min-same 99` makes
it fail under a threshold. `scripts/compare-with-museum.sh <md5>...` renders skins from the
museum and measures each against the museum's screenshot.

### Skin info

```
walkit info Bento.wal
walkit info --json skins/ > skins.json
```

prints what is known about a skin: its md5 and files, its readme, the author's other text
files, and by kind:

- a classic skin: the bitmaps it lacks and the optional ones it has, the playlist's colours
  and font from `pledit.txt`, the 24 vis colours, the windows `region.txt` shapes, and its
  cursors.
- a modern skin: what `<skininfo>` says (title, version, author, comment, email, homepage),
  the format version, its containers, the windows that open once it has loaded and their
  sizes, its colour themes, scripts and fonts, the parts of drawn windows nothing defines
  (such as `wasabi:standardframe:status` without `--wasabi`), and the plugin it is made for.

The readme is picked as the Winamp Skin Museum picks it (`getReadme` in webamp's
`packages/skin-database/data/SkinModel.ts`): the first entry, in the archive's order, whose
name ends in `.txt` or holds `file_id.diz`, leaving out the files Winamp and its plugins read
(`pledit.txt`, `genex.txt`, `region.txt` and others). Its path in the archive is printed with
it. Text that is not UTF-8 is read as Windows-1252, where the museum shows `�`. The museum
shows no readme for modern skins; `info` picks one by the same rule.

`--json` prints an object for one skin and an array for more. `--base` and `--wasabi` lay the
same files under the skins as for `render`.

## The parameter file

`--params file.json` sets what the preview shows. Every field is optional: a file names only
what it changes, and everything else keeps the museum's value. An unknown field is an error.

`walkit params` prints the defaults as a complete file. The same file is in the
repository as [`params/museum.json`](params/museum.json):

```json
{
  "output": { "scale": 1, "background": null, "thumbnails": [] },
  "classic": {
    "width": 275,
    "height": 348,
    "winamp": "2.91",
    "main": { "show": true, "x": 0, "y": 0 },
    "equalizer": { "show": true, "x": 0, "y": 116 },
    "playlist": { "show": true, "x": 0, "y": 232, "width": 275, "height": 116 },
    "playlist_font_size": null,
    "time_remaining": false
  },
  "modern": { "screen": [1440, 900], "settle_ms": 1000, "color_theme": null, "trim": true },
  "player": {
    "status": "playing",
    "position": 3.0,
    "volume": 78,
    "balance": 0,
    "shuffle": true,
    "repeat": true,
    "kbps": 128,
    "khz": 44,
    "channels": 2
  },
  "equalizer": {
    "on": true,
    "auto": true,
    "preamp": 56,
    "bands": [52, 74, 83, 91, 80, 54, 23, 19, 34, 75]
  },
  "vis": {
    "mode": "analyzer",
    "bars": [11.75, 11.0625, 8.5, 7.3125, 6.75, 6.4375, 6.25, 5.875, 5.625, 5.25,
             5.125, 4.875, 4.8125, 4.375, 3.625, 1.5625],
    "peaks": false
  },
  "playlist": {
    "tracks": [
      { "artist": "DJ Mike Llama", "title": "Llama Whipping Intro", "seconds": 5 },
      { "artist": "Marilyn Manson", "title": "Rock Is Dead", "seconds": 191 },
      { "artist": "Propellerheads", "title": "Spybreak! (Short One)", "seconds": 240 },
      { "artist": "Ministry", "title": "Bad Blood", "seconds": 300 }
    ],
    "current": 0,
    "selected": [2]
  }
}
```

| Field | Meaning |
| --- | --- |
| `output.scale` | Device pixels per skin pixel, 1 to 8. Pictures scale by nearest pixels. |
| `output.background` | `null` for transparent, or `#rrggbb` / `#rrggbbaa`. |
| `output.thumbnails` | Resized copies: `width`, optional `height`, and the `suffix` added to the file name. |
| `classic.width`, `classic.height` | The image's size in skin pixels. `null` fits it to the windows shown. |
| `classic.winamp` | Which Winamp the classic windows follow: `"2.91"` or `"5.666"`. They differ in the playlist's font size. |
| `classic.main`, `.equalizer`, `.playlist` | Whether each window shows and where its top left corner goes. The playlist also has a size; Winamp sizes it in steps of 25 by 29 pixels from 275 by 116. |
| `classic.playlist_font_size` | Pixels to the em, as Winamp's playlist font size preference. `null` is the version's own: 10 for 2.91, 12 for 5.666. |
| `classic.time_remaining` | The time counts down. |
| `modern.screen` | The screen size the skin's scripts see. |
| `modern.settle_ms` | How long the skin runs before the picture is taken, so the animations it starts with can finish. The clock is simulated; nothing waits. |
| `modern.color_theme` | One of the skin's color themes by name. `null` keeps the skin's own. |
| `modern.trim` | Cut the image to what the skin draws. A window's box is often larger than its shape. |
| `player.status` | `"playing"`, `"paused"` or `"stopped"`. |
| `player.position` | Seconds into the song. |
| `player.volume` | 0 to 100. |
| `player.balance` | -100 (left) to 100 (right). |
| `player.kbps`, `.khz`, `.channels` | The song's format as the main window shows it. |
| `equalizer.preamp`, `.bands` | 0 (-12 dB) to 100 (+12 dB). The ten bands are 60, 170, 310, 600, 1K, 3K, 6K, 12K, 14K, 16K. |
| `vis.mode` | `"analyzer"`, `"scope"` or `"off"`. |
| `vis.bars` | The analyzer's bars left to right, in pixels of height. A classic skin shows 19 bars; Winamp draws a bar at most 13 pixels tall. |
| `vis.peaks` | Peak marks over the bars. |
| `playlist.tracks` | Shown as "artist - title"; leave `artist` out for a bare title. |
| `playlist.current`, `.selected` | The entry that plays and the entries shown selected, counted from 0. |

The player, equalizer, vis and playlist settings apply to both kinds of skin. The `classic`
block applies to `.wsz` skins and the `modern` block to `.wal` skins.

## Where the defaults come from

The defaults describe what the museum's screenshots of classic skins show. Every screenshot
is in the same state, so the state can be read from the images:

| Default | What the museum's screenshots show |
| --- | --- |
| Image of 275 by 348 pixels, transparent background, PNG | Every screenshot has this size, and is transparent where the skin's windows are. |
| One image per skin, no thumbnails | The museum has one image per skin, named by the skin's md5, and scales it in the browser. |
| Main window at 0,0, equalizer at 0,116, playlist at 0,232 at its smallest size | The three windows are stacked in this order. |
| Playing, 3 seconds in | The time reads 00:03 and the play indicator is lit. |
| The four tracks, the first one current, the third selected, 128 kbps, 44 kHz, stereo | The playlist and the main window's displays. |
| Equalizer sliders and preamp | The positions of the eleven sliders. |
| Shuffle on, repeat on, equalizer on and auto on, the title not scrolling | The lit buttons and the title's position. |
| The analyzer's sixteen bars | The heights of the bars in the vis area. |
| Volume 78, balance 0 | The positions of the two sliders. |
| Winamp 2.91's look, playlist font at 10 pixels | The main window's layout and the height of the playlist's rows. |
| Nearest pixels when a thumbnail grows, Lanczos when it shrinks | Keeps pixel art sharp when enlarged and smooth when reduced. |

PNG is lossless, so however an image is compressed, the pixels are the same.

### Modern skins

The museum has no screenshots of modern skins. The only image it has for a modern skin is
the one its author put in the archive and named in `skin.xml`, and many skins have none.

With no convention to follow, the tool uses this one:

- The skin opens the windows it opens by itself, in its default layout, at its own size.
- It gets the same player state as a classic skin: the same song, position, volume,
  equalizer and playlist.
- It runs for one simulated second, so opening animations have finished.
- The image is transparent outside the skin's shape and cut to what the skin draws. Its size
  therefore differs from skin to skin.

## How close it is to the museum's images

Nine classic skins from the museum, rendered with the defaults and the museum's base skin
under them, compared pixel by pixel with the museum's screenshot of the same skin:

| Skin | Identical pixels | Outside the playlist's text rows |
| --- | --- | --- |
| base-2.91 | 94.65% | 98.10% |
| Winamp3_Classified_v5.5 | 94.63% | 98.07% |
| Winamp5_Classified_v5.5 | 94.19% | 97.53% |
| Bento_Classified | 94.56% | 97.96% |
| Garfield | 94.33% | 97.73% |
| Zelda_Amp_3 | 94.91% | 98.39% |
| Deus_Ex_Amp_by_AJ | 95.48% | 99.04% |
| the_Cube_v2_by_Kuki | 94.08% | 98.37% |
| Microchip_2 | 94.34% | 98.04% |

To the eye the images are the same picture. Where they differ, the same state was set up in
Winamp 5.666 in a virtual machine (the same skin, tracks, position, volume, equalizer and
selection, playlist font at 10) and both images were compared with what Winamp draws. Each
difference below is one where the tool follows Winamp instead of the museum's image.

1. **Playlist text.** About 3,300 of the roughly 5,100 differing pixels are in the four
   playlist rows. The museum's text is drawn by a browser and smoothed. Winamp's is not:
   Windows fits Arial to the pixel grid at this size and leaves it unsmoothed, and the tool
   draws text the same way (see [Fonts](#fonts)). With Arial itself passed in `--fonts`,
   the four rows came out identical to Winamp's, pixel for pixel; with the built-in
   stand-in the letters are as crisp but shaped a little differently. The rows also sit
   one pixel higher than in the museum's image, which is where Winamp puts them.
2. **Slider thumbs.** The slider thumbs in the museum's images land up to three pixels
   from Winamp's: the volume thumb is three pixels further right in the
   museum's image, the position thumb one or two, the equalizer thumbs one pixel off, and
   one equalizer band shows the next color of its bar. In Winamp, the volume and balance
   sliders, the position bar and all eleven equalizer sliders came out identical to the
   tool's, pixel for pixel.
3. **The analyzer's colors.** The bars have the museum's heights, but Winamp colors a bar
   from the sixteenth entry of `viscolor.txt` at the bottom, and the museum's image starts
   one entry further down. Each bar is therefore one shade off.
4. **The cell left of the time.** Winamp draws the blank cell of `numbers.bmp` where the
   minus sign goes. The museum's image shows the main window's background there. On skins where the
   two differ, a few dozen pixels do (35 on the base skin).
5. **Window shapes.** For a skin with a `region.txt`, the tool cuts the windows as
   Winamp does. On Microchip_2 the museum's image leaves 689 pixels opaque that Winamp
   cuts away; the tool's shape matched Winamp's exactly.
6. **The equalizer's curve.** The line in the equalizer's graph differs from the museum's
   in about 90 pixels. The tool draws the curve as Winamp does; the museum's differs from
   Winamp's in 90 pixels for these slider values.

The title in the main window does not scroll in either image, the visualizer window stays
closed, and the main window has the focus.

## Fonts

Skins name Windows fonts, mostly Arial and Tahoma. The tool draws them with free
stand-ins that are compiled into the binary: Liberation Sans for Arial (SIL Open Font
License) and DejaVu Sans Condensed for Tahoma (Bitstream Vera license). The font files and
their licenses are in [`fonts/`](fonts). A skin's own TrueType fonts are read from the skin.

Text is drawn by the walkit engine's own TrueType drawing (its `truetype` feature), as
Winamp draws it. A classic skin's text is drawn the way Windows draws it:

- The font's own hinting fits each letter to the pixel grid.
- Letters advance by whole pixels, with the widths the font's device tables give and the
  kerning of its `kern` table. A font whose `GPOS` table leaves kerning out of Latin text
  is not kerned, as Windows does not kern it: Tahoma is not, Arial is.
- The font's `gasp` table says at which sizes letters are smoothed. Arial from 9 to 17
  pixels, which covers the playlist, is not smoothed at all. Smoothed letters blend with
  Windows' gamma.

A modern skin's text is always smoothed, as Winamp draws it, fitted to the pixel grid by
an autohinter at three quarters of the skin's font size, unless the skin sets
`antialias="0"`.

### A Winamp reference needs Arial

Liberation Sans has Arial's widths but not its letter shapes, and DejaVu Sans Condensed
only resembles Tahoma. An image drawn with the stand-ins is therefore a good preview, not
a reference for what Winamp shows on Windows: on the playlist of the default state about
550 pixels differ from Winamp's. With Arial itself those rows are identical.

So for a reference, pass a folder that holds the real fonts under the names Windows gives
them (`arial.ttf`, `arialbd.ttf`, `tahoma.ttf`, `tahomabd.ttf`; any of the four). The
classic playlist needs only `arial.ttf`:

```
walkit render skin.wsz --fonts /path/to/fonts
```

On Windows they are in `C:\Windows\Fonts`. They are Microsoft's and are not part of this
repository.

Checked against Winamp 5.666 in a virtual machine, with the real fonts: a classic playlist
of twelve test lines (both alphabets, digits, punctuation, pangrams) at playlist font sizes
10 to 24, in Arial and in Tahoma. Rows sit at the same height at every size. At size 10,
Arial differs in 2 pixels (the `@`), Tahoma in 11. Up to size 16 no size differs in more
than 150 pixels. From size 17 (Tahoma) and 18 (Arial), where letters are smoothed, they sit
where Winamp puts them but their shades differ, in about 7,000 to 9,000 pixels of the
playlist. Modern skins' letters land where Winamp puts them; their shades differ a little.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for the checks, the commit message format and how
releases are made.

The tests render the two skins in `tests/skins`. Both are original work from the walkit
repository and are under this repository's license. Please do not add other people's skins
to the repository; a test that needs one should download it when it runs.

## License

GPL-3.0-or-later, see [LICENSE](LICENSE). The fonts in `fonts/` carry their own licenses.
