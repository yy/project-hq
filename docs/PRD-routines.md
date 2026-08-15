# First-class routines

Status: implemented.

## Purpose

Replace recurring OmniFocus maintenance without turning permanent obligations
into active pseudo-projects. Routines are absent from project counts, Kanban,
Analysis, and stale-project reports.

## Storage

One independently completable routine per `_routines/*.md` file. Required
frontmatter:

- `type: routine`
- `title`
- `area`
- `repeat`: positive number plus hour, day, week, month, or year
- `repeat_from`: `completion` or `schedule`
- `available_before`: nonnegative interval
- `next_due`: `YYYY-MM-DD`, `YYYY-MM-DD HH:MM`, or an RFC 3339 timestamp

Optional `last_completed` and `deferred_until` values are maintained by the
app. Deferral accepts a date or RFC 3339 timestamp. The body holds notes and an
append-only `## History` of completion and skip events.

Hour cadences are intraday: the schedule carries a clock time, so `next_due`
and `last_completed` round-trip as local `YYYY-MM-DD HH:MM`, the same form the
history entries use. Every other cadence keeps the plain-date form.

## Recurrence semantics

- Completion-based: completion on date D sets the next due date to D plus the
  interval.
- Fixed schedule: completion advances from the scheduled due date until exactly
  one future occurrence remains.
- An occurrence stays due for the rest of its day, or for one interval when the
  cadence is intraday, and is overdue after that.
- A missed routine remains overdue. Missed occurrences never accumulate.
- Skip advances the schedule but does not change `last_completed`.
- Deferral affects only current visibility and clears on completion or skip.
- `available_on = next_due - available_before`.

## Interface

The Routines top-level view is one availability timeline grouped into Now,
Today, This week, This month, This season, This year, and Later. Now contains
actionable occurrences; future sections show when an occurrence becomes
available again. Rows are full-width and compact, all horizons remain visible
when empty, and unavailable occurrences are heavily muted. Clicking opens
schedule and notes editing. Complete, Skip, and hour- or date-level Defer are
available directly from each row. All mutations use the shared revision-safe
Undo history.

The embedded Agent may create or edit valid `_routines/` files. The same
multi-file revision checks and automatic application used for projects apply.
