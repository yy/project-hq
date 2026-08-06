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
  _tasks/
    todo.txt
    done.txt
  _routines/
    flush-water-heater.md
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
hq action reset lab/semester-prep.md  # reset completed checklist actions
hq new work --title "Prepare launch" --action-mode serial
hq check          # validate the current HQ directory
hq serve          # start web dashboard on http://localhost:3001
hq serve --port 8080  # custom port
```

## Web dashboard

`hq serve` starts a local dashboard with Projects, Tasks, Routines, and
Analysis views:

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
- See and directly edit the first available body checklist item as the project's
  next action
- Check off body checklist items from the project view
- Follow relative Markdown links between HQ projects without leaving the app;
  external links open outside HQ
- Use the persistent Agent panel to ask across HQ or make coordinated Markdown updates
- Capture, edit, defer, prioritize, and complete standalone todo.txt tasks
- Filter by track using the controls at the top
- Color-coded cards by track
- Daily time-axis Analysis view for project load, waiting stock, intake,
  submissions, and completions over time
- Separate Routines view with one chronological upcoming timeline
- Live reload when project, routine, or task files change on disk

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

The app always starts and owns a private `hq serve` child on an OS-assigned
loopback port, then terminates it when the app quits. A structured startup
handshake supplies the actual port. Each launch also generates an authentication
token required by the WebView, API requests, and event streams. Standalone
`hq serve` remains available and can run beside the app without a port conflict.

The app icon is bundled from `macos/Assets/AppIcon.icns`. To regenerate the
checked-in icon assets, run `script/make_icon.swift`.

### HQ agent

The persistent right-hand Agent panel runs the local Codex CLI as a coding
agent. In Projects, Tasks, Routines, and Analysis its context is the whole
repository; in project focus
its default context is the open project, which remains visible beside the
conversation. The agent can inspect and update projects, `_routines/`, and
`_tasks/`.

HQ copies the current data repository—including uncommitted files—into an
isolated temporary workspace. Valid project-file changes are applied
automatically when the turn completes, the project view refreshes immediately,
and the update can be reversed with Undo. The composer remains available while
the agent works: additional messages are shown as queued and run in order after
the current turn and its edits finish applying. Tool calls and file-operation
details stay out of the transcript; the header shows when Codex is working.
Clear removes finished messages from the visible transcript without resetting
the Agent context or interrupting active and queued work.
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

The Agent panel lists models and supported thinking levels reported by the
local Codex runtime. A selection applies to subsequent turns and is remembered
by HQ; authentication and billing still follow the local Codex login.
Conversations are ephemeral and are not stored in project Markdown.

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
- `my_next` — legacy next-action field; new work should use body checkboxes
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

## Standalone tasks

Small independent actions live one per line in `_tasks/todo.txt`. Completed
lines move to `_tasks/done.txt`; they are not projects and do not enter project
counts, columns, Analysis, or stale-project reports.

```text
2026-07-31 Call electrician @phone &electrician +house p:100 due:2026-08-15
2026-07-31 Buy filters +house p:50 t:2026-08-03
2026-07-31 Ask about rebate @email status:waiting
```

HQ follows todo.txt conventions for a creation date, `@context`, `+tag`, and
completed lines. It adds `&person`, numeric `p:` priority, `due:YYYY-MM-DD`,
`t:YYYY-MM-DD` deferral, and `status:waiting`. Higher priorities sort first;
dragging a task assigns a numeric priority between its neighbors. Tasks without
`p:` sort last.

The Tasks view supports capture, inline detail editing, completion, deferral,
available/deferred filtering, search, and drag reordering. Future `t:` dates
hide tasks from Available without changing their content. All mutations use
exact-line revision checks and support Undo. The Agent can also edit both task
files directly.

## Routines

Routines are independently recurring obligations stored as one Markdown file
each under the reserved `_routines/` directory. They do not enter project
counts, Kanban columns, Analysis, or stale-project reports.

```yaml
---
type: routine
title: Flush water heater
area: home
repeat: 1 year
repeat_from: completion
available_before: 1 month
next_due: 2027-07-30
last_completed: 2026-07-30
---

Vendor, manual, and cost notes.

## History

- 2026-07-30 — completed
```

`repeat` and `available_before` accept a number plus `day`, `week`, `month`, or
`year`. `repeat_from: completion` advances from the actual completion date.
`repeat_from: schedule` preserves the fixed cadence and advances to the first
future occurrence, so missed daily or weekly routines never create a backlog.

The Routines view provides:

- One availability timeline grouped into Now, Today, This week, This month,
  This season, This year, and Later.
- Now contains actionable occurrences; later horizons show when unavailable or
  deferred occurrences become available. Unavailable rows are heavily muted.
- **Complete:** records completion and advances the next occurrence.
- **Skip:** records a skip and advances without changing `last_completed`.
- **Defer:** hides only the current occurrence until a chosen time or date.

Click a compact routine row to edit its schedule and notes. Routine creation,
editing, completion, skipping, and deferral support Undo.

The same fixed header remains visible across all four views, including the HQ
title and recent project pulse figures. **+ New** opens the creation dialog for
the active view.

## Actions, contexts, and people

Markdown checklist items are actions. Three prefixes carry distinct meanings:

- `@phone`, `@home` — context
- `&alex`, `&electrician` — person or role
- `!serial`, `!parallel` — execution directive for a nested branch

Reset every completed action in one project while leaving its frontmatter,
status, and deferral unchanged:

```bash
hq action reset lab/semester-prep.md
```

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
values also appear in those results for compatibility. In the dashboard, the
first available body checkbox is the derived next action; `my_next` is used only
as a fallback. Editing that fallback in project focus migrates it into the body.

## Options

```
--dir <PATH>    Path to the data directory (default: current directory)
                Also settable via HQ_DIR environment variable
```

## License

MIT
