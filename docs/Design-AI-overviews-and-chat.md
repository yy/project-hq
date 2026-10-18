# AI project overviews and contextual chat

Status: desktop overview prototype implemented 2026-10-18. Mobile remains a design direction.

HQ should make commitments easy to review and organize, even when project documents contain extensive history and detail.
Combine concise AI-generated overviews with a Codex-style conversation grounded in the same HQ files.
Substantial execution can happen in Codex or a cloud app; HQ's first scope is review, capture, and organization.

## Project overviews

Opening HQ should offer useful orientation without requiring a prompt.
Each project gets a short overview that answers:

- What this is: a one-sentence description.
- Where things stand: the latest supported progress, blocker, or dependency.
- What needs me: an action or decision, including an explicit waiting condition when the record supports one.

Extract what matters for a decision; expand the overview to see supporting details and the full document.
Keep direct controls for routine actions such as completing, deferring, or changing status.

## Contextual chat

Across HQ, the user can ask what needs attention or capture a new commitment.
Selecting a project gives the conversation that project as its default context, with the scope visible.
"What's blocking this?" should bring up relevant details without requiring the user to restate the project.
Instructions such as "Move this to waiting; Alex owes me a draft" update the underlying record and the displayed overview.

Preserve the existing revision checks and Undo for supported HQ edits.
Changing views must not silently retarget work already submitted.

Todos have an **Ask Agent** action in Today and Todos.
It attaches the exact saved todo above the composer without sending a message or replacing a draft.
The attachment is captured when a message is queued and remains visible in the sent message; selecting another item cannot retarget that request.
Resolve the reference against the current todo file before starting the turn, accepting an unchanged item that moved to another line and rejecting an edited or completed item rather than substituting its former neighbor.

## Source records and freshness

Markdown documents, checklists, and task files remain authoritative.
Generating an overview must not replace or shorten the source record.
Distinguish suggested next steps from recorded actions and expose missing or contradictory information as uncertainty.

Refresh an overview when its source changes, including after an agent edit.
Mark stale summaries and keep records and controls usable while generation is pending or fails.
Use deterministic rules for status, due dates, deferral, counts, and view membership; AI interpretation should not silently change them.

## Desktop navigation and mobile

The desktop sidebar should provide stable entry points such as Today, Projects, and Todos, alongside Routines and Analysis.
See [[PRD-sidebar]] for the desktop navigation and Today inclusion rules.
Show the selected collection or project centrally, with contextual chat beside it.
Keep Today's date-based inclusion rules separate from AI interpretation; projects needing attention without a deadline must remain easy to review elsewhere.

A mobile app should let the user scan commitments, ask questions, and dictate updates in short sessions.
Overview, details, and chat can occupy successive screens while preserving project context.
Things and OmniFocus inform the preference for predictable navigation and fast direct interaction.

Prototype on desktop with real long project documents: can the user identify the current situation and next step without reading the full history?
Use that experience to shape mobile.

## Decisions still open

- How users correct an overview beyond editing its source and regenerating it.
- Mobile storage and synchronization, offline behavior, authentication, and whether agent requests depend on an awake Mac or a hosted service.
- How HQ hands off substantial work to Codex or a cloud app and brings the result back to the project.

The desktop agent currently runs locally; mobile architecture and automated handoffs remain open.

## Desktop prototype

Generate an overview when a project opens; reuse cached summaries in Today and Projects.
Opening HQ does not generate summaries for every project.
The project view leads with the three overview sections, keeps recorded actions and metadata controls available, and puts the complete notes behind an expandable section.
Each section can expose verbatim supporting excerpts; suggestions are labeled separately from recorded actions.

Run generation through the installed Codex CLI in a separate, ephemeral, read-only session with shell tools and web search disabled.
Supply only the selected record as data; overview generation does not share or interrupt the Agent conversation.
Use structured output and verify supporting excerpts against the supplied record before displaying the result.
This uses the user's existing Codex sign-in and allowance.
The separate **Project overview model** setting is available from the overview's model button and the Agent model menu.
It defaults to GPT-6.1-Sol with low reasoning effort; every generation passes the selected model explicitly.
HQ saves its overview model, chat model, and chat thinking level in application settings outside the vault, so choices survive server-port changes and app relaunches.
Chat starts with HQ's explicit GPT-6-Astra / xhigh defaults until changed in its model menu.

Store derived summaries in the local application cache, outside the HQ vault, keyed by HQ directory, project path, source revision, and selected overview model.
Changing a record makes its overview stale; refresh an open project's overview automatically and keep the previous text visibly stale while refreshing or after a failure.
Offer manual retry or refresh, allow two concurrent generations, and time out each run after two minutes.
Records above 120 KB show an explicit size limit instead of being silently truncated.
Failures leave the complete record and direct editing controls available.
The desktop cache and local Codex process are prototype choices, not mobile architecture decisions.
