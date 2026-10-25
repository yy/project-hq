# AI project overviews and contextual chat

Status: desktop overview prototype implemented 2026-10-18. Git + iCloud mobile storage direction agreed 2026-10-25; implementation is pending.

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

Project checklist items offer the same **Ask Agent** attachment in the next-action section and beside incomplete checkboxes in the full notes.
Show the project and item in the composer and sent message; attaching preserves the draft and waits for the user to send.
Capture the project path, body line, and exact saved checkbox text with each queued request, independently of later navigation.
Before inference, resolve the item in that project, allowing a unique unchanged item to move and rejecting changed, completed, or ambiguous references.
Ignore fenced examples when identifying checklist items.
Attaching an item does not mark it complete or delegated.
Queued messages have a Remove action so a rejected reference can be replaced without restarting the conversation.

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

Each project's Chats section lists related conversations with their current chat status, including queued requests and archived history.
Start a new chat from that section to scope it to the project, or open an existing chat while keeping the project visible.
Preserve the chat's draft, explicit attachment, and queued request scopes when switching.
Save project links when a chat is explicitly started for a project or a project-scoped request is accepted; browsing another project does not create a link.
Links survive transcript clearing, archiving, and restart.
Older conversations gain links as new project-scoped requests are submitted; do not infer historical links from their last viewed project.
Chat activity does not change project or checklist status.

Each conversation uses a separate workspace and Codex session.
Apply edits one at a time against the current file revision; conflicting edits pause that chat's queue and remain available to inspect, retry, or discard.
Undo and direct edits use the same write lock without interrupting other conversations.
Chat history, drafts, selected project or todo context, queued messages, and model overrides persist on this Mac, separately for each HQ folder.
The selected chat and project reopen after a page reload or app restart.
HQ saves runtime replies even when the browser is disconnected and resumes a stored Codex conversation when the user selects it.
This uses [Codex thread/resume](https://learn.chatgpt.com/docs/app-server#start-or-resume-a-thread), preserving conversation context without submitting old prompts again.
Codex may leave an unused chat without saved runtime history until its first message.
If that history is missing and HQ has never submitted a message in the chat, recreate the Codex session while preserving HQ's chat identity, workspace, draft, attachments, project links, queue, and settings.
Save the replacement runtime ID separately so subsequent messages and restarts use it.
A chat with any submitted message, including hidden transcript entries, must report missing runtime history and preserve its HQ record.

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

## Mobile storage: Git and iCloud

Keep `~/git/hq` as a normal Git checkout for desktop HQ, agents, and CLI tools.
Git remains the versioned data repository, with commits, remote synchronization, and recovery history.
HQ synchronizes the supported Markdown, checklist, task, and configuration files between that checkout and iCloud Drive documents.
Git pulls flow into iCloud; phone edits arriving through iCloud flow into the checkout and its commit/push workflow.
Keep `.git`, agent workspaces, and device-specific runtime state outside iCloud.

Mobile review, capture, and direct organization should work with downloaded records while offline or while the Mac sleeps.
Show pending synchronization explicitly; Git commits and pushes catch up when the Mac is available.
Cache overviews with their source revision and model so offline review can distinguish current and stale summaries.
The desktop overview cache remains local until portable summary synchronization is implemented.

HQ must reconcile both copies against the last shared revision before writing.
Preserve concurrent edits, merge compatible changes, and surface contradictory changes for resolution.
Coordinate iCloud document access and retain conflict versions until they are resolved.
Incomplete downloads or temporarily unavailable files must never be interpreted as deletions.
Define stable item identities and explicit deletion handling before enabling writes across devices.
Use fixture data to verify offline edits, concurrent changes, renames, deletions, and restart recovery before migrating live data.

iCloud transports documents and saved results; an available Mac or hosted executor runs agent requests.
Show requests as pending until an executor accepts them.
Portable conversation records and request handling need a separate design from the Mac's Codex session and workspace storage.
See [Apple's document synchronization guide](https://developer.apple.com/documentation/uikit/synchronizing-documents-in-the-icloud-environment) for coordinated access and conflict handling.

## Decisions still open

- How users correct an overview beyond editing its source and regenerating it.
- The iCloud container, reconciliation and deletion mechanics, stable item identities, and migration from the existing data folder.
- Portable summaries and conversations, and the hosting, authentication, and delivery of agent requests.
- How HQ hands off substantial work to Codex or a cloud app and brings the result back to the project.

The desktop agent currently runs locally; mobile synchronization and remote agent execution remain unimplemented.

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
