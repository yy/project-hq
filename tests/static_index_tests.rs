use std::io;
use std::process::Command;

fn compute_priority_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function computePriority(")
        .expect("static index should define computePriority");
    let rest = &html[start..];
    let end = rest
        .find("\n\nasync function handleDrop")
        .expect("computePriority should end before handleDrop");

    rest[..end].to_string()
}

fn get_column_items_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function matchesSearch(")
        .expect("static index should define matchesSearch");
    let rest = &html[start..];
    let end = rest
        .find("\n\nfunction computePriority")
        .expect("getColumnItems should end before computePriority");

    rest[..end].to_string()
}

fn days_since_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function parseLocalDate(")
        .expect("static index should define parseLocalDate");
    let rest = &html[start..];
    let end = rest
        .find("\n\n// SSE live reload")
        .expect("date helpers should end before the SSE setup");

    rest[..end].to_string()
}

fn deferral_helpers_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function padDatePart(")
        .expect("static index should define deferral date helpers");
    let rest = &html[start..];
    let end = rest
        .find("\n\nconst DEFER_LABELS")
        .expect("date helpers should end before deferral actions");

    rest[..end].to_string()
}

fn track_colors_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("const TRACK_PALETTE")
        .expect("static index should define track colors");
    let rest = &html[start..];
    let end = rest
        .find("\n\nfunction renderOwnerOptions")
        .expect("track color setup should end before owner rendering");

    rest[..end].to_string()
}

fn markdown_link_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function resolveMarkdownProjectLink(")
        .expect("static index should define Markdown link routing");
    let rest = &html[start..];
    let end = rest
        .find("\n\nfunction wireRenderedLinks")
        .expect("link resolver should end before DOM wiring");

    rest[..end].to_string()
}

fn clear_agent_conversation_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function clearAgentConversation(")
        .expect("static index should define Agent transcript clearing");
    let rest = &html[start..];
    let end = rest
        .find("\n\nfunction connectAgentEvents")
        .expect("Agent clearing should end before event wiring");

    rest[..end].to_string()
}

fn agent_elapsed_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function formatAgentElapsed(")
        .expect("static index should define Agent elapsed formatting");
    let rest = &html[start..];
    let end = rest
        .find("\n\nfunction updateAgentElapsed")
        .expect("elapsed formatting should end before its DOM update");

    rest[..end].to_string()
}

fn agent_running_indicator_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function updateAgentElapsed(")
        .expect("static index should define Agent activity tooltip updates");
    let rest = &html[start..];
    let end = rest
        .find("\n\nfunction clearAgentConversation")
        .expect("running indicator updates should end before transcript clearing");

    rest[..end].to_string()
}

fn routine_filter_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function routineMatchesSearch(")
        .expect("static index should define routine search");
    let rest = &html[start..];
    let end = rest
        .find("\n\nfunction routineDateLabel")
        .expect("routine filtering should end before labels");

    rest[..end].to_string()
}

fn routine_label_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function routineDateLabel(")
        .expect("static index should define routine labels");
    let rest = &html[start..];
    let end = rest
        .find("\n\nfunction addRoutineInterval")
        .expect("routine labels should end before interval math");

    rest[..end].to_string()
}

fn task_priority_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function effectiveTaskPriority(")
        .expect("static index should define task priority helpers");
    let rest = &html[start..];
    let end = rest
        .find("\n\nasync function reorderStandaloneTask")
        .expect("task priority helpers should end before task mutation");
    rest[..end].to_string()
}

fn prompt_waiting_on_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function promptWaitingOn(")
        .expect("static index should define the waiting-on modal");
    let rest = &html[start..];
    let end = rest
        .find("\n\nasync function moveProjectStatus")
        .expect("waiting-on modal should end before project status moves");
    rest[..end].to_string()
}

fn run_node(script: String) {
    let output = match Command::new("node").arg("-e").arg(script).output() {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run node: {error}"),
    };

    assert!(
        output.status.success(),
        "node regression failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn compute_priority_accounts_for_dragged_card_when_moving_downward() {
    let script = format!(
        r#"
{}

const items = [
  {{ file: "a.md", priority: 30 }},
  {{ file: "b.md", priority: 20 }},
  {{ file: "c.md", priority: 10 }},
];

const downwardPriority = computePriority(items, 2, "a.md");
if (!(downwardPriority > 10 && downwardPriority < 20)) {{
  throw new Error(`expected downward drag between b and c, got ${{downwardPriority}}`);
}}

const upwardPriority = computePriority(items, 1, "c.md");
if (!(upwardPriority > 20 && upwardPriority < 30)) {{
  throw new Error(`expected upward drag between a and b, got ${{upwardPriority}}`);
}}
"#,
        compute_priority_source()
    );

    run_node(script);
}

#[test]
fn get_column_items_sorts_and_filters_like_rendered_columns() {
    let script = format!(
        r#"
let projects = [
  {{ file: "research-low.md", track: "research", status: "active", priority: 10, title: "Embedding paper" }},
  {{ file: "admin-high.md", track: "admin", status: "active", priority: 30, title: "Hiring plan" }},
  {{ file: "research-high.md", track: "research", status: "active", priority: 20, title: "Citation network" }},
  {{ file: "future.md", track: "research", status: "active", priority: 100, title: "Future", visible: false, deferred_until: "2027-01-01" }},
  {{ file: "waiting.md", track: "research", status: "waiting", priority: 99, title: "Embedding review" }},
  {{ file: "submitted.md", track: "research", status: "submitted", priority: 100, title: "Submitted paper" }},
  {{ file: "done.md", track: "research", status: "done", priority: 80, title: "Finished project" }},
  {{ file: "dropped.md", track: "research", status: "dropped", priority: 90, title: "Abandoned project" }},
];
let activeTrack = null;
let searchQuery = "";

{}

const allActive = getColumnItems("active").map(project => project.file).join(",");
if (allActive !== "admin-high.md,research-high.md,research-low.md") {{
  throw new Error(`expected all active projects by priority, got ${{allActive}}`);
}}

activeTrack = "research";
const researchActive = getColumnItems("active").map(project => project.file).join(",");
if (researchActive !== "research-high.md,research-low.md") {{
  throw new Error(`expected visible research projects by priority, got ${{researchActive}}`);
}}

activeTrack = null;
searchQuery = "embedding";
const searched = getColumnItems("active").map(project => project.file).join(",");
if (searched !== "research-low.md") {{
  throw new Error(`expected search to match title case-insensitively, got ${{searched}}`);
}}

searchQuery = "future";
const deferredSearch = getColumnItems("active").map(project => project.file).join(",");
if (deferredSearch !== "future.md") {{
  throw new Error(`expected search to reveal a deferred project, got ${{deferredSearch}}`);
}}

searchQuery = "2027-01-01";
const deferredDateSearch = getColumnItems("active").map(project => project.file).join(",");
if (deferredDateSearch !== "future.md") {{
  throw new Error(`expected deferred date to be searchable, got ${{deferredDateSearch}}`);
}}

searchQuery = "EMBEDDING research";
const multiTerm = getColumnItems("active").map(project => project.file).join(",");
if (multiTerm !== "research-low.md") {{
  throw new Error(`expected every term to match across fields, got ${{multiTerm}}`);
}}

searchQuery = "embedding admin";
const noMatch = getColumnItems("active").map(project => project.file).join(",");
if (noMatch !== "") {{
  throw new Error(`expected no project to match all terms, got ${{noMatch}}`);
}}

searchQuery = "";
const waiting = getColumnItems("waiting").map(project => project.file).join(",");
if (waiting !== "submitted.md,waiting.md") {{
  throw new Error(`expected submitted projects inside waiting, got ${{waiting}}`);
}}

const submitted = projects.find(project => project.file === "submitted.md");
if (moveStatusForDrop(submitted, "waiting") !== "submitted") {{
  throw new Error("reordering in Waiting should preserve submitted status");
}}
if (moveStatusForDrop(submitted, "active") !== "active") {{
  throw new Error("moving out of Waiting should use the target status");
}}

const done = getColumnItems("done").map(project => project.file).join(",");
if (done !== "dropped.md,done.md") {{
  throw new Error(`expected dropped projects inside Done, got ${{done}}`);
}}

const dropped = projects.find(project => project.file === "dropped.md");
if (moveStatusForDrop(dropped, "done") !== "dropped") {{
  throw new Error("reordering in Done should preserve dropped status");
}}
if (moveStatusForDrop(dropped, "active") !== "active") {{
  throw new Error("moving out of Done should use the target status");
}}
const active = projects.find(project => project.file === "research-low.md");
if (moveStatusForDrop(active, "done") !== "done") {{
  throw new Error("moving an open project into Done should mark it done");
}}
if (statusLabel("my-plate") !== "My Plate") {{
  throw new Error("status chooser should present readable labels");
}}
"#,
        get_column_items_source()
    );

    run_node(script);
}

#[test]
fn open_project_helper_excludes_terminal_statuses() {
    let script = format!(
        r#"
{}

const visible = [
  {{ status: "my-plate" }},
  {{ status: "active" }},
  {{ status: "waiting" }},
  {{ status: "done" }},
  {{ status: "dropped" }},
].filter(isOpenProject).map(project => project.status).join(",");

if (visible !== "my-plate,active,waiting") {{
  throw new Error(`expected only open projects, got ${{visible}}`);
}}
"#,
        get_column_items_source()
    );

    run_node(script);
}

#[test]
fn compute_priority_uses_fractional_priority_when_no_integer_gap_exists() {
    let script = format!(
        r#"
{}

const items = [
  {{ file: "a.md", title: "Zulu", priority: 20 }},
  {{ file: "b.md", title: "Beta", priority: 19 }},
  {{ file: "c.md", title: "Alpha", priority: 10 }},
];

const priority = computePriority(items, 1, "c.md");
if (priority !== 19.5) {{
  throw new Error(`expected fractional priority between a and b, got ${{priority}}`);
}}
"#,
        compute_priority_source()
    );

    run_node(script);
}

#[test]
fn days_since_ignores_future_dates() {
    let script = format!(
        r#"
{}

const days = daysSince("2999-01-01");
if (days !== null) {{
  throw new Error(`expected future waiting date to be hidden, got ${{days}}`);
}}
"#,
        days_since_source()
    );

    run_node(script);
}

#[test]
fn days_since_treats_yyyy_mm_dd_as_local_calendar_date() {
    let script = format!(
        r#"
process.env.TZ = "America/New_York";
const RealDate = Date;
class MockDate extends RealDate {{
  constructor(...args) {{
    if (args.length === 0) return new RealDate("2026-05-22T01:00:00Z");
    return new RealDate(...args);
  }}
  static now() {{
    return new RealDate("2026-05-22T01:00:00Z").getTime();
  }}
  static parse(value) {{
    return RealDate.parse(value);
  }}
  static UTC(...args) {{
    return RealDate.UTC(...args);
  }}
}}
global.Date = MockDate;

{}

const days = daysSince("2026-05-22");
if (days !== null) {{
  throw new Error(`expected tomorrow to stay hidden, got ${{days}}`);
}}
"#,
        days_since_source()
    );

    run_node(script);
}

#[test]
fn defer_presets_produce_expected_dates_and_instants() {
    let script = format!(
        r#"
process.env.TZ = "America/New_York";
{}

const afternoon = new Date("2026-07-26T16:00:00-04:00");
if (deferUntilForPreset("hour", afternoon) !== "2026-07-26T21:00:00.000Z") {{
  throw new Error("1 hour preset should preserve an exact instant");
}}
if (deferUntilForPreset("evening", afternoon) !== "2026-07-26T22:00:00.000Z") {{
  throw new Error("evening preset should resolve to 6 PM local time");
}}
if (deferUntilForPreset("tomorrow", afternoon) !== "2026-07-27") {{
  throw new Error("tomorrow preset should use the next local calendar date");
}}
if (deferUntilForPreset("week", afternoon) !== "2026-08-02") {{
  throw new Error("week preset should add seven local calendar days");
}}

const afterEvening = new Date("2026-07-26T19:00:00-04:00");
if (deferUntilForPreset("evening", afterEvening) !== "2026-07-27T22:00:00.000Z") {{
  throw new Error("evening preset should roll forward after 6 PM");
}}

const monthEnd = new Date("2026-01-31T12:00:00-05:00");
if (deferUntilForPreset("month", monthEnd) !== "2026-02-28") {{
  throw new Error("month preset should clamp to the final calendar day");
}}

if (deferredWakeTime("2026-07-27") !== new Date(2026, 6, 27).getTime()) {{
  throw new Error("date-only deferrals should wake at local midnight");
}}
if (deferredWakeTime("2026-07-26T21:00:00.000Z") !== Date.parse("2026-07-26T21:00:00.000Z")) {{
  throw new Error("timestamp deferrals should wake at their exact instant");
}}
"#,
        deferral_helpers_source()
    );

    run_node(script);
}

#[test]
fn deferred_timestamps_are_displayed_as_local_human_readable_times() {
    let script = format!(
        r#"
function parseLocalDate(value) {{
  const [year, month, day] = value.split("-").map(Number);
  return new Date(year, month - 1, day);
}}
{}

Date.prototype.toLocaleString = function(locale, options) {{
  if (this.toISOString() !== "2026-08-12T18:59:36.485Z") {{
    throw new Error(`formatted the wrong instant: ${{this.toISOString()}}`);
  }}
  if (locale !== undefined || options.month !== "short" || options.day !== "numeric"
      || options.hour !== "numeric" || options.minute !== "2-digit") {{
    throw new Error("timestamp label should use the compact local date-time format");
  }}
  return "Aug 12, 2:59 PM";
}};

if (deferredUntilLabel("2026-08-12T18:59:36.485Z") !== "Aug 12, 2:59 PM") {{
  throw new Error("timestamp deferral was not made human readable");
}}
if (deferredUntilLabel("2026-08-12") !== "2026-08-12") {{
  throw new Error("date-only deferral should stay compact");
}}
"#,
        routine_filter_source()
    );

    run_node(script);
}

#[test]
fn cards_expose_the_defer_control_and_endpoint() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("class=\"card-defer\""));
    assert!(html.contains("hqFetch(\"/api/defer\""));
    assert!(html.contains("data-defer-preset=\"hour\""));
    assert!(html.contains("data-defer-preset=\"evening\""));
    assert!(html.contains("visibilityTimer = setTimeout(fetchProjects"));
    assert!(html.contains("deferred-search-result"));
    assert!(html.contains("Currently deferred until"));
    assert!(html.contains("card.draggable = !dimmed && !deferredSearchResult"));
    assert!(!html.contains(".card-defer-wrap:hover .card-defer-menu"));
    assert!(!html.contains(".card-defer-wrap:focus-within .card-defer-menu"));
    assert!(html.contains(".card-defer-wrap.open .card-defer-menu"));
    assert!(html.contains("function closeActionMenus(except = null)"));
    assert!(html.contains("aria-expanded=\"false\""));
    assert!(html.contains("actionButton.setAttribute(\"aria-expanded\", String(shouldOpen))"));
    assert!(!html.contains("id=\"defer-modal\""));
    assert!(html.contains("data-card-move"));
    assert!(html.contains("data-move-status"));
    assert!(html.contains("moveProjectStatus(p.file"));
    assert!(html.contains("class=\"card-defer card-status\""));
    assert!(html.contains("aria-label=\"Change status from"));
    assert!(
        html.contains("statuses.filter(status => ![\"submitted\", \"dropped\"].includes(status))")
    );
}

#[test]
fn status_menu_and_drag_prompt_when_waiting_on_is_required() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("async function requestProjectMove(payload)"));
    assert!(html.contains("data.code !== \"waiting_on_required\""));
    // In-page modal, not window.prompt: WKWebView has no prompt delegate, so
    // window.prompt silently returns null in the macOS app.
    assert!(html.contains("await promptWaitingOn(reason, people)"));
    assert!(html.contains("id=\"waiting-on-modal\""));
    assert!(html.contains("role=\"dialog\" aria-modal=\"true\""));
    assert!(html.contains("aria-labelledby=\"waiting-on-title\""));
    assert!(html.contains("type=\"button\" data-waiting-on-cancel"));
    assert!(!html.contains("window.prompt"));
    assert!(html.contains("request = { ...request, waiting_on: waitingOn };"));
    assert!(html.contains("requestProjectMove({ file, to_status: toStatus })"));
    assert!(html.contains("const moved = await requestProjectMove(payload);"));
    assert!(html.contains("to_status: moveStatusForDrop(draggedProject, targetStatus)"));
    assert!(html.contains("closeActionMenus();\n      moveButton.focus();"));

    let move_status = html
        .find("async function moveProjectStatus(file, toStatus)")
        .expect("static index should define project status moves");
    let move_status = &html[move_status..];
    let request = move_status
        .find("await requestProjectMove")
        .expect("status move should call the shared request helper");
    let optimistic_update = move_status
        .find("project.status = toStatus")
        .expect("successful status move should update the local project");
    assert!(
        request < optimistic_update,
        "the invoking control must remain connected while prompting"
    );
}

#[test]
fn waiting_on_modal_cancel_enter_does_not_submit() {
    let script = format!(
        r#"
const elements = {{}};
class FakeElement {{
  constructor(id) {{
    this.id = id;
    this.listeners = {{}};
    this.value = "";
    this.textContent = "";
    this.innerHTML = "";
    this.isConnected = true;
    this.classList = {{ add() {{}}, remove() {{}} }};
  }}
  addEventListener(type, handler) {{ this.listeners[type] = handler; }}
  removeEventListener(type, handler) {{
    if (this.listeners[type] === handler) delete this.listeners[type];
  }}
  focus() {{ document.activeElement = this; }}
  querySelectorAll(selector) {{
    if (selector === "[data-waiting-on-cancel]") return [elements.backdrop, elements.cancel];
    if (selector === "input:not([disabled]), button:not([disabled])") {{
      return [elements.input, elements.cancel, elements.submit];
    }}
    return [];
  }}
}}

for (const id of [
  "waiting-on-modal", "waiting-on-form", "waiting-on-input", "waiting-on-error",
  "waiting-on-reason", "waiting-on-people", "backdrop", "cancel", "submit", "trigger"
]) {{
  elements[id.replaceAll("-", "_")] = new FakeElement(id);
}}
elements["waiting-on-modal"] = elements.waiting_on_modal;
elements["waiting-on-form"] = elements.waiting_on_form;
elements["waiting-on-input"] = elements.waiting_on_input;
elements["waiting-on-error"] = elements.waiting_on_error;
elements["waiting-on-reason"] = elements.waiting_on_reason;
elements["waiting-on-people"] = elements.waiting_on_people;
elements.input = elements.waiting_on_input;

const document = {{
  activeElement: elements.trigger,
  getElementById(id) {{ return elements[id]; }}
}};
const esc = value => value;
{}

(async () => {{
  const result = promptWaitingOn("Pick a person", ["alex"]);
  elements.input.value = "journal";
  elements.waiting_on_modal.listeners.keydown({{
    key: "Enter",
    target: elements.cancel,
    preventDefault() {{ throw new Error("Cancel Enter must not be handled as submit"); }}
  }});
  elements.cancel.listeners.click();
  if (await result !== null) throw new Error("Cancel Enter submitted the modal");
  if (document.activeElement !== elements.trigger) throw new Error("focus was not restored");
}})().catch(error => {{ console.error(error); process.exit(1); }});
"#,
        prompt_waiting_on_source()
    );

    run_node(script);
}

#[test]
fn done_column_is_visually_deemphasized_until_interaction() {
    let html = include_str!("../static/index.html");

    assert!(html.contains(".column[data-status=\"done\"] .column-header"));
    assert!(html.contains(".column[data-status=\"done\"] .column-body"));
    assert!(html.contains(".column[data-status=\"done\"] .card {"));
    assert!(html.contains("filter: saturate(0.45)"));
    assert!(html.contains(".column[data-status=\"done\"] .card:hover"));
    assert!(html.contains(".column[data-status=\"done\"] .card.selected"));
    assert!(html.contains(".column[data-status=\"done\"] .card:focus-within"));
}

#[test]
fn card_text_wraps_unbroken_urls_inside_its_column() {
    let html = include_str!("../static/index.html");

    assert!(html.contains(".card-title,\n  .card-meta {"));
    assert!(html.contains("overflow-wrap: anywhere;"));
    assert!(html.contains(".card {\n    min-width: 0;"));
}

#[test]
fn main_app_exposes_a_repository_wide_agent_workspace() {
    let html = include_str!("../static/index.html");

    assert!(!html.contains("id=\"agent-view-btn\""));
    assert!(html.contains("class=\"app-workspace\""));
    assert!(html.contains("class=\"main-workspace\""));
    assert!(html.contains("id=\"agent-workspace\""));
    assert!(!html.contains("id=\"agent-context-select\""));
    assert!(!html.contains("id=\"agent-context-content\""));
    assert!(!html.contains("id=\"panel-ask-agent-btn\""));
    assert!(!html.contains(".agent-workspace.hidden"));
    assert!(html.contains("grid-template-columns: minmax(0, 1fr) clamp("));
    assert!(html.contains("contextFile: selectedFile"));
    assert!(html.contains("context_file: pending.contextFile"));
    let project_panel = html.find("id=\"panel-overlay\"").unwrap();
    let agent_panel = html.find("id=\"agent-workspace\"").unwrap();
    assert!(project_panel < agent_panel);
    assert!(html.contains("id=\"agent-transcript\""));
    assert!(html.contains("id=\"agent-input\""));
    assert!(html.contains("id=\"agent-stop-btn\""));
    assert!(html.contains("id=\"agent-clear-btn\""));
    assert!(html.contains("id=\"agent-model-menu-btn\""));
    assert!(html.contains("id=\"agent-model-menu\""));
    assert!(html.contains("id=\"agent-model-select\""));
    assert!(html.contains("id=\"agent-effort-select\""));
    assert!(!html.contains("class=\"agent-settings\""));
    assert!(!html.contains("agent-composer-hint"));
    assert!(!html.contains("⌘↩ send · edits apply automatically · ⌘Z undo"));
    assert!(html.contains("Agent edits apply automatically; use Undo to revert."));
    assert!(html.contains("agentFetch(\"/api/agent/models\""));
    assert!(html.contains("agentFetch(\"/api/agent/settings?thread_id=\""));
    assert!(html.contains("supportedReasoningEfforts"));
    assert!(html.contains("defaultReasoningEffort"));
    assert!(!html.contains("id=\"agent-apply-btn\""));
    assert!(!html.contains("id=\"agent-reject-btn\""));
    assert!(html.contains("new EventSource("));
    assert!(html.contains("/api/agent/events?thread_id="));
    assert!(html.contains("agentFetch(\"/api/agent/turn\""));
    assert!(html.contains("agentFetch(\"/api/agent/apply?thread_id=\""));
    assert!(html.contains("async function autoApplyAgentChange(state = agentState)"));
    assert!(html.contains("await Promise.all([fetchProjects(), fetchTasks(), fetchRoutines()]);"));
    let apply = html
        .split("async function autoApplyAgentChange(state = agentState)")
        .nth(1)
        .unwrap()
        .split("// --- Standalone tasks ---")
        .next()
        .unwrap();
    assert!(apply.contains("const openFile = selectedFile;"));
    assert!(apply.contains("await openPanel(openFile);"));
    assert!(html.contains("cache: \"no-store\""));
    assert!(html.contains("Applied ${count} HQ file"));
    assert!(!html.contains("agentFetch(\"/api/agent/reject\""));
    assert!(!html.contains("Undo is available."));
    assert!(html.contains(".agent-message.user"));
    assert!(html.contains("white-space: pre-wrap"));
    assert!(html.contains("const message = input.value;"));
    assert!(html.contains("if (!message.trim()"));
    assert!(html.contains("function clearAgentConversation()"));
    assert!(html.contains("state.entries.filter(entry => entry.queued || entry.streaming)"));
    assert!(html.contains("state.itemEntries = new Map("));
    assert!(html.contains("!state.entries.some(entry => !entry.queued && !entry.streaming)"));
    assert!(!html.contains("Clear this Agent conversation and start a new session?"));
    assert!(!html.contains("agentFetch(\"/api/agent/session\", { method: \"DELETE\" })"));
    assert!(html.contains(
        "fetchProjects();\nfetchTasks();\nfetchRoutines();\nensureAgentSession();\nconnectSSE();"
    ));
    assert!(!html.contains("id=\"panel-agent-tab\""));
    assert!(!html.contains("id=\"panel-agent-view\""));
}

#[test]
fn switching_views_refreshes_the_selected_data_source() {
    let html = include_str!("../static/index.html");
    assert!(html.contains("if (view === \"board\") void fetchProjects();"));
    assert!(html.contains("else if (view === \"tasks\") void fetchTasks();"));
    assert!(html.contains("else if (view === \"routines\") void fetchRoutines();"));
    assert!(html.contains("else if (view === \"timeline\") void fetchTimeline();"));
}

#[test]
fn clearing_agent_chat_removes_finished_messages_without_dropping_live_work() {
    let script = format!(
        r#"
{}

let rendered = false;
function renderAgentView() {{ rendered = true; }}
const finishedAgent = {{ kind: "agent", itemId: "finished", text: "Done" }};
const streamingAgent = {{ kind: "agent", itemId: "streaming", text: "Working", streaming: true }};
const queuedUser = {{ kind: "user", text: "Next", queued: true }};
const agentState = {{
  entries: [
    {{ kind: "user", text: "Old prompt" }},
    finishedAgent,
    streamingAgent,
    queuedUser,
  ],
  itemEntries: new Map([
    ["finished", finishedAgent],
    ["streaming", streamingAgent],
  ]),
}};

clearAgentConversation();
if (!rendered) throw new Error("clear did not render");
if (agentState.entries.length !== 2) throw new Error("finished messages remain");
if (!agentState.entries.includes(streamingAgent)) throw new Error("streaming response was dropped");
if (!agentState.entries.includes(queuedUser)) throw new Error("queued prompt was dropped");
if (agentState.itemEntries.has("finished")) throw new Error("finished item lookup remains");
if (agentState.itemEntries.get("streaming") !== streamingAgent) {{
  throw new Error("streaming item lookup was not preserved");
}}
"#,
        clear_agent_conversation_source()
    );
    run_node(script);
}

#[test]
fn todo_context_preserves_drafts_and_stays_with_queued_messages() {
    let html = include_str!("../static/index.html");
    let composer = html
        .split("function attachTodoContext(")
        .nth(1)
        .unwrap()
        .split("async function stopAgentTurn(")
        .next()
        .unwrap();
    let entry = html
        .split("function renderAgentEntry(")
        .nth(1)
        .unwrap()
        .split("function renderAgentView(")
        .next()
        .unwrap();
    let script = r#"
const assert = require('node:assert/strict');
let selectedTaskContext = null, selectedFile = 'research/project.md';
const elements = new Map();
const document = { getElementById(id) {
  if (!elements.has(id)) elements.set(id, { value: '', focus() {} });
  return elements.get(id);
} };
function selectedProject() { return { title: 'Project' }; }
function renderAgentView() {}
function setAgentActivity() {}
function clearAgentActivity() {}
function esc(text) { return String(text).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;'); }
const agentState = { threadId: 'thread', busy: true, entries: [], queue: [] };
const requests = [];
async function agentFetch(url, options) {
  requests.push(JSON.parse(options.body));
  return { turn_id: 'turn' };
}
const input = document.getElementById('agent-input');
input.value = 'What should I do about this?';
attachTodoContext({ line: 3, raw: '2026-10-18 Review <draft> p:9', title: 'Review <draft>' });
assert.equal(input.value, 'What should I do about this?', 'attachment must preserve the draft');
assert.equal(document.getElementById('agent-context').hidden, false);
assert.equal(document.getElementById('agent-context-label').textContent, 'Todo: Review <draft>');
sendAgentMessage();
assert.equal(requests.length, 0, 'busy agent should queue');
assert.equal(agentState.queue.length, 1);
assert.equal(selectedTaskContext, null);
assert.equal(document.getElementById('agent-context').hidden, true);
assert.equal(input.placeholder, 'Ask about Project…');
assert.ok(renderAgentEntry(agentState.entries[0]).includes('Todo: Review &lt;draft&gt;'));

// A later selection and navigation must not retarget an already queued request.
attachTodoContext({ line: 1, raw: 'Different todo', title: 'Different todo' });
selectedFile = 'another.md';
agentState.busy = false;
(async () => {
  await processAgentQueue();
  assert.deepEqual(requests, [{
    thread_id: 'thread',
    message: 'What should I do about this?',
    context_file: 'research/project.md',
    context_task: { line: 3, expected: '2026-10-18 Review <draft> p:9' },
  }]);
  assert.equal(selectedTaskContext.title, 'Different todo');
})().catch(error => { console.error(error); process.exitCode = 1; });
"#;
    run_node(format!(
        "function attachTodoContext({composer}\nfunction renderAgentEntry({entry}\n{script}"
    ));
}

#[test]
fn agent_composer_queues_messages_while_a_turn_runs() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("queue: []"));
    assert!(html.contains("dispatching: false"));
    assert!(html.contains("state.queue.push({"));
    assert!(html.contains("contextFile: selectedFile"));
    assert!(html.contains("async function processAgentQueue(state = agentState)"));
    assert!(html.contains("const pending = state.queue.shift();"));
    assert!(html.contains("message: pending.message"));
    assert!(html.contains("context_file: pending.contextFile"));
    assert!(html.contains("void processAgentQueue();"));
    assert!(html.contains(
        "send.textContent = state.busy || state.applying || state.dispatching ? \"Queue\" : \"Send\""
    ));
    assert!(html.contains("input.disabled = !state.threadId || state.configuring;"));
    assert!(!html.contains("input.disabled = !state.threadId || state.busy || state.applying"));
    assert!(html.contains("class=\"agent-queued-label\">Queued"));
    assert!(html.contains(".agent-message.user.queued"));
}

#[test]
fn agent_activity_stays_visible_during_long_turns_without_showing_tool_details() {
    let html = include_str!("../static/index.html");
    assert!(html.contains("id=\"agent-running-indicator\""));
    assert!(html.contains("class=\"agent-spinner\""));
    assert!(html.contains("@keyframes agent-spin"));
    assert!(html.contains("const isRunning = state.busy || state.applying || state.dispatching;"));
    assert!(html.contains("indicator.hidden = !isRunning;"));
    assert!(html.contains("setAgentActivity(\"Starting\", state)"));
    assert!(html.contains("setAgentActivity(\"Thinking\", state)"));
    assert!(html.contains("setAgentActivity(\"Working\", state)"));
    assert!(html.contains("setAgentActivity(\"Writing\", state)"));
    assert!(html.contains("setAgentActivity(\"Applying edits\", state)"));
    assert!(!html.contains("item.command"));

    let script = format!(
        r#"
{}
if (formatAgentElapsed(1_000, 46_999) !== "45s") throw new Error("seconds");
if (formatAgentElapsed(1_000, 126_999) !== "2m 5s") throw new Error("minutes");
"#,
        agent_elapsed_source()
    );
    run_node(script);

    let indicator_script = format!(
        r#"
{}
{}
const indicator = {{
  hidden: true,
  title: "",
  label: "",
  setAttribute(name, value) {{ if (name === "aria-label") this.label = value; }},
}};
const document = {{
  getElementById(id) {{
    if (id !== "agent-running-indicator") throw new Error("wrong element");
    return indicator;
  }},
}};

let agentState = {{ activityStartedAt: null, activityLabel: "" }};
updateAgentRunningIndicator({{ busy: false, applying: false, dispatching: false }});
if (!indicator.hidden) throw new Error("idle indicator is visible");
agentState = {{ activityStartedAt: null, activityLabel: "Thinking", applying: false }};
updateAgentRunningIndicator({{ busy: true, applying: false, dispatching: false, activityLabel: "Thinking" }});
if (indicator.hidden || indicator.label !== "Agent is thinking" || indicator.title !== "Thinking…") throw new Error("busy indicator is hidden");
agentState = {{ activityStartedAt: null, activityLabel: "", applying: true }};
updateAgentRunningIndicator({{ busy: false, applying: true, dispatching: false }});
if (indicator.hidden || indicator.title !== "Applying edits…") throw new Error("apply indicator is hidden");
"#,
        agent_elapsed_source(),
        agent_running_indicator_source()
    );
    run_node(indicator_script);
}

#[test]
fn intraday_routines_sort_and_read_by_the_clock() {
    let script = format!(
        r#"
function parseLocalDate(value) {{
  const [year, month, day] = value.split("-").map(Number);
  return new Date(year, month - 1, day);
}}
function startOfLocalDay(value) {{
  return new Date(value.getFullYear(), value.getMonth(), value.getDate());
}}
function daysSince() {{ return 1; }}
{}

{}

let searchQuery = "";
const routines = [
  {{ title: "Later today", timing: "upcoming", next_due: "2026-07-15 16:00" }},
  {{ title: "Sooner today", timing: "upcoming", next_due: "2026-07-15 14:00" }},
  {{ title: "Date only", timing: "available", next_due: "2026-07-16" }},
];
const order = routinesForTimeline().map(routine => routine.title).join(",");
if (order !== "Sooner today,Later today,Date only") {{
  throw new Error(`Unexpected routine order: ${{order}}`);
}}

const now = new Date("2026-07-15T12:00:00-04:00");
const horizon = routineHorizon(routines[1], now);
if (horizon !== "Today") throw new Error(`Unexpected horizon: ${{horizon}}`);

const dueLabel = routineDateLabel({{ timing: "due", next_due: "2026-07-15 14:00" }});
if (dueLabel === "Due today" || !dueLabel.startsWith("Due ")) {{
  throw new Error(`Timestamped routine should show a clock time: ${{dueLabel}}`);
}}
const dateLabel = routineDateLabel({{ timing: "due", next_due: "2026-07-15" }});
if (dateLabel !== "Due today") throw new Error(`Unexpected date label: ${{dateLabel}}`);
"#,
        routine_filter_source(),
        routine_label_source()
    );
    run_node(script);

    let html = include_str!("../static/index.html");
    assert!(html.contains("<option value=\"hour\">hour</option>"));
    assert!(html.contains("function syncRoutineDueInput("));
    assert!(html.contains(
        "document.getElementById(\"routine-repeat-unit\").addEventListener(\"change\", syncRoutineDueInput);"
    ));
}

#[test]
fn routines_use_one_chronological_upcoming_view() {
    let script = format!(
        r#"
function parseLocalDate(value) {{
  const [year, month, day] = value.split("-").map(Number);
  return new Date(year, month - 1, day);
}}
function startOfLocalDay(value) {{
  return new Date(value.getFullYear(), value.getMonth(), value.getDate());
}}
{}

let searchQuery = "";
const routines = [
  {{ title: "Overdue", timing: "overdue", next_due: "2026-07-14" }},
  {{ title: "Available", timing: "available", next_due: "2026-07-25" }},
  {{ title: "Today", timing: "deferred", deferred_until: "2026-07-15T17:00:00-04:00", next_due: "2026-07-15" }},
  {{ title: "Week", timing: "upcoming", available_on: "2026-07-19", next_due: "2026-07-19" }},
  {{ title: "Month", timing: "upcoming", available_on: "2026-07-25", next_due: "2026-07-25" }},
  {{ title: "Season", timing: "upcoming", available_on: "2026-08-31", next_due: "2026-08-31" }},
  {{ title: "Year", timing: "upcoming", available_on: "2026-10-01", next_due: "2026-10-01" }},
  {{ title: "Later", timing: "upcoming", available_on: "2027-01-01", next_due: "2027-01-01" }},
];
const now = new Date("2026-07-15T12:00:00-04:00");
const horizons = routines.map(routine => routineHorizon(routine, now)).join(",");
if (horizons !== "Now,Now,Today,This week,This month,This season,This year,Later") {{
  throw new Error(`Unexpected routine horizons: ${{horizons}}`);
}}
if (routinesForTimeline().length !== 8) throw new Error("Timeline omitted routines");
"#,
        routine_filter_source()
    );
    run_node(script);

    let html = include_str!("../static/index.html");
    assert!(html.contains("id=\"routine-view-btn\""));
    assert!(html.contains("id=\"routines-view\""));
    assert!(!html.contains("routineScope"));
    assert!(!html.contains("renderRoutineScopeTabs"));
    assert!(html.contains("function routineHorizon("));
    assert!(html.contains("This season"));
    assert!(html.contains("This year"));
    assert!(html.contains(
        "const routineHorizons = [\"Now\", \"Today\", \"This week\", \"This month\", \"This season\", \"This year\", \"Later\"];"
    ));
    assert!(html.contains("const groupedRoutines = groups.get(horizon) || [];"));
    assert!(html.contains(".routine-row.upcoming,"));
    assert!(html.contains("opacity: 0.22;"));
    assert!(html.contains(".header-top {"));
    assert!(html.contains("min-height: 43px;"));
    assert!(html.contains("min-height: 29px;"));
    assert!(html.contains("font-size: 24px;"));
    assert!(!html.contains("id=\"total-count\""));
    assert!(html.contains("id=\"new-btn\">+ Todo</button>"));
    assert!(html.contains("if (activeView === \"routines\") openRoutineModal();"));
    assert!(!html.contains("document.getElementById(\"mini-timeline\").classList.toggle"));
    assert!(html.contains("hqFetch(\"/api/routines\""));
    assert!(html.contains("\"/api/routine/complete\""));
    assert!(html.contains("\"/api/routine/skip\""));
    assert!(html.contains("\"/api/routine/defer\""));
    assert!(html.contains("data-routine-defer-preset=\"hour\""));
    assert!(html.contains("data-routine-defer-preset=\"evening\""));
    assert!(html.contains("Custom routine defer date and time"));
    assert!(html.contains("scheduleRoutineVisibilityRefresh();"));
    assert!(!html.contains("routine-group-pending"));
}

#[test]
fn standalone_tasks_use_todo_txt_and_numeric_drag_priority() {
    let script = format!(
        r#"
{}
const items = [
  {{ line: 0, priority: 300 }},
  {{ line: 1, priority: 200 }},
  {{ line: 2, priority: 100 }},
];
if (taskPriorityForDrop(items, 3, 0) !== 0) {{
  throw new Error("moving the first task to the bottom should assign p:0");
}}
if (taskPriorityForDrop(items, 0, 2) !== 400) {{
  throw new Error("moving the last task to the top should assign p:400");
}}
if (taskPriorityForDrop(items, 2, 0) !== 150) {{
  throw new Error("moving between tasks should assign a midpoint priority");
}}
"#,
        task_priority_source()
    );
    run_node(script);

    let html = include_str!("../static/index.html");
    assert!(html.contains("id=\"task-view-btn\""));
    assert!(html.contains("aria-label=\"Todos\""));
    assert!(html.contains("id=\"tasks-view\""));
    assert!(html.contains("id=\"new-btn\">+ Todo</button>"));
    assert!(
        html.contains("if (activeView === \"tasks\" || activeView === \"today\") openTaskModal();")
    );
    assert!(html.contains("id=\"task-modal\""));
    assert!(html.contains("hqFetch(\"/api/tasks\""));
    assert!(html.contains("\"/api/task/complete\""));
    assert!(html.contains("\"/api/task/defer\""));
    assert!(html.contains("\"/api/task/priority\""));
    assert!(html.contains("p:${esc(task.priority)}"));
    assert!(html.contains("taskScope = \"available\""));
}

#[test]
fn analysis_load_excludes_waiting_and_pulses_share_a_window() {
    let html = include_str!("../static/index.html");
    assert!(html.contains("const LOAD_STATUSES = new Set([\"my-plate\", \"active\"]);"));
    assert!(html.contains("const LOAD_STATUS_ORDER = [\"my-plate\", \"active\"];"));
    assert!(html.contains("const ANALYSIS_STATUS_ORDER = [...LOAD_STATUS_ORDER, \"waiting\"];"));
    assert!(html.contains("const pulseSnapshots = miniTimelineSnapshots(snapshots);"));
    assert!(
        html.contains("[\"Load\", miniPulseTitle(\"load\"), pulseSnapshots, maxLoad, \"load\"]")
    );
    assert!(html.contains("renderMiniPulseGroup(\"waiting\""));
    assert!(html.contains("[\"Waiting\", miniPulseTitle(\"waiting\")"));
    assert!(html.contains("if (kind === \"waiting\") return STATUS_COLORS.waiting;"));
    assert!(html.contains("${waitingCount(snapshot)} waiting"));
    assert!(html.contains("${snapshots.length} daily columns from ${source}"));
    assert!(html.contains("day.className = \"timeline-axis-day\";"));
    assert!(html.contains("timelineAxisTickIndexes(snapshots)"));
    assert!(html.contains("date.getDate() === 1"));
    assert!(html.contains("const minimumEndpointGap = 8;"));
}

#[test]
fn agent_transcript_hides_tool_activity() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("indicator.title = `${label}…${elapsed ? ` ${elapsed}` : \"\"}`;"));
    assert!(html.contains(".agent-status[hidden]"));
    assert!(!html.contains("agentActivityMarkup"));
    assert!(!html.contains("kind: \"activity\""));
    assert!(!html.contains(".agent-message.activity"));
    assert!(!html.contains("item.aggregatedOutput"));
    assert!(!html.contains("item.command ||"));
    assert!(html.contains("item.type === \"agentMessage\""));
    assert!(html.contains("kind: \"notice\""));
}

#[test]
fn project_focus_has_clear_navigation_and_editable_metadata() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("id=\"home-btn\""));
    assert!(html.contains("title=\"Return to Today\""));
    assert!(html.contains("id=\"panel-close\">← Projects"));
    assert!(html.contains("id=\"panel-move-btn\""));
    assert!(html.contains("id=\"panel-defer-btn\""));
    assert!(html.contains("data-panel-move-status"));
    assert!(html.contains("data-panel-defer-preset=\"hour\""));
    assert!(html.contains("data-panel-defer-preset=\"evening\""));
    assert!(html.contains("id=\"panel-defer-custom\""));
    assert!(html.contains("async function deferSelectedProject(until, label)"));
    assert!(html.contains("closePanel();\n  await applyDeferral(file, until, label);"));
    assert!(html.contains("id=\"panel-more-btn\""));
    assert!(html.contains("class=\"card-defer-menu panel-more-menu\""));
    assert!(html.contains("id=\"panel-metadata-btn\""));
    let more_menu = html
        .find("class=\"card-defer-menu panel-more-menu\"")
        .unwrap();
    let edit_details = html.find("id=\"panel-metadata-btn\"").unwrap();
    assert!(more_menu < edit_details);
    assert!(html.contains("async function saveMetadata()"));
    assert!(html.contains("hqFetch(\"/api/metadata\""));
    assert!(html.contains("document.getElementById(\"home-btn\").addEventListener"));
    assert!(html.contains("[\"parallel\", \"Parallel\"]"));
    assert!(html.contains("[\"serial\", \"Serial\"]"));
    assert!(!html.contains("[\"single\", \"Single\"]"));
    assert!(!html.contains("[\"sequential\", \"Sequential\"]"));
}

#[test]
fn next_action_is_body_derived_and_read_only() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("id=\"panel-next-action\""));
    assert!(html.contains("function availableBodyAction(project)"));
    assert!(html.contains("action.source === \"checklist\" && action.available"));
    assert!(html.contains("function legacyNextAction(project)"));
    assert!(html.contains("availableBodyAction(project) || legacyNextAction(project)"));
    assert!(html.contains("function renderProjectNextAction(project)"));
    assert!(html.contains("No available checklist item"));
    assert!(!html.contains("id=\"panel-next-action-edit\""));
    assert!(!html.contains("id=\"panel-next-action-input\""));
    assert!(!html.contains("saveNextAction"));
    assert!(!html.contains("/api/action"));
    assert!(html.contains("const nextAction = nextActionText(p);"));
    assert!(!html.contains("id=\"np-my-next\""));
    assert!(!html.contains("metadataField(\"Next action\", \"my_next\""));
}

#[test]
fn markdown_project_links_do_not_replace_the_app() {
    let script = format!(
        r##"
{}

const cases = [
  ["yy-home-electrification.md", "personal/yy-bike-shelter.md", "personal/yy-home-electrification.md"],
  ["../funding/grant.md#notes", "personal/task.md", "funding/grant.md"],
  ["/personal/file%20name.md", "research/source.md", "personal/file name.md"],
  ["https://example.com/file.md", "research/source.md", null],
  ["#section", "research/source.md", null],
  ["notes.txt", "research/source.md", null],
];
for (const [href, currentFile, expected] of cases) {{
  const actual = resolveMarkdownProjectLink(href, currentFile);
  if (actual !== expected) {{
    throw new Error(`${{href}} resolved to ${{actual}}, expected ${{expected}}`);
  }}
}}
"##,
        markdown_link_source()
    );
    run_node(script);

    let html = include_str!("../static/index.html");
    assert!(html.contains("wireRenderedLinks(content, selectedFile);"));
    assert!(html.contains("openPanel(projectFile);"));
    assert!(html.contains("Project link not found: ${projectFile}"));
    assert!(html.contains("link.target = \"_blank\""));

    let swift = include_str!("../macos/HQDesktop/HQDesktopApp.swift");
    assert!(
        swift.contains("navigationAction.navigationType == .linkActivated && url.path != \"/\"")
    );
    assert!(swift.contains("decisionHandler(.cancel)"));
}

#[test]
fn dashboard_exposes_revision_safe_undo() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("id=\"undo-btn\""));
    assert!(html.contains("hqFetch(\"/api/undo\""));
    assert!(html.contains("async function undoLastAction()"));
    assert!(html.contains("e.key.toLowerCase() === \"z\""));
    assert!(html.contains("!typing"));
    assert!(!html.contains("discardAgentState()"));
}

#[test]
fn study_and_writing_have_visibly_distinct_track_colors() {
    let script = format!(
        r#"
{}

assignTrackColors([
  "admin", "funding", "lab", "personal", "research", "service",
  "side-projects", "study", "teaching", "trip", "writing",
]);

const study = trackColorMap.study.match(/[0-9a-f]{{2}}/gi).map(value => parseInt(value, 16));
const writing = trackColorMap.writing.match(/[0-9a-f]{{2}}/gi).map(value => parseInt(value, 16));
const distance = Math.sqrt(study.reduce((sum, value, index) => sum + (value - writing[index]) ** 2, 0));
if (distance < 100) {{
  throw new Error(`study and writing colors remain too similar: ${{distance}}`);
}}
"#,
        track_colors_source()
    );

    run_node(script);
}

#[test]
fn desktop_auth_token_is_forwarded_to_http_and_event_streams() {
    let html = include_str!("../static/index.html");
    assert!(html.contains("new URLSearchParams(window.location.search).get(\"hq_token\")"));
    assert!(html.contains("headers.set(\"X-HQ-Token\", HQ_AUTH_TOKEN)"));
    assert!(html.contains("function hqEventSource(url)"));
    assert!(html.contains("hq_token=${encodeURIComponent(HQ_AUTH_TOKEN)}"));
    assert!(html.contains("history.replaceState("));
}

#[test]
fn today_includes_due_work_and_available_routines_across_tracks() {
    let html = include_str!("../static/index.html");
    let today_source = html.split("function dueByToday(").nth(1).unwrap();
    let today_source = format!(
        "function dueByToday({}",
        today_source
            .split("\n\nfunction refreshToday")
            .next()
            .unwrap()
    );
    let task_search = html.split("function taskMatchesSearch(").nth(1).unwrap();
    let task_search = format!(
        "function taskMatchesSearch({}",
        task_search
            .split("\n\nfunction tasksForScope")
            .next()
            .unwrap()
    );
    let script = format!(
        r#"
{}
{}
{}
{}
{}
{}
let activeTrack = "research";
let taskScope = "deferred";
let searchQuery = "";
const OUTFLOW_STATUSES = new Set(["submitted", "done", "dropped"]);
const now = new Date(2026, 9, 18, 0, 15);
const projects = [
  {{ title: "Today", track: "personal", status: "active", deadline: "2026-10-18", priority: 10 }},
  {{ title: "Overdue", track: "research", status: "waiting", deadline: "2026-10-17 (review)", priority: 20 }},
  {{ title: "Tomorrow", status: "active", deadline: "2026-10-19" }},
  {{ title: "Deferred", status: "active", deadline: "2026-10-17", visible: false }},
  ...["submitted", "done", "dropped"].map(status => ({{ title: status, status, deadline: "2026-10-17" }})),
  {{ title: "Undated", status: "active" }},
  {{ title: "Invalid", status: "active", deadline: "2026-02-30" }},
];
const tasks = [
  {{ text: "Today todo", visible: true, due: "2026-10-18" }},
  {{ text: "Overdue todo", visible: true, due: "2026-10-01" }},
  {{ text: "Waiting todo", visible: true, waiting: true, due: "2026-10-18" }},
  {{ text: "Deferred todo", visible: false, due: "2026-10-01" }},
  {{ text: "Future todo", visible: true, due: "2026-10-19" }},
  {{ text: "Undated todo", visible: true }},
];
const routines = ["available", "due", "overdue", "upcoming", "deferred"].map(timing => ({{
  title: timing + " routine", timing, next_due: "2026-10-18",
}}));
const assert = require("node:assert/strict");
let result = todayItems(now);
assert.deepEqual(result.projects.map(p => p.title), ["Overdue", "Today"]);
assert.deepEqual(result.tasks.map(t => t.text), ["Today todo", "Overdue todo", "Waiting todo"]);
assert.deepEqual(result.routines.map(r => r.timing).sort(), ["available", "due", "overdue"]);
searchQuery = "overdue";
result = todayItems(now);
assert.deepEqual(result.projects.map(p => p.title), ["Overdue"]);
assert.deepEqual(result.tasks.map(t => t.text), ["Overdue todo"]);
assert.deepEqual(result.routines.map(r => r.timing), ["overdue"]);
assert.equal(dueByToday("2026-10-19", now), false);
assert.equal(dueByToday("2026-10-19", new Date(2026, 9, 19, 0, 1)), true);
assert.equal(dueByToday("not a date", now), false);
"#,
        days_since_source(),
        deferral_helpers_source(),
        get_column_items_source(),
        routine_filter_source(),
        task_search,
        today_source,
    );
    run_node(script);
}

#[test]
fn overviews_show_provenance_and_ignore_late_project_responses() {
    let html = include_str!("../static/index.html");
    let source = html
        .split("// --- Project overviews ---")
        .nth(1)
        .unwrap()
        .split("// --- Side panel ---")
        .next()
        .unwrap();
    let script = r#"
const assert = require('node:assert/strict');
let projectOverviews = {}, selectedFile = 'a.md', overviewTimer = null, overviewRequestId = 0, panelLoadId = 0;
let editMode = false, metadataEditMode = false, originalBody = '';
function esc(text) { return String(text).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;'); }
const section = { text: 'A concise <overview>', evidence: ['<script>untrusted()</script>'] };
const cached = { generated_at: '2026-10-18', overview: { what_is: section, situation: section, needs_you: section, action_kind: 'suggested' } };
let markup = overviewMarkup({ cached, stale: true, generating: true });
assert.ok(markup.includes('Stale · Refreshing'));
assert.ok(markup.includes('What needs me · Suggested'));
assert.ok(markup.includes('Saved source excerpts'));
assert.ok(markup.includes('&lt;script&gt;'));
assert.ok(!markup.includes('<script>'));
assert.ok(markup.includes('data-overview-refresh disabled'));
markup = overviewMarkup({ cached, stale: true, error: 'Offline' });
assert.ok(markup.includes('>Retry</button>'));
assert.ok(markup.includes('Offline'));
assert.ok(markup.includes('A concise'));
assert.equal(overviewSnippet('missing.md'), '');
projectOverviews['a.md'] = { cached, stale: true };
assert.ok(overviewSnippet('a.md').includes('AI · Stale'));

let rendered = 0;
renderOverview = () => { rendered++; };
const requests = [];
function hqFetch(url, options) { return new Promise(resolve => requests.push({ url, options, resolve })); }
const response = data => ({ ok: true, json: async () => data });
const elements = new Map();
const document = { getElementById(id) {
  if (!elements.has(id)) elements.set(id, {});
  return elements.get(id);
} };
function renderBody(el, body) { el.textContent = body; }
function selectedProject() { return null; }

(async () => {
  const oldRequest = refreshOverview();
  selectedFile = 'b.md';
  const newRequest = refreshOverview();
  requests[1].resolve(response({ revision: 'b', cached, stale: false, generating: false }));
  await newRequest;
  const rendersBeforeOldResult = rendered;
  requests[0].resolve(response({ revision: 'a', cached: null, generating: false }));
  await oldRequest;
  assert.equal(projectOverviews['b.md'].revision, 'b');
  assert.equal(projectOverviews['a.md'].cached, cached);
  assert.equal(rendered, rendersBeforeOldResult);

  const oldNotes = refreshProjectNotes('b.md');
  selectedFile = 'c.md';
  const newNotes = refreshProjectNotes('c.md');
  requests[3].resolve(response({ body: 'C notes' }));
  await newNotes;
  requests[2].resolve(response({ body: 'B notes' }));
  await oldNotes;
  assert.equal(originalBody, 'C notes');
  assert.equal(document.getElementById('panel-content').textContent, 'C notes');

  const pendingNotes = refreshProjectNotes('c.md');
  editMode = true;
  requests[4].resolve(response({ body: 'External edit' }));
  await pendingNotes;
  assert.equal(originalBody, 'C notes', 'external refresh must not overwrite an active editor');

  // An older revision can fail while a new revision is waiting for its slot.
  // There is no current cache or current error; start generation for this revision.
  selectedFile = 'd.md';
  const obsoleteFailure = refreshOverview(false, true);
  requests.at(-1).resolve(response({ cached: null, stale: false, generating: false, error: null }));
  await obsoleteFailure;
  assert.equal(requests.at(-1).options.method, 'POST');
  requests.at(-1).resolve(response({ cached, stale: false, generating: false, error: null }));
})().catch(error => { console.error(error); process.exitCode = 1; });
"#;
    run_node(format!("{source}\n{script}"));
}

#[test]
fn parallel_chats_keep_drafts_responses_and_applies_with_their_own_thread() {
    let html = include_str!("../static/index.html");
    let source = html
        .split("// --- Agent workspace ---")
        .nth(1)
        .unwrap()
        .split("// --- Standalone tasks ---")
        .next()
        .unwrap();
    let script = r#"
const assert = require('node:assert/strict');
let selectedTaskContext = null, selectedFile = null, agentState, agentConversations = [];
let editMode = false, metadataEditMode = false, projects = [];
const elements = new Map();
const document = {
  getElementById(id) {
    if (!elements.has(id)) elements.set(id, { value: '', textContent: '', innerHTML: '',
      classList: { contains() { return false; }, remove() {} },
      focus() {}, addEventListener() {}, setAttribute() {} });
    return elements.get(id);
  }, querySelectorAll() { return []; },
};
function selectedProject() { return null; }
function esc(s) { return String(s).replaceAll('<', '&lt;'); }
const marked = { parse: text => text };
renderAgentConversations = () => {};
renderAgentSettings = () => {};
updateAgentRunningIndicator = () => {};
const a = { ...createAgentState(), threadId: 'a', title: 'A' };
const b = { ...createAgentState(), threadId: 'b', title: 'B' };
agentState = a;
agentConversations = [a, b];
const input = document.getElementById('agent-input');
input.value = 'Draft A';
selectedTaskContext = { title: 'Todo A', line: 4, expected: 'Todo A' };
selectAgentConversation(b);
assert.equal(a.draft, 'Draft A');
assert.equal(input.value, '');
assert.equal(selectedTaskContext, null);
input.value = 'Draft B';
selectAgentConversation(a);
assert.equal(input.value, 'Draft A');
assert.equal(selectedTaskContext.title, 'Todo A');

const calls = [];
agentFetch = (url, options) => new Promise(resolve => calls.push({ url, options, resolve }));
function pending(message) { return { message, entry: { queued: true }, contextFile: null, contextTask: null }; }
a.queue.push(pending('Request A'));
b.queue.push(pending('Request B'));
(async () => {
  const runningA = processAgentQueue(a);
  selectAgentConversation(b);
  const runningB = processAgentQueue(b);
  assert.equal(calls.length, 2, 'both chats can dispatch concurrently');
  assert.deepEqual(calls.map(call => JSON.parse(call.options.body).thread_id), ['a', 'b']);
  calls[1].resolve({ turn_id: 'turn-b' });
  calls[0].resolve({ turn_id: 'turn-a' });
  await Promise.all([runningA, runningB]);
  handleAgentEvent({ method: 'item/agentMessage/delta', params: { itemId: 'same-id', delta: 'Answer A' } }, a);
  handleAgentEvent({ method: 'item/agentMessage/delta', params: { itemId: 'same-id', delta: 'Answer B' } }, b);
  assert.equal(a.entries.at(-1).text, 'Answer A');
  assert.equal(b.entries.at(-1).text, 'Answer B');
  assert.equal(input.value, 'Draft B');
  assert.equal(document.getElementById('agent-chat-title').textContent, 'B');
  assert.ok(!document.getElementById('agent-transcript').innerHTML.includes('Answer A'));

  // Background completion requests the original chat's diff, even after switching.
  const apply = autoApplyAgentChange(a);
  assert.equal(calls.at(-1).url, '/api/agent/change?thread_id=a');
  calls.at(-1).resolve({ changed: false });
  await apply;
  assert.equal(document.getElementById('agent-chat-title').textContent, 'B');
  assert.equal(input.value, 'Draft B');

  // Failed edits pause this chat's queue until explicitly resolved.
  a.busy = false;
  a.applyBlocked = true;
  a.queue.push(pending('Must wait'));
  const before = calls.length;
  await processAgentQueue(a);
  assert.equal(calls.length, before);
})().catch(error => { console.error(error); process.exitCode = 1; });
"#;
    let (prefix, body) = script.split_once("renderAgentConversations =").unwrap();
    run_node(format!(
        "{prefix}\n{source}\nrenderAgentConversations ={body}"
    ));
}

#[test]
fn settings_own_the_default_and_summary_charts_do_not_navigate() {
    let html = include_str!("../static/index.html");
    assert!(html.contains("id=\"settings-btn\""));
    assert!(html.contains("id=\"default-model-select\""));
    assert!(!html.contains("overview-model-select"));
    assert!(!html.contains("Toggle analysis view"));
    assert!(!html.contains("document.getElementById(\"mini-timeline\").addEventListener(\"click\""));
    assert!(html.contains("id=\"timeline-view-btn\""));
    let source = html
        .split("async function saveHQSettings() {")
        .nth(1)
        .unwrap()
        .split("document.getElementById(\"settings-btn\").addEventListener")
        .next()
        .unwrap();
    run_node(format!(
        r#"
const assert = require('node:assert/strict');
const agentState = {{ model: 'chat-override', reasoningEffort: 'high' }};
const elements = {{ 'default-model-select': {{ value: 'app-default' }}, 'default-effort-select': {{ value: 'low' }}, 'settings-save': {{}} }};
const document = {{ getElementById: id => elements[id] }};
let saved;
async function agentFetch(url, options) {{ assert.equal(url, '/api/ai/settings'); saved = JSON.parse(options.body); }}
function closeHQSettings() {{}}
async function fetchProjects() {{}}
function toast() {{}}
async function saveHQSettings() {{{source}
(async () => {{
  await saveHQSettings();
  assert.deepEqual(saved, {{ model: 'app-default', reasoning_effort: 'low' }});
  assert.equal(agentState.model, 'chat-override', 'saving defaults must not switch a running chat');
}})().catch(error => {{ console.error(error); process.exitCode = 1; }});
"#
    ));
}

#[test]
fn typing_renders_the_active_chat_without_using_the_dom_event_as_state() {
    let binding = include_str!("../static/index.html")
        .lines()
        .find(|line| line.contains("getElementById(\"agent-input\").addEventListener(\"input\""))
        .unwrap();
    run_node(format!(
        r#"
const assert = require('node:assert/strict');
const agentState = {{ threadId: 'active-chat' }};
let listener, rendered;
const document = {{ getElementById: () => ({{ addEventListener: (_, fn) => {{ listener = fn; }} }}) }};
function renderAgentView(state = agentState) {{ rendered = state; }}
{binding}
listener({{ type: 'input' }});
assert.equal(rendered, agentState);
"#
    ));
}
