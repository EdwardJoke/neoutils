# neoutils

<p align="center">
  <img src="https://img.shields.io/badge/edition-2024-blue" alt="Rust edition 2024">
  <img src="https://img.shields.io/badge/license-Apache%202.0-green" alt="Apache 2.0">
  <img src="https://img.shields.io/badge/nightly-toolchain-orange" alt="Nightly toolchain">
  <img src="https://img.shields.io/badge/built%20for-agents-purple" alt="Built for agents">
  <img src="https://img.shields.io/badge/crates-2-important" alt="2 crates">
</p>

**neoutils — modern Rust CLI utilities, built for you and your agent.**

A workspace of drop-in replacements for everyday Unix commands. Safer, faster, and machine-parseable by design. Each tool is standalone, installable via `cargo install`, and shares the same design philosophy: progress bars, JSON output, sensible defaults, and zero surprises.

---

## Crates

| Crate | Description | Install |
|-------|-------------|---------|
| [cup](./cup) | Faster `cp` with progress bars, `.env` detection, and JSON output | `cargo install cupcli` |
| [ram](./ram) | Safer `rm` — trash-by-default with restore, filters, and auto-clean | `cargo install ramcli` |

---

## Design

Every `neoutils` tool follows the same principles:

- **Progress bars** — visual feedback by default, suppress with `-q`
- **JSON mode** — `--json` for agent pipelines and programmatic use
- **Safety-first** — confirmation prompts, same-file protection, root guards
- **Structured output** — `[OK]`/`[ERR]`/`[SKIP]` status codes with `-v`

---

## Development

Requires nightly Rust (edition 2024).

```bash
git clone https://github.com/EdwardJoke/neoutils
cd neoutils
# Build all crates
cargo build
# Test all crates
cargo test --workspace
# Lint all crates
cargo clippy --release --all-targets --workspace -- -D warnings
# Format
cargo fmt
```

---

## License

Apache 2.0
