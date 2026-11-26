# App roles

Status: proposed role split. Cross-app handoff remains future work.

HQ answers "What am I committed to, and what needs me?"
Finch answers "What work have I delegated, and what happened?"
Periscope answers "What might deserve my attention?"

| App | Role |
| --- | --- |
| HQ | Project state, commitments, next actions, waiting conditions, and review. Markdown and task files hold the records. |
| HQ on iPhone | Quick review, capture, and organization during short visits. It accesses the same records through iCloud. |
| HQ on Mac | Broader review and editing, contextual conversations, and the Git data checkout. |
| Finch | Accept and execute delegated jobs, with durable ownership, progress, results, failures, and explicit retries. |
| Codex or herdr | Sustained work with an agent when the user wants to develop, investigate, discuss, or review the result together. |
| Periscope | Discover and review papers, news, and other signals before the user turns them into commitments. |
| Dial and teum | Focus timing and time records. |
| OmniFocus | Existing task capture and personal task workflows, including the current Finch intake. |

## HQ and Finch

HQ's contextual chat helps the user understand a project and organize its records.
Its current desktop agent can also execute work in a local workspace.
Finch provides the durable queue for work delegated to run with less active supervision.

For future integration, a request made in HQ should create one Finch job and retain its link in HQ.
Finch should own that job's execution state and result history.
HQ should continue to show the project's next action and supported progress, with a link to the job when relevant.
Accepting or starting a job must leave the project's completion state unchanged.
Record supported progress in HQ and preserve the user's completion decisions.

The app-to-app handoff and result writeback still need implementation.
The current Finch intake accepts requests from OmniFocus, and HQ's existing conversations remain local to the Mac.

## One HQ across devices

Desktop and mobile share the same project and task records.
The iPhone reads one live HQ library and queues new tasks through iCloud for desktop HQ to import into Todos.
The iPhone queues project moves, deferrals, and todo edits. The Mac applies them to the source records and hosts contextual chat.
A Mac or hosted executor handles agent requests while iCloud synchronizes documents.
