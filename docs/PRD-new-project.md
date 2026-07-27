# Project Creation

Status: implemented.

## Filename convention

HQ creates project files as `<owner>-<slug>.md`.

- Owner and slug use lowercase ASCII letters, numbers, and single hyphens.
- `default_owner` in `hq.toml` supplies the default owner; the fallback is
  `yy`.
- HQ derives the slug from the title unless the user supplies one. Slug
  generation removes diacritics and collapses non-alphanumeric runs to
  hyphens.
- HQ checks all configured tracks for a collision and appends `-2`, `-3`, and
  later suffixes as needed.

The filename owner is separate from the optional `owner` frontmatter field.

## CLI

```text
hq new <track> --title "Title" [--owner <owner>] [--slug <slug>]
               [--status <status>] [--priority <number>]
               [--deadline <date>] [--my-next "<text>"]
               [--action-mode <serial|parallel>] [--edit] [--new-track]
```

The command:

- requires an existing track unless `--new-track` is present;
- defaults to `status: active`;
- writes required frontmatter plus supplied optional fields;
- creates an empty body;
- prints the created path; and
- opens the file with `$EDITOR` when `--edit` is present.

`--new-track` creates a track whose name matches the same lowercase
alphanumeric and hyphen rules. HQ rejects hidden, `_`-prefixed, absolute, and
parent-traversal paths.

## Dashboard

Each Kanban column has a quick-add control.

- With a track filter active, the inline form asks for owner and title, creates
  the project in that track and column status, then opens the focused project
  in notes-edit mode.
- With All selected, the control opens the global New Project dialog with the
  column status preselected.

The global dialog supports:

- existing or new track;
- owner with autocomplete;
- title and editable slug;
- status;
- priority, deadline, and `my_next` under More options; and
- a live filename preview.

After creation, the dashboard reloads, opens the new project, and focuses its
empty notes editor. Undo removes a newly created project only if it has not
changed since creation.

The dialog does not expose `action_mode`; use the CLI, focused metadata editor,
or Markdown frontmatter to set it.

## REST API

### `POST /api/projects`

Request:

```json
{
  "track": "research",
  "owner": "yy",
  "slug": "new-study",
  "title": "New study",
  "status": "active",
  "priority": 50,
  "deadline": "2026-08-01",
  "my_next": "Define the question",
  "action_mode": "parallel",
  "create_track": false
}
```

Only `track` and `title` are required. The response is:

```json
{
  "file": "research/yy-new-study.md",
  "project": {}
}
```

The `project` value has the same shape as an item in `GET /api/projects`.
Validation errors return 400. A missing track returns 404 unless
`create_track` is true. File conflicts return 409.

### `POST /api/tracks`

Request:

```json
{"name": "research"}
```

The endpoint creates an empty track directory and returns its name. Under
auto-discovery, the directory appears as a track after it contains a project.

## Current limits

- No project templates, duplication, rename, or deletion.
- No inline filename rename after creation.
- The dashboard creation dialog does not expose `action_mode`.

## Implementation

- `src/main.rs` and `src/commands.rs`: CLI.
- `src/project_file.rs`: path validation and atomic creation.
- `src/web.rs`: project and track endpoints.
- `static/index.html`: quick add, global dialog, and post-create focus.
- `tests/parser_tests.rs`, `tests/static_index_tests.rs`, and `src/web.rs`
  tests: regression coverage.
