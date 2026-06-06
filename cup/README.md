# cup

<p align="center">
  <img src="https://img.shields.io/badge/edition-2024-blue" alt="Rust edition 2024">
  <img src="https://img.shields.io/badge/license-Apache%202.0-green" alt="Apache 2.0">
  <img src="https://img.shields.io/badge/nightly-toolchain-orange" alt="Nightly toolchain">
  <img src="https://img.shields.io/badge/built%20for-agents-purple" alt="Built for agents">
</p>

**Cup is the faster `cp` command — for You and Your Agent.**

`cup` copies files and directories with a real-time progress bar, configurable buffered I/O, and `.env` safety detection. Every copy produces structured output — human-friendly in a terminal, machine-parseable in a pipeline.

Why `cup`? It's `cp` wrapped in something smarter. Less waiting, fewer accidents.

---

## Quickstart

```bash
# Copy a single file with progress bar
cup src/main.rs dest/

# Copy an entire directory (auto-detected)
cup ./project ./backup/

# JSON output for agent pipelines
cup --json src/ dest/

# Skip overwrite/environment prompts
cup -f ./data ./archive/
```

---

## Features

### Progress bar by default

Every copy shows a live progress bar with per-file status. Suppress it with `--no-progress` or `-q` for quiet mode.

### .env safety detection

`cup` scans for `.env` and `.env.*` files before copying. If found, it prompts for confirmation — no accidental credential leaks.

```bash
cup ./project ./backup/
# [WARN] 2 .env files detected:
#   project/.env
#   project/.env.prod
# Copy .env file(s) too? [y/N]
```

Use `-f` to skip the prompt and proceed automatically.

### Buffered I/O

Configurable buffer size (default: 1 MB) for fast transfers. Tune it for your workload:

```bash
cup -B 4096 smallfile dest/    # 4 KB buffer for small files
cup -B 16777216 bigfile dest/  # 16 MB buffer for large files
```

### Structured status codes

```bash
cup -v src/file.txt dest/
[OK] file.txt

cup src/missing.txt dest/
[ERR] source 'src/missing.txt' does not exist
```

`[OK]` for success, `[ERR]` always shown for errors.

### JSON output (agent-native)

```bash
cup --json ./src/ ./dest/
# {"event":"start","total":42,"total_bytes":1048576,"source":"./src/","destination":"./dest/"}
# {"event":"done","target":"main.rs","status":"ok"}
# {"event":"summary","total":42,"success":42,"errors":0,"elapsed_ms":312,"total_bytes":1048576}
```

### Safety gates

| Flag | What it does |
|------|-------------|
| `-f, --force` | Skip overwrite and `.env` confirmation prompts |
| `-v, --verbose` | Per-entry `[OK]`/`[ERR]` status codes |
| `-q, --quiet` | Suppress progress and summary output |
| `-B, --buffer-size` | I/O buffer size in bytes (default: 1048576) |
| `--no-progress` | Hide progress bar (keep summary) |
| `--json` | Machine-parseable JSON output |

### Same-file protection

`cup` refuses to copy a file onto itself — no silent truncation, no data loss.

---

## Install

### From source

```bash
git clone https://github.com/EdwardJoke/neoutils
cd neoutils/cup
cargo install --path .
```

Requires nightly Rust (edition 2024).

### Cargo

```bash
cargo install cupcli
```

---

## Development

```bash
git clone https://github.com/EdwardJoke/neoutils
cd neoutils/cup
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
