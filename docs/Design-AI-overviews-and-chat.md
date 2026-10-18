# AI project overviews and contextual chat

Status: design direction, recorded 2026-10-18. AI overviews and the mobile app are not implemented.

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

- Where and when overviews are generated and cached, and how users correct them.
- Mobile storage and synchronization, offline behavior, authentication, and whether agent requests depend on an awake Mac or a hosted service.
- How HQ hands off substantial work to Codex or a cloud app and brings the result back to the project.

The desktop agent currently runs locally; mobile architecture and automated handoffs remain open.
