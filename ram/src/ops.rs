use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use std::{fs, io};

use globset::Glob;
use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// ── data types ───────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct TrashEntry {
    pub trash_name: String,
    pub original_path: PathBuf,
    pub size: u64,
    pub trashed_at: SystemTime,
}

/// ── .env detection ──────────────────────────────────────────────
pub fn detect_env_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for p in paths {
        if p.is_dir() {
            found.extend(scan_env_files(p));
        } else if is_env_file(p) {
            found.push(p.to_path_buf());
        }
    }
    found
}

fn scan_env_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if is_env_file(&path) {
                found.push(path.clone());
            }
            if path.is_dir() {
                found.extend(scan_env_files(&path));
            }
        }
    }
    found
}

fn is_env_file(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == ".env" || n.starts_with(".env."))
}

/// ── --match ─────────────────────────────────────────────────────
/// Walk roots and collect entries matching a glob pattern.
pub fn collect_matching(pattern: &str, roots: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
    let glob = Glob::new(pattern).map_err(|e| io::Error::other(format!("invalid glob: {}", e)))?;
    let matcher = glob.compile_matcher();
    let has_path_sep = pattern.contains('/');

    let mut entries = Vec::new();
    for root in roots {
        let abs = if root.is_relative() {
            std::env::current_dir()?.join(root)
        } else {
            root.clone()
        };
        walk_and_match(&abs, &matcher, has_path_sep, &mut entries);
    }
    entries.sort();
    entries.dedup();
    Ok(entries)
}

fn walk_and_match(
    dir: &Path,
    matcher: &globset::GlobMatcher,
    has_path_sep: bool,
    out: &mut Vec<PathBuf>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let is_dir = meta.is_dir();

        let matched = if has_path_sep {
            matcher.is_match(&path)
        } else {
            path.file_name().is_some_and(|n| matcher.is_match(n))
        };
        if matched {
            out.push(path.clone());
        }
        if is_dir {
            walk_and_match(&path, matcher, has_path_sep, out);
        }
    }
}

/// ── stdin paths ─────────────────────────────────────────────────
pub fn read_paths_from_stdin() -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            paths.push(PathBuf::from(trimmed));
        }
    }
    Ok(paths)
}

/// ── --older-than / --larger-than ────────────────────────────────
pub fn parse_duration(s: &str) -> io::Result<Duration> {
    let mut total = Duration::ZERO;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            if num.is_empty() {
                return Err(io::Error::other(format!("invalid duration: {}", s)));
            }
            let n: f64 = num
                .parse()
                .map_err(|_| io::Error::other(format!("invalid number in duration: {}", s)))?;
            let dur = match c {
                's' => Duration::from_secs_f64(n),
                'm' => Duration::from_secs_f64(n * 60.0),
                'h' => Duration::from_secs_f64(n * 3600.0),
                'd' => Duration::from_secs_f64(n * 86400.0),
                'w' => Duration::from_secs_f64(n * 604800.0),
                _ => {
                    return Err(io::Error::other(format!(
                        "unknown unit '{}' in duration",
                        c
                    )));
                }
            };
            total += dur;
            num.clear();
        }
    }
    if !num.is_empty() {
        let n: f64 = num
            .parse()
            .map_err(|_| io::Error::other(format!("invalid number in duration: {}", s)))?;
        total += Duration::from_secs_f64(n);
    }
    Ok(total)
}

pub fn parse_size(s: &str) -> io::Result<u64> {
    let clean = s.trim().to_uppercase();
    let (num_str, multiplier): (&str, u64) = if clean.ends_with("TB") {
        (&clean[..clean.len() - 2], 1u64 << 40)
    } else if clean.ends_with("GB") {
        (&clean[..clean.len() - 2], 1u64 << 30)
    } else if clean.ends_with("MB") {
        (&clean[..clean.len() - 2], 1u64 << 20)
    } else if clean.ends_with("KB") {
        (&clean[..clean.len() - 2], 1u64 << 10)
    } else if clean.ends_with('B') {
        (&clean[..clean.len() - 1], 1u64)
    } else {
        (clean.as_str(), 1u64)
    };
    let num_str = num_str.trim();
    if num_str.is_empty() {
        return Err(io::Error::other(format!("invalid size: {}", s)));
    }
    let n: f64 = num_str
        .parse()
        .map_err(|_| io::Error::other(format!("invalid number in size: {}", s)))?;
    Ok((n * multiplier as f64) as u64)
}

pub fn filter_by_age(entries: Vec<PathBuf>, older_than: Duration) -> Vec<PathBuf> {
    let cutoff = SystemTime::now()
        .checked_sub(older_than)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    entries
        .into_iter()
        .filter(|p| {
            p.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .is_some_and(|t| t < cutoff)
        })
        .collect()
}

pub fn filter_by_min_size(entries: Vec<PathBuf>, min_size: u64) -> Vec<PathBuf> {
    entries
        .into_iter()
        .filter(|p| p.metadata().map(|m| m.len() >= min_size).unwrap_or(false))
        .collect()
}

/// ── --preserve-root ─────────────────────────────────────────────
pub fn is_root(path: &Path) -> bool {
    if path == Path::new("/") {
        return true;
    }
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical == Path::new("/")
}

/// ── .gitignore collection ──────────────────────────────────────
pub fn collect_gitignored(paths: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = Vec::new();

    for target in paths {
        let abs = if target.is_relative() {
            std::env::current_dir()?.join(target)
        } else {
            target.clone()
        };

        let Some(gitignore) = build_gitignore(&abs)? else {
            continue;
        };

        let root = if abs.is_dir() {
            abs
        } else {
            abs.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        };

        walk_gitignored(&root, &gitignore, &mut entries);
    }

    entries.sort();
    entries.dedup();
    Ok(entries)
}

fn build_gitignore(target: &Path) -> io::Result<Option<Gitignore>> {
    let Some(gi_path) = find_gitignore_upwards(target) else {
        return Ok(None);
    };

    let root = gi_path.parent().unwrap_or(Path::new("."));
    let mut builder = GitignoreBuilder::new(root);
    if let Some(err) = builder.add(&gi_path) {
        return Err(io::Error::other(format!(
            "failed to read {}: {}",
            gi_path.display(),
            err
        )));
    }
    builder
        .build()
        .map(Some)
        .map_err(|e| io::Error::other(format!("failed to compile {}: {}", gi_path.display(), e)))
}

fn walk_gitignored(dir: &Path, gitignore: &Gitignore, out: &mut Vec<PathBuf>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();

        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }

        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let is_dir = meta.is_dir();

        if matches!(
            gitignore.matched_path_or_any_parents(&path, is_dir),
            Match::Ignore(_)
        ) {
            out.push(path);
            continue;
        }

        if is_dir {
            walk_gitignored(&path, gitignore, out);
        }
    }
}

fn find_gitignore_upwards(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_dir() {
        Some(start.to_path_buf())
    } else {
        start.parent().map(|p| p.to_path_buf())
    };

    while let Some(dir) = current {
        let candidate = dir.join(".gitignore");
        if candidate.is_file() {
            return Some(candidate);
        }
        current = dir.parent().map(|p| p.to_path_buf());
    }
    None
}

/// ── Trash / delete ops ─────────────────────────────────────────
fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

pub fn trash_dir() -> PathBuf {
    home_dir().join(".ram_trash")
}

fn ensure_trash_dir() -> io::Result<()> {
    let d = trash_dir();
    if !d.exists() {
        fs::create_dir_all(&d)?;
    }
    Ok(())
}

fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub fn trash(path: &Path) -> io::Result<()> {
    ensure_trash_dir()?;

    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        let cwd = std::env::current_dir().unwrap_or_default();
        if cwd.as_os_str().is_empty() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        }
    };

    let ts = timestamp_nanos();
    let name = path.file_name().unwrap_or_default();
    let trash_name = format!("{}_{}", ts, name.to_string_lossy());
    let dest = trash_dir().join(&trash_name);

    fs::rename(path, &dest).map_err(|e| {
        if e.kind() == io::ErrorKind::CrossesDevices {
            io::Error::other(format!(
                "cannot move across filesystems; use --delete for permanent removal: {}",
                e
            ))
        } else {
            e
        }
    })?;

    let meta = trash_dir().join(format!(".{}.meta", trash_name));
    let _ = fs::write(&meta, abs.to_string_lossy().as_bytes());

    Ok(())
}

pub fn permanent_delete(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// ── List / Restore ──────────────────────────────────────────────
pub fn list_trash() -> io::Result<Vec<TrashEntry>> {
    let dir = trash_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') {
            continue;
        }

        let meta = entry.metadata()?;
        let size = meta.len();
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        let ts = name
            .split('_')
            .next()
            .and_then(|s| s.parse::<u128>().ok())
            .unwrap_or(0);

        let trashed_at = SystemTime::UNIX_EPOCH
            .checked_add(std::time::Duration::from_nanos(ts as u64))
            .unwrap_or(mtime);

        let meta_path = dir.join(format!(".{}.meta", name));
        let original_path = if meta_path.is_file() {
            fs::read_to_string(&meta_path)
                .map(|s| PathBuf::from(s.trim()))
                .unwrap_or_else(|_| PathBuf::from(&name))
        } else {
            PathBuf::from(&name)
        };

        entries.push(TrashEntry {
            trash_name: name,
            original_path,
            size,
            trashed_at,
        });
    }

    entries.sort_by_key(|b| std::cmp::Reverse(b.trashed_at));
    Ok(entries)
}

pub fn restore(patterns: &[String]) -> io::Result<Vec<(TrashEntry, Result<(), String>)>> {
    let entries = list_trash()?;
    let mut results = Vec::new();

    for entry in &entries {
        let original_str = entry.original_path.to_string_lossy();

        let matches = patterns.is_empty()
            || patterns
                .iter()
                .any(|p| original_str.contains(p) || entry.trash_name.contains(p));

        if !matches {
            continue;
        }

        if let Some(parent) = entry.original_path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
            && let Err(e) = fs::create_dir_all(parent)
        {
            results.push((entry.clone(), Err(format!("mkdir: {}", e))));
            continue;
        }

        if entry.original_path.exists() {
            results.push((entry.clone(), Err("target exists, skipping".to_string())));
            continue;
        }

        let src = trash_dir().join(&entry.trash_name);
        match fs::rename(&src, &entry.original_path) {
            Ok(()) => {
                let meta = trash_dir().join(format!(".{}.meta", entry.trash_name));
                let _ = fs::remove_file(&meta);
                results.push((entry.clone(), Ok(())));
            }
            Err(e) => {
                results.push((entry.clone(), Err(e.to_string())));
            }
        }
    }

    Ok(results)
}

/// ── Config & Clean ───────────────────────────────────────────────
pub fn config_path() -> PathBuf {
    trash_dir().join("config.json")
}

pub struct RamConfig {
    pub max_size: Option<String>,
    pub max_age: Option<String>,
}

pub fn load_config() -> RamConfig {
    let path = config_path();
    if !path.exists() {
        return RamConfig {
            max_size: None,
            max_age: None,
        };
    }
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            return RamConfig {
                max_size: None,
                max_age: None,
            };
        }
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => {
            return RamConfig {
                max_size: None,
                max_age: None,
            };
        }
    };
    RamConfig {
        max_size: v.get("max_size").and_then(|v| v.as_str().map(String::from)),
        max_age: v.get("max_age").and_then(|v| v.as_str().map(String::from)),
    }
}

pub fn clean_trash(
    older_than: Option<Duration>,
    larger_than: Option<u64>,
    dry_run: bool,
) -> io::Result<(Vec<TrashEntry>, u64)> {
    let entries = list_trash()?;
    let mut to_remove: Vec<TrashEntry> = entries;

    if let Some(dur) = older_than {
        let cutoff = SystemTime::now()
            .checked_sub(dur)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        to_remove.retain(|e| e.trashed_at < cutoff);
    }

    if let Some(min_size) = larger_than {
        to_remove.retain(|e| e.size >= min_size);
    }

    let total_freed: u64 = to_remove.iter().map(|e| e.size).sum();

    if !dry_run {
        for entry in &to_remove {
            let path = trash_dir().join(&entry.trash_name);
            remove_entry(&path);
            let meta = trash_dir().join(format!(".{}.meta", entry.trash_name));
            let _ = fs::remove_file(&meta);
        }
    }

    Ok((to_remove, total_freed))
}

pub fn auto_clean() -> io::Result<Option<(usize, u64)>> {
    let config = load_config();
    if config.max_age.is_none() && config.max_size.is_none() {
        return Ok(None);
    }

    let mut entries = list_trash()?;
    let mut to_remove: Vec<TrashEntry> = Vec::new();

    if let Some(ref age_s) = config.max_age
        && let Ok(dur) = parse_duration(age_s)
    {
        let cutoff = SystemTime::now()
            .checked_sub(dur)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        entries.retain(|e| {
            if e.trashed_at < cutoff {
                to_remove.push(e.clone());
                false
            } else {
                true
            }
        });
    }

    if let Some(ref size_s) = config.max_size
        && let Ok(max_bytes) = parse_size(size_s)
    {
        let total: u64 = entries.iter().map(|e| e.size).sum();
        if total > max_bytes {
            entries.sort_by_key(|e| e.trashed_at);
            let mut running = total;
            entries.retain(|e| {
                if running > max_bytes {
                    running = running.saturating_sub(e.size);
                    to_remove.push(e.clone());
                    return false;
                }
                true
            });
        }
    }

    if to_remove.is_empty() {
        return Ok(None);
    }

    let total_freed: u64 = to_remove.iter().map(|e| e.size).sum();
    let removed_count = to_remove.len();

    for entry in &to_remove {
        let path = trash_dir().join(&entry.trash_name);
        remove_entry(&path);
        let meta = trash_dir().join(format!(".{}.meta", entry.trash_name));
        let _ = fs::remove_file(&meta);
    }

    Ok(Some((removed_count, total_freed)))
}

fn remove_entry(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

/// ── secure deletion ──────────────────────────────────────────────
fn overwrite_file(path: &Path) -> io::Result<()> {
    use std::io::Write;
    let len = fs::metadata(path)?.len();
    if len == 0 {
        return Ok(());
    }
    let mut f = fs::File::create(path)?;
    let buf = vec![0u8; 4096];
    let mut remaining = len;
    while remaining > 0 {
        let n = buf.len().min(remaining as usize);
        f.write_all(&buf[..n])?;
        remaining -= n as u64;
    }
    f.sync_all()?;
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

pub fn secure_delete(path: &Path) -> io::Result<()> {
    let mut files = Vec::new();
    if path.is_dir() {
        collect_files(path, &mut files)?;
    } else {
        files.push(path.to_path_buf());
    }

    for f in &files {
        overwrite_file(f)?;
    }

    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

// ── helpers ──────────────────────────────────────────────────────
pub fn format_size(bytes: u64) -> String {
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
