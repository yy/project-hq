# Desktop sidebar and My Plate

Status: implemented.

## Navigation

A persistent left sidebar opens My Plate, Projects, Todos, Routines, and Analysis.
HQ opens to My Plate. The HQ button returns to it.
The selected view has a highlighted row, an accessible current-page state, and a matching heading.
Below 900px, the sidebar becomes an icon rail with accessible labels and tooltips.
The selected agent conversation remains beside the main content, including when a project is open.
An Agents section below navigation lists conversations and their running or attention state; New chat starts independent work.
Settings at the bottom owns the default AI model for new chats and project overviews.
The top activity charts do not navigate; use the Analysis sidebar entry.

Search and creation follow the current view.
My Plate and Todos create standalone todos. Projects and Analysis create projects. Routines creates routines.
Project detail returns to the collection it was opened from.
Project track filters and todo availability filters stay within their respective views.

## My Plate

My Plate shows the same three groups as the iPhone app.

- **Todos** includes visible standalone tasks due within the next seven days or overdue, including waiting tasks.
- **Projects** includes visible projects with `my-plate` status, ordered by priority. A project can be on the plate with or without a deadline.
- **Routines** includes available, due, and overdue occurrences.

My Plate searches across all three groups independently of the track and todo filters.
The todo cutoff includes the date seven local calendar days from today.
My Plate refreshes on data changes, entry, local midnight, and the next scheduled availability or deferral boundary.

Todo and routine rows reuse the existing editors, completion, deferral, and Undo behavior.
Todo drag ordering is available in Todos.
Project rows open the existing detail view.

These inclusion rules are deterministic and separate from the overviews described in [[Design-AI-overviews-and-chat]].
