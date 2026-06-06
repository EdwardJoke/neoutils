use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fs, io, process};

use clap::Parser;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Parser, Debug)]
#[command(
    name = "cup",
    version,
    about = "Cup is a faster, securer cp",
    long_about = "cup copies files and directories with progress, .env detection, and LLM-friendly JSON output.

Directories are auto-detected — no flags needed.

Key features:
  - Single file & recursive directory copy (auto-detected)
  - Buffered I/O for speed (configurable via -B)
  - .env detection with confirmation prompt
  - --json: machine-parseable JSON output
  - -v / --verbose: per-entry [OK]/[ERR] status codes
  - --force: skip overwrite prompts"
)]
struct Args {
    source: PathBuf,
    destination: PathBuf,

    #[arg(short = 'f', long = "force", help = "Skip confirmation prompts")]
    force: bool,

    #[arg(
        short = 'v',
        long = "verbose",
        help = "Per-entry [OK]/[ERR] status codes"
    )]
    verbose: bool,

    #[arg(
        long,
        help = "Machine-parseable JSON output (events on stdout, warnings on stderr)"
    )]
    json: bool,

    #[arg(long, help = "Suppress progress bar")]
    no_progress: bool,

    #[arg(
        short = 'q',
        long = "quiet",
        help = "Suppress progress and summary output"
    )]
    quiet: bool,

    #[arg(
        short = 'B',
        long = "buffer-size",
        default_value = "1048576",
        help = "I/O buffer size in bytes"
    )]
    buffer_size: usize,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(&args) {
        if args.json {
            println!(
                "{}",
                serde_json::json!({"event":"error","message":e.to_string()})
            );
        } else {
            eprintln!("{} {}", "[ERR]".red().bold(), e);
        }
        process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let source = &args.source;
    let dest = &args.destination;

    if !source.exists() {
        return Err(format!("source '{}' does not exist", source.display()).into());
    }

    let metadata = source.metadata()?;
    let is_dir = metadata.is_dir();

    let files = if is_dir {
        collect_files(source)?
    } else {
        vec![(source.clone(), metadata.len())]
    };

    if files.is_empty() {
        return Err("no files to copy".into());
    }

    if is_same_file(source, dest) {
        return Err(format!(
            "'{}' and '{}' are the same file",
            source.display(),
            dest.display()
        )
        .into());
    }

    if !is_dir && dest.exists() && !dest.is_dir() && !args.force {
        return Err(format!(
            "destination '{}' already exists; use -f to overwrite",
            dest.display()
        )
        .into());
    }

    let total_bytes: u64 = files.iter().map(|(_, s)| s).sum();
    let total_files = files.len() as u64;

    let env_files = detect_env_files(&files);
    if !env_files.is_empty() {
        if args.force {
            if !args.quiet {
                eprintln!(
                    "{} {} .env files detected (--force, proceeding)",
                    "[WARN]".yellow().bold(),
                    env_files.len()
                );
            }
        } else if args.json {
            println!(
                "{}",
                serde_json::json!({"event":"warn","message":"env files detected","count":env_files.len()})
            );
        } else {
            eprintln!(
                "{} {} .env files detected:",
                "[WARN]".yellow().bold(),
                env_files.len()
            );
            for f in &env_files {
                eprintln!("  {}", f.display().to_string().yellow());
            }
            eprint!("Copy .env file(s) too? [y/N] ");
            io::stdout().flush().ok();
            let mut input = String::new();
            io::stdin().read_line(&mut input).ok();
            if !input.trim().eq_ignore_ascii_case("y") {
                eprintln!("{}", "[WARN] Aborted.".red());
                process::exit(0);
            }
        }
    }

    // ── JSON start event ────────────────────────────────────────────
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "event": "start",
                "total": total_files,
                "total_bytes": total_bytes,
                "source": source.to_string_lossy(),
                "destination": dest.to_string_lossy(),
            })
        );
    }

    // ── Progress bar setup ──────────────────────────────────────────
    let use_progress = !args.json && !args.quiet && !args.no_progress;
    let pb: Option<ProgressBar> = if use_progress {
        let bar = ProgressBar::new(total_bytes);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.white} [{bar:32.green}] {msg:.yellow}")
                .expect("invalid progress bar template")
                .progress_chars("=> "),
        );
        bar.enable_steady_tick(Duration::from_millis(80));
        Some(bar)
    } else {
        None
    };

    // ── Process each file ───────────────────────────────────────────
    let mut success = 0u64;
    let mut errors: Vec<(String, String)> = Vec::new();

    for (src_path, _) in &files {
        let relative = src_path.strip_prefix(source).unwrap_or(src_path);
        let dst_path = if is_dir {
            dest.join(relative)
        } else {
            if dest.exists() && dest.is_dir() {
                let name = src_path.file_name().unwrap_or_default();
                dest.join(name)
            } else {
                dest.clone()
            }
        };

        let display = relative.to_string_lossy().to_string();

        if let Some(bar) = &pb {
            bar.set_message(display.clone());
        }

        let outcome = match copy_with_progress(src_path, &dst_path, args.buffer_size, &pb) {
            Ok(()) => Outcome::Ok,
            Err(e) => Outcome::Err(e.to_string()),
        };

        match outcome {
            Outcome::Ok => {
                emit_entry_status(args, "ok", &display, None);
                success += 1;
            }
            Outcome::Err(e) => {
                emit_entry_status(args, "err", &display, Some(&e));
                errors.push((display.clone(), e));
            }
        }

        if let Some(bar) = &pb {
            bar.inc(1);
        }
    }

    // ── Progress done ───────────────────────────────────────────────
    if let Some(bar) = pb {
        bar.finish_and_clear();
    }

    // ── Summary output ─────────────────────────────────────────────
    let total = total_files;
    let elapsed = start.elapsed();

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "event": "summary",
                "total": total,
                "success": success,
                "errors": errors.len(),
                "elapsed_ms": elapsed.as_millis(),
                "total_bytes": total_bytes,
            })
        );
    } else if !args.quiet {
        if errors.is_empty() {
            eprintln!(
                "{} copied {} targets ({}) in {:.2}s",
                "[OK]".green().bold(),
                success,
                format_size(total_bytes),
                elapsed.as_secs_f64(),
            );
        } else {
            eprintln!(
                "{} copied {} / {} ({} errors) — {}",
                "[OK]".green().bold(),
                success,
                total,
                errors.len(),
                "done".red()
            );
        }
    }

    if !errors.is_empty() {
        process::exit(1);
    }

    Ok(())
}

// ── Copy implementation ──────────────────────────────────────────

fn copy_with_progress(
    src: &PathBuf,
    dst: &PathBuf,
    buffer_size: usize,
    pb: &Option<ProgressBar>,
) -> io::Result<()> {
    let mut src_file = fs::File::open(src)?;
    let metadata = src_file.metadata()?;

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut dst_file = fs::File::create(dst)?;
    let mut buffer = vec![0u8; buffer_size];
    let mut total: u64 = 0;

    loop {
        let n = src_file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        dst_file.write_all(&buffer[..n])?;
        total += n as u64;
        if let Some(bar) = pb {
            bar.set_position(total);
        }
    }

    fs::set_permissions(dst, metadata.permissions())?;
    Ok(())
}

// ── File collection ──────────────────────────────────────────────

fn collect_files(path: &Path) -> io::Result<Vec<(PathBuf, u64)>> {
    let mut files = Vec::new();
    let mut dirs = vec![path.to_path_buf()];
    let mut i = 0;

    while i < dirs.len() {
        let mut entries: Vec<_> = fs::read_dir(&dirs[i])?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let ft = entry.file_type()?;
            let path = entry.path();
            if ft.is_dir() {
                dirs.push(path);
            } else if ft.is_file() {
                files.push((path, entry.metadata()?.len()));
            }
        }
        i += 1;
    }

    Ok(files)
}

// ── .env detection ──────────────────────────────────────────────

fn detect_env_files(files: &[(PathBuf, u64)]) -> Vec<PathBuf> {
    files
        .iter()
        .map(|(p, _)| p)
        .filter(|p| is_env_file(p))
        .cloned()
        .collect()
}

fn is_env_file(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == ".env" || n.starts_with(".env."))
}

fn is_same_file(a: &Path, b: &Path) -> bool {
    let a_canon = a.canonicalize();
    let b_canon = b.canonicalize();
    match (a_canon, b_canon) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

// ── Entry status output ─────────────────────────────────────────

enum Outcome {
    Ok,
    Err(String),
}

fn emit_entry_status(args: &Args, status: &str, target: &str, message: Option<&str>) {
    if args.json {
        let mut obj = serde_json::json!({
            "event": "done",
            "target": target,
            "status": status,
        });
        if let Some(msg) = message {
            obj.as_object_mut()
                .unwrap()
                .insert("message".into(), msg.into());
        }
        println!("{}", obj);
    } else if args.verbose || status == "err" {
        match status {
            "ok" => eprintln!("{} {}", "[OK]".green(), target),
            "skip" => {
                let msg = message.unwrap_or("skipped");
                eprintln!("{} {}: {}", "[SKIP]".yellow(), target, msg)
            }
            "err" => {
                let msg = message.unwrap_or("unknown error");
                eprintln!("{} {}: {}", "[ERR]".red().bold(), target, msg)
            }
            _ => {}
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}
