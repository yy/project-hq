# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build                # debug build
cargo run -- <subcommand>  # run without installing
cargo install --path .     # install binary as `hq`
```

Run `cargo check` and `cargo clippy` for static analysis. Run `cargo test` for tests (integration tests live in `tests/`).

For the macOS app wrapper, use `./script/build_and_run.sh` (add `--verify` or `--logs` as needed). Set `HQ_DIR` to point at a non-default data repo.

## Architecture

Rust CLI built with clap (derive). Binary name: `hq`.

- **`src/main.rs`** — CLI definition (clap `Parser`/`Subcommand`) and dispatch.
- **`src/lib.rs`** — `load_all` function shared by CLI and web.
- **`src/commands.rs`** — Render functions for each CLI subcommand (`render_my_plate`, `render_summary`, etc.).
- **`src/project.rs`** — `Project` struct and deserialization from frontmatter fields.
- **`src/frontmatter.rs`** — Hand-rolled YAML frontmatter parser. Just `key: value` pairs, no nested structures.
- **`src/project_file.rs`** — `ProjectDocument` struct for reading/writing project `.md` files (frontmatter + body). Path validation, body editing, frontmatter rewriting.
- **`src/mover.rs`** — `move_project` (change status/priority) and `reorder_projects` (batch priority rewrite).
- **`src/config.rs`** — `Config` loaded from optional `hq.toml` (via serde/toml crate). Falls back to auto-discovering tracks by scanning subdirectories for `.md` files with frontmatter. Skips dirs starting with `.` or `_`, plus a hardcoded skip list.
- **`src/web.rs`** — Axum web server (`hq serve`). Serves a kanban board SPA from `static/index.html`. REST API for projects, move, reorder, body read/write. SSE live reload via file watcher (notify crate).
- **`static/index.html`** — Single-file kanban board frontend (vanilla JS, no build step). Drag-and-drop between status columns, track filters, side panel with markdown preview/edit.
- **`macos/`** + **`script/build_and_run.sh`** — Native macOS wrapper (Swift) around the web dashboard. Builds `dist/HQ.app`, which launches `hq serve --port 3001` as a child process scoped to `~/git/hq` (override via `HQ_DIR`). If the port is already in use, the app attaches to the existing server instead of spawning its own.
- **`tests/`** — Integration tests for the frontmatter parser, BOM handling, and `static/index.html`.
- **`docs/`** — In-repo Obsidian vault for design notes, roadmap, and specs. See `docs/README.md`.

## Data Model

`hq` operates on a directory of "tracks" (subdirectories like `research/`, `funding/`). Each `.md` file in a track is a project with YAML frontmatter containing `title`, `status`, and optional fields (`priority`, `waiting_on`, `waiting_since`, `my_next`, `deadline`, `deferred_until`, etc.). Status values: `active`, `waiting`, `submitted`, `deferred`, `done`, `dropped`.

## Key Design Decisions

- Frontmatter parser is intentionally simple (no YAML crate) — just `key: value` pairs, no nested structures.
- Track auto-discovery checks for at least one `.md` file starting with `---` in a subdirectory.
- Default data directory is `.` (current working directory), overridable via `--dir` flag or `HQ_DIR` env var.
