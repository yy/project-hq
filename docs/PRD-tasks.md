# Standalone tasks

Status: implemented.

## Purpose

Represent small independent actions without inflating the project model or
creating one file per task. Tasks remain separate from projects and routines.

## Storage

- Active tasks: `_tasks/todo.txt`
- Completed tasks: `_tasks/done.txt`
- One task per line; blank lines are preserved.
- Completion moves the exact source line to `done.txt` and prefixes the todo.txt
  completion marker and date.

Example:

```text
2026-07-31 Call electrician @phone &electrician +house p:100 due:2026-08-15
```

The parser recognizes standard todo.txt creation/completion dates, `@context`,
and `+tag`, plus HQ extensions:

- `&person`
- `p:<number>`: manual priority; higher values sort first, absent values last
- `due:YYYY-MM-DD`
- `t:YYYY-MM-DD`: unavailable until this date
- `status:waiting`

Unknown tokens remain part of the task text and survive edits.

## Behavior

- Projects, Tasks, Routines, and Analysis are peer navigation views.
- Available is the default task scope; future-deferred tasks have a separate
  scope and disappear automatically until their date arrives.
- A compact row opens task details for text, priority, due date, deferral, and
  waiting state.
- Rows can be completed or quickly deferred.
- Dragging computes a numeric priority between neighboring tasks, preserving a
  manually sortable text representation.
- Search covers task text and annotations.

## Safety and integration

- Mutations identify both the physical line and its expected raw contents, so a
  stale UI cannot overwrite an external edit.
- Create, edit, defer, prioritize, and complete operations are undoable.
- File watching reloads task text files.
- The Agent snapshot, validator, change applier, and Undo support both task
  files.
