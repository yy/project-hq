# hq

A CLI for tracking projects across multiple areas of work and life.

`hq` reads markdown files with YAML frontmatter from a directory of "tracks" (e.g., `research/`, `funding/`, `personal/`) and gives you a portfolio-level view of everything you're working on.

## Install

```bash
cargo install --path .
# or from GitHub:
cargo install --git https://github.com/yy/project-hq
```

## Quick start

1. Create a directory with track subdirectories:

```
my-projects/
  work/
    website-redesign.md
    api-migration.md
  personal/
    tax-filing.md
```

2. Each `.md` file has YAML frontmatter:

```yaml
---
title: "Website redesign"
track: work
status: active
my_next: finalize mockups
deadline: 2026-04-15
---

Freeform notes, context, links...
```

3. Run commands:

```bash
cd my-projects
hq summary        # counts by status per track
hq my-plate       # active projects (ball in your court)
hq waiting        # everything in waiting/submitted
hq stale          # waiting > 30 days
hq all            # everything grouped by status
hq undefer        # deferred projects ready to resume
```

## Configuration

Optionally create `hq.toml` in your data directory:

```toml
tracks = ["work", "personal", "side-projects"]
skip_files = ["notes.md", "template.md"]
stale_days = 14
```

Without a config file, `hq` auto-discovers tracks by scanning subdirectories for markdown files with frontmatter.

## Frontmatter fields

### Required
- `title` — project name
- `status` — `active`, `waiting`, `submitted`, `deferred`, `done`, `dropped`

### Optional
- `track` — inferred from directory name if omitted
- `owner` — who's responsible (omit if it's you)
- `priority` — integer, default 50
- `waiting_on` — who/what you're waiting on
- `waiting_since` — date (`YYYY-MM-DD`), used by `stale`
- `my_next` — your next concrete action
- `last` — most recent completed action
- `deadline` — date
- `deferred_until` — date, used by `undefer`

## Options

```
--dir <PATH>    Path to the data directory (default: current directory)
                Also settable via HQ_DIR environment variable
```

## License

MIT
