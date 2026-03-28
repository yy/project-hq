# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build                # debug build
cargo run -- <subcommand>  # run without installing
cargo install --path .     # install binary as `hq`
```

No tests or linter configured yet. Run `cargo check` and `cargo clippy` for static analysis.

## Architecture

Rust CLI built with clap (derive). Binary name: `hq`.

- **`src/main.rs`** — CLI definition (clap `Parser`/`Subcommand`), project loading, and all `cmd_*` display functions. Each subcommand (`my-plate`, `waiting`, `stale`, `summary`, `all`, `undefer`) is a standalone function that filters/formats the loaded project list.
- **`src/project.rs`** — `Project` struct and a hand-rolled YAML frontmatter parser (`parse_frontmatter`). Does not use a YAML library; splits on `---` and parses `key: value` lines into a `BTreeMap<String, String>`.
- **`src/config.rs`** — `Config` loaded from optional `hq.toml` (via serde/toml crate). Falls back to auto-discovering tracks by scanning subdirectories for `.md` files with frontmatter. Skips dirs starting with `.` or `_`, plus a hardcoded skip list.

## Data Model

`hq` operates on a directory of "tracks" (subdirectories like `research/`, `funding/`). Each `.md` file in a track is a project with YAML frontmatter containing `title`, `status`, and optional fields (`priority`, `waiting_on`, `waiting_since`, `my_next`, `deadline`, `deferred_until`, etc.). Status values: `active`, `waiting`, `submitted`, `deferred`, `done`, `dropped`.

## Key Design Decisions

- Frontmatter parser is intentionally simple (no YAML crate) — just `key: value` pairs, no nested structures.
- Track auto-discovery checks for at least one `.md` file starting with `---` in a subdirectory.
- Default data directory is `.` (current working directory), overridable via `--dir` flag or `HQ_DIR` env var.
