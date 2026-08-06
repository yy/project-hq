# CLAUDE.md

`hq`: Markdown project tracker — Rust CLI (clap) + axum web dashboard + macOS
app wrapper. Design notes, roadmap, specs: `docs/` (in-repo Obsidian vault).

## Workflow

- Handoff check: `cargo fmt --check && cargo test && cargo clippy --all-targets -- -D warnings && git diff --check`
- After **any** change, rebuild and relaunch — the user tests in the running
  app, not a dev server: `cargo install --path . && ./script/build_and_run.sh --verify`
- Real data lives at `~/git/hq`; point `--dir`/`HQ_DIR` elsewhere for testing.

## Non-obvious

- `static/index.html` is the entire frontend: vanilla JS, single file, no build step.
- `src/frontmatter.rs` is intentionally minimal (`key: value` only) — don't add a YAML crate or nesting.
- Deferral is visibility, not status: a future `deferred_until` hides a project unchanged.
- The dashboard folds submitted→waiting and dropped→done but keeps the underlying status.
- Checklist syntax: `@phone` context, `&alex` person, `!serial`/`!parallel` nested override; `action_mode` sets the project default (parallel).
- Standalone tasks use todo.txt-style lines in `_tasks/todo.txt`; completed lines move to `_tasks/done.txt`. Numeric `p:` sorts higher first and supports drag ordering.
- Routines live in `_routines/` and are excluded from project counts and aging.
- `my_next` is a legacy field; keep parsing it.
- macOS app data-dir resolution: `HQ_DIR`, then saved Settings dir, then bundled default; it owns an authenticated child server on an OS-assigned loopback port.
