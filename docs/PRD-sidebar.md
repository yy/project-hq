# Desktop sidebar and Today

Status: implemented.

## Navigation

A persistent left sidebar opens Today, Projects, Todos, Routines, and Analysis.
Today is the launch view; the HQ button returns to it.
The selected view has a highlighted row, an accessible current-page state, and a matching heading.
Below 900px, the sidebar becomes an icon rail with accessible labels and tooltips.
The selected agent conversation remains beside the main content, including when a project is open.
An Agents section below navigation lists conversations and their running or attention state; New chat starts independent work.
Settings at the bottom owns the default AI model for new chats and project overviews.
The top activity charts do not navigate; use the Analysis sidebar entry.

Search and creation follow the current view.
Today and Todos create standalone todos; Projects and Analysis create projects; Routines creates routines.
Project detail returns to the collection it was opened from.
Project track filters and todo availability filters stay within their respective views.

## Today

Today groups existing records into Todos, Projects, and Routines:

- Todos: visible standalone tasks due within the next seven days or overdue, including waiting tasks.
- Projects: visible projects with a deadline within the next seven days or overdue, excluding submitted, done, and dropped statuses.
- Routines: occurrences whose timing is available, due, or overdue.

Undated work remains accessible from Projects and Todos.
Today ignores the selected project track and todo availability scope; search applies across all three groups.
Dates use the local calendar, including existing project deadlines with a date followed by explanatory text.
The deadline cutoff includes the date seven calendar days from today.
Today refreshes on data changes, entry, local midnight, and the next scheduled availability or deferral boundary.

Todo and routine rows reuse the existing editors, completion, deferral, and Undo behavior.
Todo drag ordering remains in Todos; Today does not reorder a filtered subset.
Project rows open the existing detail view.

These inclusion rules are deterministic and separate from the overviews described in [[Design-AI-overviews-and-chat]].
