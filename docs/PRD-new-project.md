# PRD: Create New Project

## Problem

`hq` has no way to create a new project. Users must hand-create a `.md` file in the right track directory with valid frontmatter (`title`, `status`, etc.) and a filename matching the `<owner>-<slug>.md` convention. Every other lifecycle operation (move, reorder, edit body) is supported by both the CLI and the web UI; creation is the missing primitive.

## Goal

Add "create new project" as a first-class operation in both CLI and web. Keep the surface area minimal: filename, track, status, title. Everything else can be edited after creation.

## Filename Convention

All existing projects follow `<owner>-<slug>.md` (e.g., `yy-nsf-sos.md`, `byunghwee-persuasion-theory.md`). Owner is the lead person — `yy` for self, a collaborator's first name otherwise. New projects must follow this pattern.

- **Owner**: lowercase, `^[a-z0-9-]+$`. Defaults to `yy` (configurable in `hq.toml` as `default_owner`).
- **Slug**: lowercase, hyphen-separated, `^[a-z0-9-]+$`. Derived from the title but editable before save.
- **Validation**: reject empty, leading/trailing hyphens, double hyphens.
- **Collision**: if `{owner}-{slug}.md` exists in *any* track, append `-2`, `-3`, … and surface the final filename in the response. Cross-track check matters because projects move between tracks (active → archive).

Owner is filename-only for now; not duplicated in frontmatter.

## Functional Requirements

### FR-1: CLI `hq new`

```
hq new <track> --title "Title" [--owner <owner>] [--slug <slug>]
               [--status <status>] [--priority <n>]
               [--deadline <date>] [--my-next "<text>"]
               [--edit] [--new-track]
```

Behavior:
- `<track>` must be an existing track unless `--new-track` is passed, in which case the directory is created.
- `--owner` defaults to `default_owner` from `hq.toml`, falling back to `yy`.
- `--slug` defaults to a slugified `--title`.
- `--status` defaults to `active`.
- Writes `{track}/{owner}-{slug}.md` with minimal frontmatter (`title`, `status`, plus any flags passed) and an empty body.
- Prints the absolute path on success.
- `--edit` opens `$EDITOR` on the new file after creation.
- Refuses to overwrite existing files (uses the `-2` suffix collision rule).
- Unknown track without `--new-track` → error with a list of existing tracks.

### FR-2: Web quick-add per column

Each kanban column gets a `+` button at the bottom. Clicking it reveals an inline two-field form:

- **Owner** field (text, autocompletes from owners scanned across all existing projects; default = `default_owner`)
- **Title** field (text; the slug is derived live and shown as `→ {owner}-{slug}.md` beneath the field so the user sees what gets created)
- `Enter` submits; `Esc` cancels.

On submit:
- New project is created with `status = <column's status>`, `track = <currently selected track>` (if a track filter is active; otherwise see FR-3), empty body.
- Card appears at the **bottom** of the column.
- Side panel auto-opens in edit mode on the new card so the user can immediately type the body.

If no track filter is active, the quick-add opens the modal (FR-3) instead, prefilled with the column's status.

### FR-3: Web "New project" modal (global)

A `+ New project` button in the header opens a modal:

- **Track** (dropdown, populated from existing tracks; includes `+ New track…` at the bottom)
- **Owner** (text + autocomplete, default `default_owner`)
- **Title** (text)
- **Slug** (text, auto-filled from title, editable)
- **Status** (dropdown, default `active`)
- *Optional disclosure ("More options…")*: priority, deadline, my_next.

Filename preview shown live. Submit creates the file, closes the modal, opens the side panel in edit mode on the new card.

`+ New track…` swaps the track dropdown for a text input (`^[a-z0-9-]+$`). On submit the directory is created if it doesn't exist.

### FR-4: New track creation

Both CLI (`--new-track`) and web (modal's `+ New track…`) create a track directory if it doesn't exist. Track name must match `^[a-z0-9-]+$` and must not start with `.` or `_` (those are excluded by auto-discovery; see `src/config.rs`).

After creation, the track is immediately usable; the web UI refreshes its track list via the existing reload mechanism.

### FR-5: REST API

New endpoints in `src/web.rs`:

- `POST /api/projects` — body: `{ track, owner, slug?, title, status?, priority?, deadline?, my_next?, create_track? }`. Response: `{ path, filename, project }`. Errors: 400 (validation), 409 (collision after suffix exhaustion — practically never), 404 (track missing and `create_track != true`).
- `POST /api/tracks` — body: `{ name }`. Creates an empty track directory. Response: `{ name }`. Errors: 400 (invalid name), 409 (already exists).

### FR-6: Empty body

The new file's body is empty (just frontmatter + a trailing newline). No starter template in this version.

### FR-7: Post-create UX

- **CLI**: prints the path. With `--edit`, opens `$EDITOR`.
- **Web**: card appears at column bottom, side panel opens in edit mode on the new card.

## Out of Scope

- Body templates (per-track or global)
- Storing owner in frontmatter
- Bulk import
- Duplicating an existing project as a starting point
- Deleting/renaming projects (separate concern)
- Inline rename of the slug after creation
- Reordering newly created cards by priority on insert (always bottom)

## Acceptance Criteria

| ID | Criteria |
|----|----------|
| AC-1 | `hq new research --title "Foo bar"` creates `research/yy-foo-bar.md` with `title: Foo bar`, `status: active`, empty body |
| AC-2 | `--owner byunghwee --slug persuasion-theory` produces `byunghwee-persuasion-theory.md` |
| AC-3 | Creating a duplicate filename produces `…-2.md`, then `…-3.md`, etc. |
| AC-4 | Collision check spans all tracks, not just the target track |
| AC-5 | Invalid owner/slug/track names are rejected with a clear error |
| AC-6 | `hq new` against a missing track without `--new-track` errors and lists existing tracks |
| AC-7 | `hq new --new-track foo …` creates the `foo/` directory and the project inside it |
| AC-8 | Web column `+` button creates a project in the column's status and the active track filter |
| AC-9 | New card appears at the bottom of its column and the side panel opens in edit mode |
| AC-10 | Web modal supports `+ New track…` and creates the directory before the project |
| AC-11 | `POST /api/projects` returns the created project payload in the same shape as `GET /api/projects` items |
| AC-12 | `default_owner` in `hq.toml` is honored when `--owner` is omitted |

## Implementation Notes

- **Slug generation**: lowercase, replace non-alphanumeric runs with `-`, trim leading/trailing `-`, collapse repeats. Strip diacritics (NFKD + filter combining marks) so "Café résumé" → `cafe-resume`.
- **Collision scan**: iterate tracks from `Config`, check for `{owner}-{slug}.md` existence. Cheap (small N).
- **File write**: `src/project_file.rs` already has frontmatter write logic; extend it with a `create_new(path, frontmatter, body)` that fails if the file exists.
- **Owner autocomplete (web)**: derive the unique owner set on the server from filename prefixes (split on first `-`); expose via existing projects payload or a new `GET /api/owners`.
- **Track creation**: `POST /api/tracks` just `fs::create_dir` + reload `Config`. The file watcher (notify crate) should pick up the new directory; verify the SSE reload fires.
- **Config**: add `default_owner: Option<String>` to `Config`.

## Related Files

- `src/main.rs` — add `New` variant to the `Subcommand` enum
- `src/commands.rs` — add `render_new` (or `run_new`, since it's a side-effecting command)
- `src/project_file.rs` — extend with create-new helper
- `src/config.rs` — add `default_owner` field
- `src/web.rs` — `POST /api/projects`, `POST /api/tracks`, owner list endpoint
- `static/index.html` — column `+` buttons, header `+ New project` modal, side-panel auto-open hook
- `tests/` — parser tests already cover frontmatter; add integration tests for `hq new` filename/collision logic
