use clap::Parser;
use regex::Regex;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_DATA: &str = ".nav";
const MAX_SCORE: u64 = 9000;

#[derive(Parser)]
#[command(
    name = "nav",
    about = "jump around - tracks your most-used directories by frecency",
    version
)]
struct Cli {
    #[arg(
        long = "add",
        help = "Add a directory to the database (used by shell hook)"
    )]
    add: Option<String>,

    #[arg(long = "init", help = "Print shell integration script")]
    init: bool,

    #[arg(short = 'c', help = "Restrict matches to subdirs of current directory")]
    restrict: bool,

    #[arg(short = 'e', help = "Echo best match, don't cd")]
    echo: bool,

    #[arg(short = 'l', help = "List matches by frecency")]
    list: bool,

    #[arg(short = 'r', help = "Match by rank only")]
    rank: bool,

    #[arg(short = 't', help = "Match by recent access only")]
    recent: bool,

    #[arg(short = 'x', help = "Remove current directory from database")]
    remove: bool,

    #[arg(trailing_var_arg = true, hide = true)]
    patterns: Vec<String>,
}

fn data_path() -> String {
    if let Ok(v) = env::var("NAV_DATA") {
        return v;
    }
    let home = env::var("HOME").expect("HOME not set");
    format!("{}/{}", home, DEFAULT_DATA)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn load_entries() -> Vec<(String, u64, u64)> {
    let path = data_path();
    let f = match fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let mut entries = Vec::new();
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 3 {
            continue;
        }
        let rank: u64 = match parts[1].parse() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let time: u64 = match parts[2].parse() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if Path::new(parts[0]).exists() {
            entries.push((parts[0].to_string(), rank, time));
        }
    }
    entries
}

fn save_entries(entries: &[(String, u64, u64)]) {
    let path = data_path();
    let tmp = format!("{}.tmp.{}", path, std::process::id());
    {
        let mut f = fs::File::create(&tmp).expect("cannot create data file");
        for (p, r, t) in entries {
            let _ = writeln!(f, "{}|{}|{}", p, r, t);
        }
    }
    fs::rename(&tmp, &path).expect("cannot rename data file");
}

fn frecency(rank: u64, age_secs: u64) -> u64 {
    let dx = age_secs as f64;
    (10000.0 * rank as f64 * (3.75 / (0.0001 * dx + 1.0 + 0.25))).round() as u64
}

fn cmd_add(target: &str) {
    let canonical: String = if env::var("NAV_NO_RESOLVE_SYMLINKS").is_err() {
        fs::canonicalize(target)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| target.to_string())
    } else {
        target.to_string()
    };

    let home = env::var("HOME").unwrap_or_default();
    if canonical == home || canonical == "/" {
        return;
    }

    if let Ok(excl) = env::var("NAV_EXCLUDE_DIRS") {
        for e in excl.split(':') {
            if canonical.starts_with(e) {
                return;
            }
        }
    }

    let mut entries = load_entries();
    let now = now_secs();
    let mut found = false;
    for e in &mut entries {
        if e.0 == canonical {
            e.1 += 1;
            e.2 = now;
            found = true;
            break;
        }
    }
    if !found {
        entries.push((canonical, 1, now));
    }

    let total: u64 = entries.iter().map(|e| e.1).sum();
    if total > MAX_SCORE {
        for e in &mut entries {
            e.1 = (e.1 as f64 * 0.99) as u64;
        }
        entries.retain(|e| e.1 >= 1);
    }

    save_entries(&entries);
}

fn cmd_remove_current() {
    let mut entries = load_entries();
    if let Ok(cwd) = env::current_dir() {
        let cwd_str = cwd.to_string_lossy().to_string();
        entries.retain(|e| e.0 != cwd_str);
        save_entries(&entries);
    }
}

fn common_prefix(matches: &[(&str, u64)]) -> Option<String> {
    if matches.len() <= 1 {
        return None;
    }
    let shortest = matches.iter().map(|m| m.0).min_by_key(|p| p.len())?;
    if shortest == "/" {
        return None;
    }
    if matches.iter().all(|m| m.0.starts_with(shortest)) {
        Some(shortest.to_string())
    } else {
        None
    }
}

fn cmd_query(
    restrict: bool,
    echo: bool,
    list: bool,
    rank: bool,
    recent: bool,
    patterns: &[String],
) {
    let entries = load_entries();
    if entries.is_empty() {
        return;
    }

    let mut list = list;
    let no_query = patterns.is_empty() && !restrict;
    if no_query {
        list = true;
    }

    let typ = match (rank, recent) {
        (true, _) => Some("rank"),
        (_, true) => Some("recent"),
        _ => None,
    };

    let now = now_secs();

    let query_str = if patterns.is_empty() {
        String::new()
    } else {
        patterns.join(" ")
    };

    let mut full_pattern = String::new();
    if restrict && let Ok(cwd) = env::current_dir() {
        let cwd_str = regex::escape(&cwd.to_string_lossy());
        full_pattern.push_str(&cwd_str);
        full_pattern.push_str(".*");
    }
    if !query_str.is_empty() {
        let q = query_str.replace(' ', ".*");
        full_pattern.push_str(&q);
    }

    let mut scored: Vec<(&str, u64)> = Vec::new();

    for entry in &entries {
        let path = entry.0.as_str();

        if !full_pattern.is_empty() {
            let re = match Regex::new(&full_pattern) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if !re.is_match(path) {
                let insensitive = format!("(?i){}", full_pattern);
                let re_i = match Regex::new(&insensitive) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if !re_i.is_match(path) {
                    continue;
                }
            }
        }

        let score = match typ {
            Some("rank") => entry.1,
            Some("recent") => entry.2,
            _ => frecency(entry.1, now.saturating_sub(entry.2)),
        };
        scored.push((path, score));
    }

    if scored.is_empty() {
        return;
    }

    scored.sort_by_key(|b| std::cmp::Reverse(b.1));

    if list {
        for m in &scored {
            println!("{:>10}  {}", m.1, m.0);
        }
        return;
    }

    if typ.is_none()
        && !echo
        && let Some(common) = common_prefix(&scored)
    {
        println!("{}", common);
        return;
    }

    if let Some(best) = scored.first() {
        println!("{}", best.0);
    }
}

fn print_init() {
    println!(
        r#"nav() {{
    case " $* " in
        *" --add "*|*" --init "*|*" -h "*|*" --help "*)
            command nav "$@"
            ;;
        *" -l "*|*" -x "*)
            command nav "$@"
            ;;
        *" -e "*)
            command nav "$@"
            ;;
        *)
            local target
            target="$(command nav "$@" 2>/dev/null)" && [ -n "$target" ] && builtin cd "$target"
            ;;
    esac
}}

if [ -n "$BASH_VERSION" ]; then
    if ! [[ "${{PROMPT_COMMAND}}" == *"nav --add"* ]]; then
        PROMPT_COMMAND='(nav --add "$(command pwd -P 2>/dev/null)" 2>/dev/null &)'$'\n'"${{PROMPT_COMMAND}}"
    fi
elif [ -n "$ZSH_VERSION" ]; then
    _nav_precmd() {{
        (nav --add "$(command pwd -P 2>/dev/null)" &>/dev/null &)
    }}
    precmd_functions+=('_nav_precmd')
fi
"#
    );
}

fn main() {
    let cli = Cli::parse();

    if cli.init {
        print_init();
        return;
    }

    if let Some(path) = cli.add {
        cmd_add(&path);
        return;
    }

    if cli.remove {
        cmd_remove_current();
        return;
    }

    cmd_query(
        cli.restrict,
        cli.echo,
        cli.list,
        cli.rank,
        cli.recent,
        &cli.patterns,
    );
}
