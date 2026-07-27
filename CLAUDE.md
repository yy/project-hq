# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build                # debug build
cargo run -- <subcommand>  # run without installing
cargo install --path .     # install binary as `hq`
```

Run `cargo check` and `cargo clippy` for static analysis. Run `cargo test` for tests (integration tests live in `tests/`).

Before handing off a change, run:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
```

For the macOS app wrapper, use `./script/build_and_run.sh` (add `--verify`,
`--logs`, or `--dist` as needed). Set `HQ_DIR` to point at a non-default data
repo. After changing the app or bundled server, rebuild, replace
`/Applications/HQ.app`, relaunch it, and inspect the running UI.

## Architecture

Rust CLI built with clap (derive). Binary name: `hq`.

- **`src/main.rs`** — CLI definition (clap `Parser`/`Subcommand`) and dispatch.
- **`src/lib.rs`** — `load_all` function shared by CLI and web.
- **`src/commands.rs`** — CLI reports, project creation, directory validation,
  and starter content for `hq init`.
- **`src/project.rs`** — `Project` struct and deserialization from frontmatter fields.
- **`src/action.rs`** — Checklist parsing, context and person annotations,
  serial/parallel branch availability, and nested `!mode` directives.
- **`src/frontmatter.rs`** — Hand-rolled YAML frontmatter parser. Just `key: value` pairs, no nested structures.
- **`src/project_file.rs`** — Safe project path resolution, frontmatter and body
  writes, creation, checklist toggles, and revision-checked replacement.
- **`src/mover.rs`** — Status, priority, deferral, metadata, and batch reorder
  mutations.
- **`src/config.rs`** — `Config` loaded from optional `hq.toml` (via serde/toml crate). Falls back to auto-discovering tracks by scanning subdirectories for `.md` files with frontmatter. Skips dirs starting with `.` or `_`, plus a hardcoded skip list.
- **`src/undo.rs`** — Bounded, revision-safe in-memory undo history for
  dashboard and Agent mutations.
- **`src/agent.rs`** — Local Codex app-server session, isolated repository
  snapshots, event streaming, change validation, and multi-file application.
- **`src/timeline.rs`** — Git-history snapshots and Analysis view series.
- **`src/web.rs`** — Axum server and REST/SSE API for projects, metadata,
  deferral, checklists, timeline data, Agent turns, and Undo.
- **`static/index.html`** — Single-file frontend with Kanban, Analysis, focused
  project editing with primary Move and Defer menus, creation, card actions,
  Undo, and the persistent Agent panel with serial message queuing and a
  tool-call-free transcript. Vanilla JavaScript; no frontend build step.
- **`macos/`** + **`script/build_and_run.sh`** — Swift wrapper and app bundle
  builder. The app can create or select an HQ directory, saves the selection in
  Settings, launches the bundled server, and attaches to an existing server on
  initial launch when the configured port is occupied.
- **`tests/`** — Integration tests for the frontmatter parser, BOM handling, and `static/index.html`.
- **`docs/`** — In-repo Obsidian vault for design notes, roadmap, and specs. See `docs/README.md`.

## Data Model

`hq` operates on a directory of tracks such as `research/`, `funding/`, and
`personal/`. Each Markdown file in a track is a project with `title`, `status`,
and optional fields such as `priority`, `waiting_on`, `my_next`, `deadline`,
`deferred_until`, and `action_mode`.

Default statuses are `my-plate`, `active`, `waiting`, `submitted`, `done`, and
`dropped`. The dashboard folds Submitted into Waiting and Dropped into Done but
retains the underlying status. Deferral is a visibility rule: a future
`deferred_until` hides the project without changing its status.

Checklist items are actions. `@phone` marks a context, `&alex` marks a person or
role, and `!serial` or `!parallel` overrides availability for a nested branch.
Project-level `action_mode` accepts `serial` or `parallel`; parallel is the
default.

## Key Design Decisions

- Frontmatter parser is intentionally simple (no YAML crate) — just `key: value` pairs, no nested structures.
- Track auto-discovery checks for at least one `.md` file starting with `---` in a subdirectory.
- Default data directory is `.` (current working directory), overridable via `--dir` flag or `HQ_DIR` env var.
- The macOS app resolves `HQ_DIR`, then its saved Settings directory, then the
  bundled default.
- Agent turns edit an isolated snapshot and apply supported project-file
  changes automatically with revision checks. Undo records the resulting
  multi-file operation.
