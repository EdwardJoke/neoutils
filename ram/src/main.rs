use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use std::{io, process};

use clap::Parser;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};

mod ops;

#[derive(Parser, Debug)]
#[command(
    name = "ram",
    version,
    about = "Ram is a safer rm",
    long_about = "ram moves targets to ~/.ram_trash (undo-friendly) instead of deleting.

Key features:
  - -r / --remove: trash files/folders to ~/.ram_trash (default)
  - -d / --delete: permanently delete files/folders (with confirmation)
  --list: browse trash contents
  --restore <pattern>: recover files from trash
  --match <glob>: remove files matching a glob pattern
  --older-than / --larger-than: age and size filters
  --remove-ignore: remove all files matched by .gitignore
  root is always protected
  --secure: overwrite with zeroes before permanent delete
  --json: machine-parseable JSON output
  --verbose: per-entry [OK]/[ERR]/[SKIP] status codes
  --clean: purge old trash entries (by age or size)
  --config: show current auto-clean configuration
  - Progress bar, dry-run, interactive modes for safety"
)]
struct Args {
    /// Files or directories to remove
    paths: Vec<PathBuf>,

    /// List trashed files
    #[arg(long)]
    list: bool,

    /// Restore files from trash (by pattern match; omit to restore all)
    #[arg(long, num_args = 0.., default_value = None, value_name = "PATTERN")]
    restore: Option<Vec<String>>,

    /// Remove files matching a glob pattern
    #[arg(long, value_name = "GLOB")]
    r#match: Option<String>,

    /// Only remove files older than this duration (e.g. 30d, 12h, 7d12h)
    #[arg(long, value_name = "DURATION")]
    older_than: Option<String>,

    /// Only remove files larger than this size (e.g. 10MB, 1.5GB, 500KB)
    #[arg(long, value_name = "SIZE")]
    larger_than: Option<String>,

    /// Remove all files/folders matched by .gitignore
    #[arg(long = "remove-ignore")]
    remove_ignore: bool,

    /// Permanently delete instead of moving to trash
    #[arg(short = 'd', long = "delete")]
    delete: bool,

    /// Overwrite with zeroes before permanent delete
    #[arg(long)]
    secure: bool,

    /// Skip confirmation prompts
    #[arg(short = 'f', long = "force")]
    force: bool,

    /// Remove files/folders to .ram_trash (trash)
    #[arg(short = 'r', long = "remove")]
    remove: bool,

    /// Show what would happen without actually doing it
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,

    /// Quiet mode — suppress progress and summary output
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Confirm each target before processing
    #[arg(short = 'i', long = "interactive")]
    interactive: bool,

    /// Machine-parseable JSON output (events on stdout, warnings on stderr)
    #[arg(long)]
    json: bool,

    /// Show [OK]/[ERR]/[SKIP] per entry (human mode)
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Purge old trash entries (by age or size)
    #[arg(long)]
    clean: bool,

    /// Show auto-clean configuration
    #[arg(long)]
    config: bool,
}

fn main() {
    let args = Args::parse();

    // ── list / restore dispatch ────────────────────────────────────
    if args.list {
        if args.json {
            return cmd_list_json(&args);
        }
        return cmd_list(&args);
    }
    if let Some(patterns) = &args.restore {
        if args.json {
            return cmd_restore_json(patterns);
        }
        return cmd_restore(patterns);
    }

    // ── config / clean dispatch ──────────────────────────────────
    if args.config {
        return cmd_config(&args);
    }

    if args.clean {
        return cmd_clean(&args);
    }

    // ── Collect targets ────────────────────────────────────────────
    let mut targets = collect_targets(&args);

    // ── Apply filters ──────────────────────────────────────────────
    if let Some(ref dur_s) = args.older_than {
        let dur = match ops::parse_duration(dur_s) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{} {}", "[ERR]".red().bold(), e);
                process::exit(1);
            }
        };
        targets = ops::filter_by_age(targets, dur);
    }

    if let Some(ref size_s) = args.larger_than {
        let min = match ops::parse_size(size_s) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{} {}", "[ERR]".red().bold(), e);
                process::exit(1);
            }
        };
        targets = ops::filter_by_min_size(targets, min);
    }

    // ── Safety checks ──────────────────────────────────────────────
    for t in &targets {
        if ops::is_root(t) {
            eprintln!(
                "{} refusing to remove '/' — root is protected",
                "[ERR]".red().bold()
            );
            process::exit(1);
        }
    }

    if targets.is_empty() {
        if args.json {
            println!(
                "{}",
                serde_json::json!({"event":"summary","total":0,"success":0,"errors":0,"skipped":0})
            );
        } else {
            eprintln!("{} no targets to remove", "[WARN]".yellow().bold());
        }
        process::exit(0);
    }

    // ── .env detection ──────────────────────────────────────────────
    let env_files = ops::detect_env_files(&targets);
    if !env_files.is_empty() {
        if args.force {
            eprintln!(
                "{} {} .env files detected (--force, proceeding)",
                "[WARN]".yellow().bold(),
                env_files.len()
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
            eprint!("Continue? [y/N] ");
            io::stdout().flush().ok();
            let mut input = String::new();
            io::stdin().read_line(&mut input).ok();
            if !input.trim().eq_ignore_ascii_case("y") {
                eprintln!("{}", "[WARN] Aborted.".red());
                process::exit(0);
            }
        }
    }

    // ── Delete confirmation ─────────────────────────────────────────
    if args.delete && !args.force && !args.dry_run && !args.json {
        eprintln!(
            "{} This will permanently delete {} item(s). Continue? [y/N]",
            "[WARN]".yellow().bold(),
            targets.len()
        );
        eprint!("> ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        if !input.trim().eq_ignore_ascii_case("y") {
            eprintln!("{}", "[WARN] Aborted.".red());
            process::exit(0);
        }
    }

    // ── Progress bar setup ──────────────────────────────────────────
    let total = targets.len() as u64;
    let mode_label = if args.delete { "delete" } else { "remove" };

    let use_progress = !args.json && !args.quiet;
    let pb: Option<ProgressBar> = if use_progress {
        let bar = ProgressBar::new(total);
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

    // ── JSON start event ────────────────────────────────────────────
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "event": "start",
                "total": total,
                "mode": mode_label,
                "dry_run": args.dry_run,
            })
        );
    }

    // ── Process each target ─────────────────────────────────────────
    let mut success = 0u64;
    let mut skipped = 0u64;
    let mut errors: Vec<(String, String)> = Vec::new();

    for target in &targets {
        let display = target.display().to_string();

        if let Some(ref bar) = pb {
            bar.set_message(display.clone());
        }

        if args.interactive {
            let action = if args.delete { "DELETE" } else { "REMOVE" };
            eprint!("{} {}? [y/N] ", action, display);
            io::stdout().flush().ok();
            let mut input = String::new();
            io::stdin().read_line(&mut input).ok();
            if !input.trim().eq_ignore_ascii_case("y") {
                emit_entry_status(
                    &args,
                    "skip",
                    &display,
                    Some("declined in interactive mode"),
                );
                skipped += 1;
                if let Some(ref bar) = pb {
                    bar.inc(1);
                }
                continue;
            }
        }

        let outcome = if args.dry_run {
            Outcome::Skip
        } else if args.delete && args.secure {
            outcome_from(ops::secure_delete(target))
        } else if args.delete {
            outcome_from(ops::permanent_delete(target))
        } else {
            outcome_from(ops::trash(target))
        };

        match outcome {
            Outcome::Ok => {
                emit_entry_status(&args, "ok", &display, None);
                success += 1;
            }
            Outcome::Skip => {
                emit_entry_status(&args, "skip", &display, Some("dry-run"));
                skipped += 1;
            }
            Outcome::Err(e) => {
                emit_entry_status(&args, "err", &display, Some(&e));
                errors.push((display.clone(), e));
            }
        }

        if let Some(ref bar) = pb {
            bar.inc(1);
        }
    }

    if let Some(bar) = pb {
        bar.finish_and_clear();
    }

    // ── Summary output ─────────────────────────────────────────────
    let total_processed = targets.len();
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "event": "summary",
                "total": total_processed,
                "success": success,
                "errors": errors.len(),
                "skipped": skipped,
            })
        );
    } else if !args.quiet {
        if errors.is_empty() && skipped == 0 {
            eprintln!(
                "{} {}d {} targets",
                "[OK]".green().bold(),
                mode_label,
                success
            );
        } else {
            let mut parts = vec![format!("{}", success)];
            if !errors.is_empty() {
                parts.push(format!("{} errors", errors.len()));
            }
            if skipped > 0 {
                parts.push(format!("{} skipped", skipped));
            }
            eprintln!(
                "{} {}d {} / {} — {}",
                "[OK]".green().bold(),
                mode_label,
                parts.join(", "),
                total_processed,
                if errors.is_empty() {
                    "done".green()
                } else {
                    "done".red()
                }
            );
        }
    }

    // ── Auto-clean ─────────────────────────────────────────────────
    if !args.delete && !args.dry_run && errors.is_empty() {
        match ops::auto_clean() {
            Ok(Some((removed, freed))) => {
                if args.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "event": "auto-clean",
                            "removed": removed,
                            "freed": freed,
                            "freed_human": ops::format_size(freed),
                        })
                    );
                } else if !args.quiet {
                    eprintln!(
                        "{} auto-cleaned {} entries ({} freed)",
                        "[INFO]".cyan(),
                        removed,
                        ops::format_size(freed)
                    );
                }
            }
            Ok(None) => {}
            Err(e) => {
                if !args.quiet {
                    eprintln!("{} auto-clean: {}", "[WARN]".yellow().bold(), e);
                }
            }
        }
    }

    if !errors.is_empty() {
        process::exit(1);
    }
}

enum Outcome {
    Ok,
    Skip,
    Err(String),
}

fn outcome_from(r: io::Result<()>) -> Outcome {
    match r {
        Ok(()) => Outcome::Ok,
        Err(e) => Outcome::Err(e.to_string()),
    }
}

/// Emit per-entry status code — JSON event or human [OK]/[ERR]/[SKIP]
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

/// ── Target collection pipeline ──────────────────────────────────
fn collect_targets(args: &Args) -> Vec<PathBuf> {
    // --match
    if let Some(ref pattern) = args.r#match {
        let roots = if args.paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            args.paths.clone()
        };
        return match ops::collect_matching(pattern, &roots) {
            Ok(list) => list,
            Err(e) => {
                eprintln!("{} {}", "[ERR]".red().bold(), e);
                process::exit(1);
            }
        };
    }

    // --remove-ignore
    if args.remove_ignore {
        let mut roots = args.paths.clone();
        if roots.is_empty() {
            roots.push(PathBuf::from("."));
        }
        let list = match ops::collect_gitignored(&roots) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("{} {}", "[ERR]".red().bold(), e);
                process::exit(1);
            }
        };
        return list;
    }

    // stdin (any path is exactly "-")
    if args.paths.iter().any(|p| p == &PathBuf::from("-")) {
        return match ops::read_paths_from_stdin() {
            Ok(list) => list,
            Err(e) => {
                eprintln!("{} {}", "[ERR]".red().bold(), e);
                process::exit(1);
            }
        };
    }

    args.paths.clone()
}

// ── List command ─────────────────────────────────────────────────
fn cmd_list(_args: &Args) {
    let entries = match ops::list_trash() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{} {}", "[ERR]".red().bold(), e);
            process::exit(1);
        }
    };

    if entries.is_empty() {
        eprintln!("{} trash is empty", "[INFO]".cyan());
        return;
    }

    let total_size: u64 = entries.iter().map(|e| e.size).sum();

    eprintln!(
        "{} {} entries, {} total",
        "[INFO]".cyan(),
        entries.len(),
        ops::format_size(total_size).cyan()
    );
    eprintln!();

    for (i, entry) in entries.iter().enumerate() {
        let ts = format_timestamp(entry.trashed_at);
        let size = ops::format_size(entry.size);
        let path = entry.original_path.display();

        eprintln!(
            " {:>3}. {}  {:>8}  {}",
            i + 1,
            ts.dimmed(),
            size.dimmed(),
            path.to_string().yellow()
        );
    }
}

fn cmd_list_json(_args: &Args) {
    let entries = match ops::list_trash() {
        Ok(e) => e,
        Err(e) => {
            println!(
                "{}",
                serde_json::json!({"event":"error","message":e.to_string()})
            );
            process::exit(1);
        }
    };

    let total_size: u64 = entries.iter().map(|e| e.size).sum();
    let json_entries: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            let ts = e
                .trashed_at
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            serde_json::json!({
                "trash_name": e.trash_name,
                "original_path": e.original_path.to_string_lossy(),
                "size": e.size,
                "trashed_at": ts,
            })
        })
        .collect();

    println!(
        "{}",
        serde_json::json!({
            "event": "list",
            "count": entries.len(),
            "total_size": total_size,
            "entries": json_entries,
        })
    );
}

// ── Restore command ──────────────────────────────────────────────
fn cmd_restore(patterns: &[String]) {
    let results = match ops::restore(patterns) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{} {}", "[ERR]".red().bold(), e);
            process::exit(1);
        }
    };

    let mut ok = 0u64;
    let mut errs: Vec<(String, String)> = Vec::new();

    for (entry, result) in &results {
        let path = entry.original_path.display();
        match result {
            Ok(()) => {
                eprintln!("{} {}", "[OK]".green(), path);
                ok += 1;
            }
            Err(msg) => {
                eprintln!("{} {}: {}", "[ERR]".red().bold(), path, msg);
                errs.push((entry.trash_name.clone(), msg.clone()));
            }
        }
    }

    if !errs.is_empty() {
        process::exit(1);
    } else if ok == 0 {
        eprintln!("{} no matching entries in trash", "[WARN]".yellow().bold());
    }
}

fn cmd_restore_json(patterns: &[String]) {
    let results = match ops::restore(patterns) {
        Ok(r) => r,
        Err(e) => {
            println!(
                "{}",
                serde_json::json!({"event":"error","message":e.to_string()})
            );
            process::exit(1);
        }
    };

    let total = results.len();
    println!(
        "{}",
        serde_json::json!({"event":"restore-start","total":total})
    );

    let mut ok = 0u64;
    let mut errs = 0u64;

    for (entry, result) in &results {
        let path = entry.original_path.to_string_lossy();
        match result {
            Ok(()) => {
                println!(
                    "{}",
                    serde_json::json!({"event":"restore-done","target":path,"status":"ok"})
                );
                ok += 1;
            }
            Err(msg) => {
                println!(
                    "{}",
                    serde_json::json!({"event":"restore-done","target":path,"status":"error","message":msg})
                );
                errs += 1;
            }
        }
    }

    println!(
        "{}",
        serde_json::json!({"event":"restore-summary","total":total,"success":ok,"errors":errs})
    );

    if errs > 0 {
        process::exit(1);
    } else if ok == 0 {
        println!(
            "{}",
            serde_json::json!({"event":"restore-summary","total":0,"success":0,"errors":0})
        );
    }
}

// ── Config command ───────────────────────────────────────────────
fn cmd_config(args: &Args) {
    let config = ops::load_config();
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "max_size": config.max_size,
                "max_age": config.max_age,
            })
        );
    } else {
        eprintln!("{}", "[INFO] Configuration".cyan());
        eprintln!(
            "  max_size: {}",
            config.max_size.as_deref().unwrap_or("(not set)")
        );
        eprintln!(
            "  max_age:  {}",
            config.max_age.as_deref().unwrap_or("(not set)")
        );
        eprintln!();
        eprintln!(
            "  Config file: {}",
            ops::config_path().display().to_string().dimmed()
        );
    }
}

// ── Clean command ────────────────────────────────────────────────
fn cmd_clean(args: &Args) {
    let older_than = match &args.older_than {
        Some(s) => match ops::parse_duration(s) {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("{} {}", "[ERR]".red().bold(), e);
                process::exit(1);
            }
        },
        None => None,
    };
    let larger_than = match &args.larger_than {
        Some(s) => match ops::parse_size(s) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("{} {}", "[ERR]".red().bold(), e);
                process::exit(1);
            }
        },
        None => None,
    };

    // First pass: list candidates (always dry-run)
    let (candidates, estimated_freed) = match ops::clean_trash(older_than, larger_than, true) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{} {}", "[ERR]".red().bold(), e);
            process::exit(1);
        }
    };

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "event": "clean-start",
                "older_than": args.older_than,
                "larger_than": args.larger_than,
                "dry_run": args.dry_run,
                "candidates": candidates.len(),
            })
        );
    }

    if candidates.is_empty() {
        if args.json {
            println!(
                "{}",
                serde_json::json!({"event":"clean-summary","total":0,"freed":0})
            );
        } else if !args.quiet {
            eprintln!("{} nothing to clean", "[INFO]".cyan());
        }
        return;
    }

    // Confirmation (skip for dry-run, force, or json)
    if !args.dry_run && !args.force && !args.json {
        eprintln!(
            "{} This will permanently remove {} trash entries ({}). Continue? [y/N]",
            "[WARN]".yellow().bold(),
            candidates.len(),
            ops::format_size(estimated_freed)
        );
        eprint!("> ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        if !input.trim().eq_ignore_ascii_case("y") {
            eprintln!("{}", "[WARN] Aborted.".red());
            process::exit(0);
        }
    }

    // Actually perform the clean
    let (removed, freed) = if args.dry_run {
        (candidates, estimated_freed)
    } else {
        match ops::clean_trash(older_than, larger_than, false) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{} {}", "[ERR]".red().bold(), e);
                process::exit(1);
            }
        }
    };

    // Per-entry output
    for entry in &removed {
        let path = entry.original_path.to_string_lossy();
        if args.json {
            println!(
                "{}",
                serde_json::json!({
                    "event": "clean-done",
                    "target": path,
                    "size": entry.size,
                    "trash_name": entry.trash_name,
                })
            );
        } else if args.verbose {
            eprintln!(
                "{} {} ({} freed)",
                "[OK]".green(),
                path,
                ops::format_size(entry.size)
            );
        }
    }

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "event": "clean-summary",
                "total": removed.len(),
                "freed": freed,
                "freed_human": ops::format_size(freed),
                "dry_run": args.dry_run,
            })
        );
    } else if !args.quiet {
        let mode = if args.dry_run { "(dry-run) " } else { "" };
        eprintln!(
            "{} {}cleaned {} entries ({} freed)",
            "[OK]".green().bold(),
            mode,
            removed.len(),
            ops::format_size(freed)
        );
    }
}

fn format_timestamp(t: SystemTime) -> String {
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let ts = dur.as_secs();

    let hours = (ts % 86400) / 3600;
    let mins = (ts % 3600) / 60;
    let s = ts % 60;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let age_secs = now.saturating_sub(ts);
    let age_days = age_secs / 86400;

    if age_days < 1 {
        format!("{:02}:{:02}:{:02}", hours, mins, s)
    } else if age_days < 7 {
        format!("{}d {:02}:{:02}", age_days, hours, mins)
    } else {
        let y = ts / 31557600 + 1970;
        let remaining = ts % 31557600;
        let mo = remaining / 2629800 + 1;
        let d = (remaining % 2629800) / 86400 + 1;
        format!("{:04}-{:02}-{:02}", y, mo, d)
    }
}
