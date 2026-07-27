# Roadmap

Forward-looking feature ideas. Not committed work — captured here so they
don't get lost between sessions.

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

Let the user dispatch a `my_next` line (or any checkbox in the body) to an
AI agent. Reference: [openai/symphony](https://github.com/openai/symphony)
for multi-agent orchestration prior art.

HQ already provides a persistent Codex panel backed by `codex app-server`. It
runs in an isolated snapshot of the whole HQ repository, receives the selected
project as the default context, streams tool activity, and applies valid project
changes with revision checks and Undo.

**Sketch.** Add an action menu item that sends the selected `my_next` or
checkbox text to the existing Agent panel. Include the project file and body
line in the prompt. Prefer filling the composer and letting the user send it;
one-click execution can follow if the extra confirmation proves unnecessary.

**Open questions.**
- Should dispatch fill the composer or start the turn immediately?
- Should HQ mark an action as delegated while the turn runs?
- Should the action keep a link to an ephemeral agent turn, or should the
  resulting project edit remain the only durable record?

Multi-agent orchestration remains out of scope until single-action dispatch is
useful in daily work.

## Agent runtime controls

Add provider, model, and reasoning-effort selectors to the persistent Agent
panel. The current implementation inherits the local Codex CLI authentication
and configuration. Future adapters may support Claude Code and local runtimes
such as Ollama or LM Studio without exposing provider credentials to browser
JavaScript.
