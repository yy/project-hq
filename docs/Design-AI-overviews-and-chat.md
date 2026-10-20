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

## Multiple conversations

The left sidebar has an Agents section with New chat and a list of conversations.
Selecting a conversation changes the adjacent chat pane while leaving the project or collection visible.
Each chat keeps its own draft, todo attachment, transcript, model override, queue, and running status.
Background chats continue working; finished responses and errors appear in the sidebar.
The chat header's actions menu offers Rename and Archive.
Custom names survive subsequent prompts and restarts.
Archive moves a settled chat into the sidebar's expandable Archived list, preserving history, drafts, attachments, queues, and unapplied edits.
Running or applying chats cannot be archived.
Archived chats can be read; restore one before sending or applying work, with saved queues still paused.

Each conversation uses a separate workspace and Codex session.
Apply edits one at a time against the current file revision; conflicting edits pause that chat's queue and remain available to inspect, retry, or discard.
Undo and direct edits use the same write lock without interrupting other conversations.
Chat history, drafts, selected project or todo context, queued messages, and model overrides persist on this Mac, separately for each HQ folder.
The selected chat and project reopen after a page reload or app restart.
HQ saves runtime replies even when the browser is disconnected and resumes a stored Codex conversation when the user selects it.
This uses [Codex thread/resume](https://learn.chatgpt.com/docs/app-server#start-or-resume-a-thread), preserving conversation context without submitting old prompts again.

An interrupted request is marked explicitly, and queued requests remain paused after restoration.
Continue queued requests only after the user selects that action; sending a new message does not replay an interrupted request.
Edits completed while the browser was disconnected require review before application.
Keep partial replies and unapplied workspace changes, including the original revision baseline, so restart recovery cannot overwrite newer live edits.

Save chat state and workspaces under HQ's local application settings directory, outside the data repository.
Use atomic file replacement, save a queued request before starting inference, and reject duplicate request IDs.
Draft updates cannot replace stored replies; conflicting draft revisions from another window return an error.
Surface save failures and preserve unreadable records rather than replacing them with empty history.
The local store and Codex's saved conversation state are required to continue a chat on this Mac; portable chat synchronization and mobile access remain open.

## Source records and freshness

Markdown documents, checklists, and task files remain authoritative.
Generating an overview must not replace or shorten the source record.
Distinguish suggested next steps from recorded actions and expose missing or contradictory information as uncertainty.

Refresh an overview when its source changes, including after an agent edit.
Mark stale summaries and keep records and controls usable while generation is pending or fails.
Use deterministic rules for status, due dates, deferral, counts, and view membership; AI interpretation should not silently change them.

## Desktop navigation and mobile

The desktop sidebar provides Today, Projects, Todos, Routines, and Analysis, with Agents below navigation and Settings at the bottom.
The top activity charts are read-only; Analysis opens from the sidebar.
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
HQ has one default model in Settings, available from the desktop sidebar and macOS Settings.
New chats and project overviews use this model explicitly; the separate overview-model preference is removed.
A chat's model menu changes only that conversation and never updates the app default.
HQ saves the default model and chat thinking level outside the vault, so choices survive server-port changes and app relaunches.
Existing saved chat preferences become the app defaults; the initial fallback remains GPT-6-Astra / xhigh.
Overview generation uses low reasoning effort.

Store derived summaries in the local application cache, outside the HQ vault, keyed by HQ directory, project path, source revision, and selected overview model.
Changing a record makes its overview stale; refresh an open project's overview automatically and keep the previous text visibly stale while refreshing or after a failure.
Offer manual retry or refresh, allow two concurrent generations, and time out each run after two minutes.
Records above 120 KB show an explicit size limit instead of being silently truncated.
Failures leave the complete record and direct editing controls available.
The desktop cache and local Codex process are prototype choices, not mobile architecture decisions.
