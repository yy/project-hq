# hq

A Markdown-backed project manager with a CLI, web dashboard, and native macOS
wrapper.

`hq` reads Markdown files with YAML frontmatter from a directory of tracks such
as `research/`, `funding/`, and `personal/`. The same files drive the CLI and
app.

## Install

```bash
cargo install --path .
# or from GitHub:
cargo install --git https://github.com/yy/project-hq
```

## Quick start

Run `init` to create a starter directory:

```bash
hq --dir ~/Documents/HQ init
cd ~/Documents/HQ
hq serve
```

To create the files yourself, make a directory with track subdirectories:

```
my-projects/
  work/
    website-redesign.md
    api-migration.md
  personal/
    tax-filing.md
```

Each `.md` file has YAML frontmatter:

```yaml
---
title: "Website redesign"
track: work
status: active
my_next: finalize mockups
deadline: 2026-04-15
action_mode: parallel
---

Freeform notes, context, links...

- [ ] Call the designer @phone
- [ ] Review the mockups @computer
```

Run commands:

```bash
cd my-projects
hq summary        # counts by status per track
hq my-plate       # my-plate projects (ball in your court)
hq waiting        # everything in waiting/submitted
hq stale          # waiting > 30 days
hq all            # everything grouped by status
hq context phone  # available actions in @phone context (`hq tag` is an alias)
hq person alex    # available actions involving &alex
hq new work --title "Prepare launch" --action-mode serial
hq check          # validate the current HQ directory
hq serve          # start web dashboard on http://localhost:3001
hq serve --port 8080  # custom port
```

## Web dashboard

`hq serve` starts a local web server with a kanban board UI:

- My Plate, Active, Waiting, and Done columns, with Submitted folded into
  Waiting and Dropped folded into Done
- Drag cards between columns to change status
- Drag cards within a column to reorder by priority
- Use each card's status menu to choose an exact status such as Submitted or
  Dropped
- Defer a card for one hour, this evening, tomorrow, next week, next month,
  next year, or a custom date and time
- Undo project changes from the header or with Command-Z
- Click a card to open a focused project view with editable metadata and rendered markdown notes
- Move or defer the open project from the focused-view header; Edit details and
  Open in Obsidian remain available under More
- Check off body checklist items from the project view
- Use the persistent Agent panel to ask across HQ or make coordinated Markdown updates
- Filter by track using the controls at the top
- Color-coded cards by track
- Analysis view for project load, intake, submissions, and completions over time
- Live reload when `.md` files change on disk

## macOS app

The repo includes a lightweight native macOS wrapper around the web
dashboard. It bundles the Rust `hq` server into `dist/HQ.app`.

The app resolves its data directory in this order:

1. `HQ_DIR`, when set.
2. The directory saved through `HQ -> Settings`.
3. The directory bundled into the app.
4. `~/git/hq`.

Development builds bundle `HQ_DIR` or `~/git/hq`. Distribution builds use
`~/Documents/HQ` and show a first-run screen that can create a starter HQ
folder or open an existing one. The Settings directory picker validates a
folder with `hq check`, saves the choice when `HQ_DIR` is not active, and
reloads the local server.

### Build

```bash
./script/build_and_run.sh            # build and launch dist/HQ.app
./script/build_and_run.sh --verify   # build, launch, and confirm the server is up
./script/build_and_run.sh --logs     # launch and stream app logs
./script/build_and_run.sh --dist     # build dist/HQ.zip for first-run setup
HQ_DIR=/path/to/hq ./script/build_and_run.sh
```

### Install to /Applications

After building, copy the bundle into `/Applications` so it shows up in
Spotlight and Launchpad:

```bash
# first install
cp -R dist/HQ.app /Applications/

# upgrade in place (works even if the old bundle has restrictive perms)
pkill -x HQ; rsync -a --delete dist/HQ.app/ /Applications/HQ.app/
killall Dock                         # refresh the icon cache
```

### How it works

The app starts `hq serve --port 3001` when it opens and terminates that
child server when it quits. If another server is already listening on the
configured port, the app attaches to it and leaves that external process
alone — so running `hq serve` from a terminal and launching the app
side-by-side is fine.

The app icon is bundled from `macos/Assets/AppIcon.icns`. To regenerate the
checked-in icon assets, run `script/make_icon.swift`.

### HQ agent

The persistent right-hand Agent panel runs the local Codex CLI as a coding
agent. On the Kanban board its context is the whole repository; in project focus
its default context is the open project, which remains visible beside the
conversation. The agent can still inspect and update the whole portfolio.

HQ copies the current data repository—including uncommitted files—into an
isolated temporary workspace. Valid project-file changes are applied
automatically when the turn completes, the project view refreshes immediately,
and the update can be reversed with Undo. The composer remains available while
the agent works: additional messages are shown as queued and run in order after
the current turn and its edits finish applying. Tool calls and file-operation
details stay out of the transcript; the header shows when Codex is working.
Every modified file has a revision check that refuses to overwrite a newer live
edit, and created files must not already exist.

Install and sign in to the Codex CLI before using the panel:

```bash
npm install -g @openai/codex
codex login
```

Set `HQ_CODEX_BIN` when the executable is not at a standard Homebrew location.
HQ launches `codex app-server` and inherits the local Codex authentication and
configuration. A ChatGPT login uses the Codex allowance attached to that
ChatGPT plan or workspace. API-key login uses metered API billing. See the
[Codex authentication](https://learn.chatgpt.com/docs/auth) and
[Codex pricing](https://learn.chatgpt.com/docs/pricing) documentation.

The app does not yet expose provider, model, or reasoning-effort selectors.
Change the Codex CLI configuration outside HQ when needed. Conversations are
ephemeral and are not stored in project Markdown.

### Undo

The dashboard keeps a bounded in-memory history of project changes made during
the current app session. **Undo** and Command-Z restore status moves, priority
changes, deferrals, checklist toggles, body edits, newly created projects, and
applied Agent updates. An undo is refused if the affected file changed again
outside the recorded HQ action, so it cannot silently overwrite a newer edit.

## Configuration

Optionally create `hq.toml` in your data directory:

```toml
tracks = ["work", "personal", "side-projects"]
skip_tracks = ["archive"]
skip_files = ["notes.md", "template.md"]
stale_days = 14
statuses = ["my-plate", "active", "waiting", "submitted", "done", "dropped"]
default_owner = "yy"
pulse_tracks = ["work"]
```

The dashboard folds `submitted` projects into Waiting and `dropped` projects
into Done while retaining their underlying statuses for card labels, reports,
and history. Dragging a card into either shared column uses the primary status
(`waiting` or `done`); use the visible status pill to choose `submitted` or
`dropped`.

`pulse_tracks` controls which tracks receive a dedicated submission series in
the Analysis view. Without a config file, `hq` auto-discovers tracks by scanning
subdirectories for Markdown files with frontmatter. It skips hidden
directories, `_`-prefixed directories, common build directories, and any
configured `skip_tracks`.

## Frontmatter fields

### Required
- `title` — project name
- `status` — one of the configured statuses (default: `my-plate`, `active`, `waiting`, `submitted`, `done`, `dropped`)

### Optional
- `track` — inferred from directory name if omitted
- `owner` — who's responsible (omit if it's you)
- `priority` — number, default 50; fractional values support drag reordering
- `waiting_on` — who/what you're waiting on
- `waiting_since` — date (`YYYY-MM-DD`), used by `stale`
- `my_next` — your next concrete action
- `last` — most recent completed action
- `deadline` — date
- `deferred_until` — date or RFC 3339 timestamp; hides the project until then without changing its status
- `action_mode` — checklist availability: `serial` or `parallel` (default)

## Deferral

`deferred_until` controls visibility rather than status. A future date or
timestamp hides the project from dashboard columns and CLI reporting views.
The project reappears when the value is reached and remains in its original
status. The dashboard's Defer menu writes this field. You can also edit or clear
it in the focused project's metadata.

Action-level deferral is not implemented.

## Actions, contexts, and people

Markdown checklist items are actions. Three prefixes carry distinct meanings:

- `@phone`, `@home` — context
- `&alex`, `&electrician` — person or role
- `!serial`, `!parallel` — execution directive for a nested branch

Use `hq context <name>` or `hq person <name>` to show matching available
actions. `hq tag <name>` remains an alias for `hq context <name>`.

```markdown
- [ ] Call &electrician @phone
- [ ] Ask &alex about the draft @email
```

Availability follows the project's `action_mode`:

- `serial` — only the first incomplete checklist branch is available
- `parallel` — every incomplete checklist item is available

Nested list branches can override the project mode with `!serial` or
`!parallel`. A checkbox with incomplete children acts as a task group: its
children become available first, and the group itself becomes available after
its children are complete.

```markdown
- Calls !serial
  - [ ] Call &electrician @phone
  - [ ] Call &plumber @phone
- Shopping !parallel
  - [ ] Buy furnace filters @errand
  - [ ] Buy batteries @errand
```

Visible projects in `active` or `my-plate` expose available actions. Actions in
waiting, submitted, done, dropped, or deferred projects remain in the file
but do not appear in context or person results. Existing annotated `my_next`
values also appear in those results for compatibility.

## Options

```
--dir <PATH>    Path to the data directory (default: current directory)
                Also settable via HQ_DIR environment variable
```

## License

MIT
