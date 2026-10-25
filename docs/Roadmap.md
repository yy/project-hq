# Roadmap

Forward-looking feature ideas. Not committed work — captured here so they
don't get lost between sessions.

## AI project overviews and contextual chat

[[Design-AI-overviews-and-chat]] records the direction for a concise first view of HQ: AI-generated project overviews with expandable source records and contextual chat over the same files.
The desktop prototype generates overviews when projects open and reuses them in Today and Projects, alongside the sidebar and Agent workspace.
Use this experience to shape a mobile app.
The agreed mobile direction keeps the normal Git data checkout and adds iCloud document synchronization for independent mobile review, capture, and organization.
Reconciliation and offline recovery need fixture-based verification before live data migration.
Substantial execution stays in Codex or a cloud app, with executor hosting and handoff details still open.

## teum timer integration

Show the running [teum](https://github.com/yy/teum) timer in the HQ header.
When the user "actively focuses" on a project (a new mode, distinct from
just clicking a card), auto-start a timer for that project.

**Sketch.** Start with an explicit focus toggle: pin a card → call
`teum start <preset>`; unpin or switch project → `teum stop` / swap.
Display `teum status` in the header. Auto-detection (body-edit activity,
sustained side-panel time) is a v2 problem.

**Open questions.**
- How does HQ know the right teum preset for a project? Frontmatter field
  (`teum_preset: research-foo`)? Track-level default? Auto-create on first
  focus?
- What happens when the user switches projects mid-session — stop the old
  timer, or prompt?
- Polling vs. file-watch on teum's state file?

## Agent dispatch from task items

Let the user dispatch any body checkbox to an AI agent. Reference:
[openai/symphony](https://github.com/openai/symphony) for multi-agent
orchestration prior art.

HQ already provides a persistent Codex panel backed by `codex app-server`. It
runs in an isolated snapshot of the whole HQ repository, receives the selected
project as the default context, streams answers while hiding tool activity, and
applies valid project changes with revision checks and Undo.

**Implemented.** Ask Agent attaches a checklist item from the next-action section or full notes, preserving the draft and waiting for the user to send.
Queued requests retain the project path and exact checkbox text; HQ validates the reference before inference.

**Open questions.**
- Should HQ mark an action as delegated while the turn runs?
- Should the action keep a link to an agent conversation, or should the
  resulting project edit remain the only durable record?

Independent parallel conversations and local chat persistence are implemented.
Automatic multi-agent orchestration remains out of scope until single-action dispatch is useful in daily work.
Chats can be renamed, archived, and restored without losing saved work.
Project detail lists related chats and can open one or start a separate project-scoped conversation.
Project checklist attachments are implemented; automatic execution and delegation status remain open.

Future adapters may support Claude Code and local runtimes such as Ollama or LM
Studio without exposing provider credentials to browser JavaScript.
