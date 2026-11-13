// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
use std::io::Read;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use clap::{Parser, Subcommand, ValueEnum};
use md5::{Digest, Md5};
use walkit::files::MemoryFiles;

use walkit_cli::compare::compare;
use walkit_cli::info;
use walkit_cli::input::{self, Source};
use walkit_cli::output;
use walkit_cli::params::Params;
use walkit_cli::shot::shoot;
use walkit_cli::skin::{self, Skin};

/// Command-line tool for Winamp skins, built on the walkit engine.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render one skin or many to PNG files.
    Render(Render),
    /// Print the default parameters, the Winamp Skin Museum's screenshot state, as a
    /// parameter file.
    Params,
    /// Compare two images pixel by pixel.
    Compare(Compare),
    /// Print a JSON Schema: of the parameter file, or of what `info --json` prints.
    Schema {
        #[arg(value_enum)]
        of: SchemaOf,
    },
    /// Print what is known about skins: who made them, what is in them, and what the
    /// engine makes of them.
    Info(Info),
}

#[derive(Clone, Copy, ValueEnum)]
enum SchemaOf {
    Params,
    Info,
}

#[derive(clap::Args)]
struct Info {
    /// Skins: .wsz or .wal files, http(s) addresses, or folders of skins.
    #[arg(required = true)]
    inputs: Vec<String>,
    /// Also search the folders under a folder.
    #[arg(short, long)]
    recursive: bool,
    /// Print JSON: an object for one skin, an array for more.
    #[arg(long)]
    json: bool,
    /// A classic skin (file or address) laid under every classic skin.
    #[arg(long, value_name = "SKIN")]
    base: Option<String>,
    /// A zip of Winamp's wasabi folder (file or address) laid under every modern skin.
    #[arg(long, value_name = "ZIP")]
    wasabi: Option<String>,
}

#[derive(clap::Args)]
struct Render {
    /// Skins: .wsz or .wal files, http(s) addresses, or folders of skins.
    inputs: Vec<String>,
    /// A file that names more inputs, one per line ("-" reads standard input).
    #[arg(short, long, value_name = "FILE")]
    list: Vec<String>,
    /// Also search the folders under a folder.
    #[arg(short, long)]
    recursive: bool,
    /// The file to write, for a single skin.
    #[arg(short, long, value_name = "FILE", conflicts_with = "out_dir")]
    output: Option<PathBuf>,
    /// The folder the previews go in.
    #[arg(short = 'd', long, value_name = "DIR")]
    out_dir: Option<PathBuf>,
    /// What a preview in the folder is named after: the skin's file name, or the md5 of its
    /// bytes as the museum names its screenshots.
    #[arg(long, value_enum, default_value_t = Naming::Stem)]
    name: Naming,
    /// The parameter file (JSON). Without one, the museum's screenshot state.
    #[arg(short, long, value_name = "FILE")]
    params: Option<PathBuf>,
    /// A classic skin (file or address) laid under every classic skin: it gives the bitmaps
    /// a skin leaves out, as Winamp's base skin does.
    #[arg(long, value_name = "SKIN")]
    base: Option<String>,
    /// A zip of Winamp's wasabi folder (file or address) laid under every modern skin.
    #[arg(long, value_name = "ZIP")]
    wasabi: Option<String>,
    /// A folder of TrueType fonts to draw with in place of the built-in stand-ins: arial.ttf,
    /// arialbd.ttf, tahoma.ttf and tahomabd.ttf, as Windows names them. Text matches Winamp
    /// on Windows exactly only with the real Arial.
    #[arg(long, value_name = "DIR")]
    fonts: Option<PathBuf>,
    /// How many skins render at once.
    #[arg(short, long, default_value_t = default_jobs())]
    jobs: usize,
    /// Print what each skin logs.
    #[arg(short, long)]
    verbose: bool,
    /// Print failures only.
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Naming {
    Stem,
    Md5,
}

#[derive(clap::Args)]
struct Compare {
    /// The image to measure.
    image: PathBuf,
    /// The image to measure it against.
    reference: PathBuf,
    /// How far a color channel may differ (0 to 255) for a pixel to count as close.
    #[arg(short, long, default_value_t = 8)]
    tolerance: u8,
    /// Write a picture of where they differ: close pixels in yellow, the others in red.
    #[arg(long, value_name = "FILE")]
    diff: Option<PathBuf>,
    /// Also report each band of this many rows (116 for the three classic windows).
    #[arg(long, value_name = "ROWS")]
    bands: Option<u32>,
    /// Fail when fewer than this percentage of the pixels are the same.
    #[arg(long, value_name = "PERCENT")]
    min_same: Option<f64>,
}

fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Render(args) => render(args),
        Command::Params => {
            print!("{}", Params::default().to_json());
            Ok(true)
        }
        Command::Compare(args) => run_compare(args),
        Command::Schema { of } => {
            print!(
                "{}",
                match of {
                    SchemaOf::Params => walkit_cli::schema::params(),
                    SchemaOf::Info => walkit_cli::schema::info(),
                }
            );
            Ok(true)
        }
        // skins nest deep: the engine recurses through their groups
        Command::Info(args) => std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || run_info(args))
            .expect("a thread")
            .join()
            .unwrap_or_else(|_| Err("the engine stopped".into())),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

/// One skin of a batch and the file its preview goes to; `None` names it after its md5.
struct Job {
    source: Source,
    path: Option<PathBuf>,
}

/// What every worker shares. The base skins are kept as bytes: unzipped files do not cross
/// threads, so each worker unzips its own.
struct Batch {
    jobs: Vec<Job>,
    next: AtomicUsize,
    params: Params,
    base: Option<Vec<u8>>,
    wasabi: Option<Vec<u8>>,
    fonts: Vec<(String, Vec<u8>)>,
    out_dir: PathBuf,
    verbose: bool,
    quiet: bool,
    failed: AtomicUsize,
    print: Mutex<()>,
}

/// Renders the skins; `Ok(false)` when any failed.
fn render(args: Render) -> Result<bool, String> {
    let params = match &args.params {
        Some(path) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            Params::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?
        }
        None => Params::default(),
    };
    let mut inputs = args.inputs.clone();
    for file in &args.list {
        let text = if file == "-" {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| e.to_string())?;
            text
        } else {
            std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?
        };
        inputs.extend(input::list(&text));
    }
    let sources = input::expand(&inputs, args.recursive)?;
    if sources.is_empty() {
        return Err("no skins to render".into());
    }
    if args.output.is_some() && sources.len() != 1 {
        return Err("--output takes a single skin; use --out-dir for more".into());
    }
    let base = under_skin("--base", &args.base)?;
    let wasabi = under_skin("--wasabi", &args.wasabi)?;
    let fonts = match &args.fonts {
        Some(dir) => read_fonts(dir)?,
        None => Vec::new(),
    };
    let out_dir = args.out_dir.clone().unwrap_or_else(|| PathBuf::from("."));
    // names are given out in input order, so a batch names its files the same on every run:
    // the second "Skin.wsz" becomes Skin-2.png
    let mut taken = HashSet::new();
    let jobs: Vec<Job> = sources
        .into_iter()
        .map(|source| {
            let path = match (&args.output, args.name) {
                (Some(path), _) => Some(path.clone()),
                (None, Naming::Md5) => None,
                (None, Naming::Stem) => {
                    let stem = file_name(&source.name());
                    let mut name = stem.clone();
                    let mut n = 1;
                    while !taken.insert(name.to_lowercase()) {
                        n += 1;
                        name = format!("{stem}-{n}");
                    }
                    Some(out_dir.join(format!("{name}.png")))
                }
            };
            Job { source, path }
        })
        .collect();
    let count = jobs.len();
    let batch = Arc::new(Batch {
        jobs,
        next: AtomicUsize::new(0),
        params,
        base,
        wasabi,
        fonts,
        out_dir,
        verbose: args.verbose,
        quiet: args.quiet,
        failed: AtomicUsize::new(0),
        print: Mutex::new(()),
    });
    // the engine's panics are reported as the skin's failure, not printed as they happen
    std::panic::set_hook(Box::new(|_| {}));
    let workers = args.jobs.clamp(1, count);
    let handles: Vec<_> = (0..workers)
        .map(|_| {
            let batch = batch.clone();
            std::thread::Builder::new()
                // skins nest deep: the engine recurses through their groups
                .stack_size(64 * 1024 * 1024)
                .spawn(move || work(&batch))
                .expect("a worker thread")
        })
        .collect();
    for handle in handles {
        let _ = handle.join();
    }
    let failed = batch.failed.load(Ordering::SeqCst);
    if count > 1 && !batch.quiet {
        println!("{} rendered, {failed} failed", count - failed);
    }
    Ok(failed == 0)
}

/// The bytes of a skin or zip laid under others, checked to unzip; `None` without one.
fn under_skin(what: &str, source: &Option<String>) -> Result<Option<Vec<u8>>, String> {
    let Some(source) = source else {
        return Ok(None);
    };
    let bytes = Source::parse(source)
        .read()
        .map_err(|e| format!("{what} {source}: {e}"))?;
    // a base that does not unzip fails here, once, rather than under every skin
    skin::unzip(&bytes).map_err(|e| format!("{what} {source}: {e}"))?;
    Ok(Some(bytes))
}

/// Prints what is known about the skins; `Ok(false)` when any could not be read.
fn run_info(args: Info) -> Result<bool, String> {
    let sources = input::expand(&args.inputs, args.recursive)?;
    if sources.is_empty() {
        return Err("no skins".into());
    }
    let unzip = |what: &str, source: &Option<String>| -> Result<Option<MemoryFiles>, String> {
        Ok(under_skin(what, source)?.and_then(|b| skin::unzip(&b).ok()))
    };
    let base = unzip("--base", &args.base)?;
    let wasabi = unzip("--wasabi", &args.wasabi)?;
    let params = Params::default();
    std::panic::set_hook(Box::new(|_| {}));
    let mut infos = Vec::new();
    let mut ok = true;
    for source in &sources {
        let result = source.read().and_then(|bytes| {
            let skin = Skin::open(&bytes, base.as_ref(), wasabi.as_ref())?;
            catch_unwind(AssertUnwindSafe(|| {
                info::inspect(&skin, &bytes, &source.name(), &params)
            }))
            .map_err(|_| "the engine stopped".to_string())
        });
        match result {
            Ok(info) if args.json => infos.push(info),
            Ok(info) => {
                if !infos.is_empty() || !ok {
                    println!();
                }
                println!("{source}\n{}", info.to_text());
                infos.push(info);
            }
            Err(message) => {
                ok = false;
                eprintln!("failed: {source}: {message}");
            }
        }
    }
    if args.json {
        let json = if sources.len() == 1 {
            infos.first().map(serde_json::to_string_pretty)
        } else {
            Some(serde_json::to_string_pretty(&infos))
        };
        if let Some(json) = json {
            println!("{}", json.map_err(|e| e.to_string())?);
        }
    }
    Ok(ok)
}

/// A skin's name as a file name: what a file system refuses is replaced.
fn file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "skin".to_string()
    } else {
        cleaned
    }
}

/// The fonts in `dir` that replace a stand-in, as `(file name, bytes)`.
fn read_fonts(dir: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("--fonts {}: {e}", dir.display()))?;
    let mut fonts = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if skin::font_path(&name).is_none() {
            continue;
        }
        let path = entry.path();
        let bytes = std::fs::read(&path).map_err(|e| format!("--fonts {}: {e}", path.display()))?;
        fonts.push((name, bytes));
    }
    if fonts.is_empty() {
        return Err(format!(
            "--fonts {}: no arial.ttf, arialbd.ttf, tahoma.ttf or tahomabd.ttf in it",
            dir.display()
        ));
    }
    fonts.sort();
    Ok(fonts)
}

/// Takes jobs until none are left.
fn work(batch: &Batch) {
    let unzip = |bytes: &Option<Vec<u8>>| bytes.as_ref().and_then(|b| skin::unzip(b).ok());
    let base = unzip(&batch.base);
    let wasabi = unzip(&batch.wasabi);
    loop {
        let index = batch.next.fetch_add(1, Ordering::SeqCst);
        let Some(job) = batch.jobs.get(index) else {
            return;
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            render_one(batch, job, base.as_ref(), wasabi.as_ref())
        }))
        .unwrap_or_else(|payload| {
            let text = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown".into());
            Err(format!("the engine stopped: {text}"))
        });
        let _lock = batch.print.lock();
        match result {
            Ok(done) => {
                for line in &done.warnings {
                    eprintln!("warning: {}: {line}", job.source);
                }
                if batch.verbose {
                    for line in &done.log {
                        eprintln!("{}: {line}", job.source);
                    }
                }
                if !batch.quiet {
                    println!("{}", done.summary);
                }
            }
            Err(message) => {
                batch.failed.fetch_add(1, Ordering::SeqCst);
                eprintln!("failed: {}: {message}", job.source);
            }
        }
    }
}

struct Done {
    summary: String,
    warnings: Vec<String>,
    log: Vec<String>,
}

fn render_one(
    batch: &Batch,
    job: &Job,
    base: Option<&MemoryFiles>,
    wasabi: Option<&MemoryFiles>,
) -> Result<Done, String> {
    let bytes = job.source.read()?;
    let skin = Skin::open(&bytes, base, wasabi)?.with_fonts(&batch.fonts);
    let mut warnings = Vec::new();
    let missing = skin.missing_sheets();
    if !missing.is_empty() {
        warnings.push(format!(
            "the skin has no {}; without --base those parts stay empty",
            missing.join(", ")
        ));
    }
    // a skin that draws nothing or one flat colour may lack what the wasabi folder or a
    // plugin would give it: the reason goes with the failure or the warning
    let mut reasons = Vec::new();
    let parts = skin.missing_wasabi_parts();
    if !parts.is_empty() {
        let mut named = parts.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        if parts.len() > 3 {
            named.push_str(&format!(" and {} more", parts.len() - 3));
        }
        reasons.push(format!(
            "the skin uses parts of Winamp's wasabi folder ({named}), which only --wasabi gives"
        ));
    }
    let plugins = skin.plugins();
    if !plugins.is_empty() {
        reasons.push(format!(
            "the skin is made for the {} plugin and takes its windows from the plugin's \
             files, which are not in the skin",
            plugins.join(" and ")
        ));
    }
    // a .wal or .wsz holds one skin, at its root or in one folder: several folders and none
    // at the root is a pack to unpack by hand, where a skin may take files from a sibling
    // folder that walkit does not read
    if let [read, others @ ..] = skin.folders.as_slice() {
        if !read.is_empty() && !others.is_empty() {
            let name = |f: &String| format!("\"{}\"", f.trim_end_matches('/'));
            let pack = format!(
                "the archive is not a single skin but a pack of {} skins to unpack by hand; only {} \
                 is read, without {}",
                others.len() + 1,
                name(read),
                others.iter().map(name).collect::<Vec<_>>().join(", ")
            );
            warnings.push(pack.clone());
            reasons.push(pack);
        }
    }
    let explain = |problem: String| {
        std::iter::once(problem)
            .chain(reasons.iter().cloned())
            .collect::<Vec<_>>()
            .join("; ")
    };
    let shot = shoot(&skin, &job.source.name(), &batch.params).map_err(explain)?;
    let (pixels, _) = shot.rgba.as_chunks::<4>();
    if pixels.iter().all(|p| p == &pixels[0]) {
        warnings.push(explain("the picture is a single colour".into()));
    }
    let path = match &job.path {
        Some(path) => path.clone(),
        None => batch.out_dir.join(format!("{:x}.png", Md5::digest(&bytes))),
    };
    let written = output::write(&shot, &path, &batch.params.output.thumbnails)?;
    let kind = skin.kind.name();
    let extra = match written.len() {
        1 => String::new(),
        n => format!(", {} thumbnails", n - 1),
    };
    Ok(Done {
        summary: format!(
            "{} -> {} ({kind}, {} by {}{extra})",
            job.source,
            path.display(),
            shot.width,
            shot.height
        ),
        warnings,
        log: shot.log,
    })
}

/// Compares two images; `Ok(false)` when they are less alike than `--min-same` asks.
fn run_compare(args: Compare) -> Result<bool, String> {
    let open = |path: &PathBuf| {
        image::open(path)
            .map(|i| i.to_rgba8())
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    let a = open(&args.image)?;
    let b = open(&args.reference)?;
    let (all, picture) = compare(&a, &b, args.tolerance, args.diff.is_some())?;
    let line = |label: &str, c: &walkit_cli::compare::Comparison| {
        println!(
            "{label}: {:.2}% the same, {:.2}% within {} ({} of {} pixels differ, largest difference {})",
            c.same_percent(),
            c.close_percent(),
            args.tolerance,
            c.pixels() - c.same,
            c.pixels(),
            c.largest
        );
    };
    line("all", &all);
    if let Some(rows) = args.bands.filter(|r| *r > 0) {
        let mut top = 0;
        while top < a.height() {
            let h = rows.min(a.height() - top);
            let crop = |i: &image::RgbaImage| {
                image::imageops::crop_imm(i, 0, top, i.width(), h).to_image()
            };
            let (band, _) = compare(&crop(&a), &crop(&b), args.tolerance, false)?;
            line(&format!("rows {top} to {}", top + h - 1), &band);
            top += h;
        }
    }
    if let (Some(path), Some(picture)) = (&args.diff, picture) {
        picture
            .save(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(args.min_same.is_none_or(|min| all.same_percent() >= min))
}
