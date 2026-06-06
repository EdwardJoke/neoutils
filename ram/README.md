# ram

<p align="center">
  <img src="https://img.shields.io/badge/edition-2024-blue" alt="Rust edition 2024">
  <img src="https://img.shields.io/badge/license-Apache%202.0-green" alt="Apache 2.0">
  <img src="https://img.shields.io/badge/nightly-toolchain-orange" alt="Nightly toolchain">
  <img src="https://img.shields.io/badge/built%20for-agents-purple" alt="Built for agents">
</p>

**Ram is the safer `rm` command — for You and Your Agent.**

`ram` moves files and directories to `~/.ram_trash` instead of deleting them permanently. Every trashed item preserves its original path via a sidecar `.meta` file, making recovery a single command away.

Why `ram`? It's `rm` spelled backwards. Undo-friendly from the ground up.

---

## Quickstart

```bash
# Safe removal — trash to ~/.ram_trash
ramcli -r file.txt

# Permanent delete (with confirmation prompt)
ramcli -d file.txt

# Secure delete — overwrite with zeroes before permanent removal
ramcli -d --secure secret.pdf

# Restore from trash
ramcli --restore temp

# Browse trash
ramcli --list
```

---

## Features

### Trash-by-default

Files and directories land in `~/.ram_trash` with a nanosecond-precision timestamp prefix. A companion `.meta` file stores the original absolute path for reliable restore. Both `-r` and `-d` handle directories automatically — no separate `--recursive` flag needed.

### Listing & Restore

```bash
ramcli --list              # browse trash (index, time, size, original path)
ramcli --restore           # restore everything
ramcli --restore pattern   # restore matching entries (by original path or trash name)
```

### Permanent delete

```bash
ramcli -d ./temp.log           # with confirmation prompt
ramcli -d --force ./temp.log   # skip confirmation
ramcli -d --secure ./secret    # overwrite with zeroes before deletion
```

`-d` always prompts for confirmation unless `--force` is given. Add `--secure` to overwrite file contents with zeroes before permanent removal.

### Safety gates

| Flag | What it does |
|------|-------------|
| `-r, --remove` | Remove (trash) targets to `.ram_trash` (default) |
| `-d, --delete` | Permanently delete targets (with confirmation) |
| `--secure` | Overwrite with zeroes before permanent delete |
| `-n, --dry-run` | Preview without side effects |
| `-f, --force` | Skip confirmation prompts |
| `-i, --interactive` | Confirm each target |
| `-q, --quiet` | Suppress progress and summary |
| `--remove-ignore` | Remove files matched by `.gitignore` |

### Selective removal

```bash
ramcli --match "*.log"                            # glob match (basename if no `/`)
ramcli --match "src/**/*.tmp"                     # glob match (full path if contains `/`)
ramcli --older-than 30d ./logs/                  # by age: s/m/h/d/w
ramcli --larger-than 100MB ./data/               # by size: B/KB/MB/GB/TB
ramcli --older-than 7d --larger-than 1GB .       # combined
ramcli --remove-ignore                           # everything git ignores
```

### Stdin

```bash
find . -name "*.tmp" | ramcli -r -
```

### JSON output (agent-native)

```bash
ramcli --json -n -r src/
# {"dry_run":true,"event":"start","mode":"remove","total":3}
# {"event":"done","message":"dry-run","status":"skip","target":"src/tmp.log"}
# {"event":"summary","errors":0,"skipped":3,"success":0,"total":3}
```

Works with `--clean`, `--list`, `--restore`, and `--config`.

### Trash lifecycle

```bash
ramcli --clean                        # purge all (interactive confirmation)
ramcli --clean -f                     # purge all (skip confirmation)
ramcli --clean --older-than 30d       # purge entries older than 30 days
ramcli --clean --larger-than 1GB      # purge entries larger than 1 GB
ramcli --clean -n                     # dry-run: preview what would be purged
ramcli --config                       # view auto-clean configuration
```

### Auto-clean (config file)

Create `~/.ram_trash/config.json`:

```json
{"max_size":"1GB", "max_age":"30d"}
```

- `max_size`: when total trash exceeds this, oldest entries are auto-removed after each `ram` operation.
- `max_age`: entries older than this are auto-removed after each `ram` operation.
- Run `ramcli --config` to inspect the active configuration.

### Structured status codes

```
ramcli -v -n file.log
[SKIP] file.log: dry-run

ramcli -v file.log
[OK] file.log
```

`[OK]`/`[ERR]`/`[SKIP]` prefixes in verbose mode; `[ERR]` always shown for errors. Summary uses `[OK]` for success. Root (`/`) is always protected — attempting to remove it is refused unconditionally.

---

## Install

### From source

```bash
git clone https://github.com/EdwardJoke/ram
cd ram
cargo install --path .
```

Requires nightly Rust (edition 2024, version 1.98).

### Cargo

```bash
cargo install ramcli
```

---

## Development

```bash
git clone https://github.com/EdwardJoke/ram
cd ram
# Pre-commit hooks (clippy + fmt)
pre-commit install
# Build
cargo build
# Test
cargo test
# Lint
cargo clippy --release --all-targets -- -D warnings
# Format
cargo fmt
```

---

## License

Apache 2.0
