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

**Sketch.** MVP shells out to `claude -p "<task>"` (or `codex`) inside the
project's git worktree, streams output to a sidecar `.md`, surfaces it in
the side panel as a thread. No supervision, no multi-agent — just dispatch
+ log + read.

**Open questions.**
- Where does the agent run — local subprocess, GitHub Actions, hosted
  service? Local is simplest but blocks on the user's machine.
- How does the agent know what "the project" is — pass the project file's
  path + body as context?
- Output channel: separate `.md` per task, or append to the project body
  under the task line?
- Symphony-style orchestration (plans, hand-offs, supervisor) is overkill
  until single-agent dispatch is validated.
