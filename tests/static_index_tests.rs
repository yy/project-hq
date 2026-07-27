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
  {{ file: "future.md", track: "research", status: "active", priority: 100, title: "Future", visible: false }},
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
fn cards_expose_the_defer_control_and_endpoint() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("class=\"card-defer\""));
    assert!(html.contains("fetch(\"/api/defer\""));
    assert!(html.contains("data-defer-preset=\"hour\""));
    assert!(html.contains("data-defer-preset=\"evening\""));
    assert!(html.contains("visibilityTimer = setTimeout(fetchProjects"));
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
    assert!(!html.contains("id=\"agent-apply-btn\""));
    assert!(!html.contains("id=\"agent-reject-btn\""));
    assert!(html.contains("new EventSource("));
    assert!(html.contains("/api/agent/events?thread_id="));
    assert!(html.contains("agentFetch(\"/api/agent/turn\""));
    assert!(html.contains("agentFetch(\"/api/agent/apply\""));
    assert!(html.contains("async function autoApplyAgentChange()"));
    assert!(!html.contains("agentFetch(\"/api/agent/reject\""));
    assert!(html.contains("Undo is available."));
    assert!(html.contains(".agent-message.user"));
    assert!(html.contains("white-space: pre-wrap"));
    assert!(html.contains("const message = input.value;"));
    assert!(html.contains("if (!message.trim()"));
    assert!(html.contains("fetchProjects();\nensureAgentSession();\nconnectSSE();"));
    assert!(!html.contains("id=\"panel-agent-tab\""));
    assert!(!html.contains("id=\"panel-agent-view\""));
}

#[test]
fn agent_composer_queues_messages_while_a_turn_runs() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("queue: []"));
    assert!(html.contains("dispatching: false"));
    assert!(html.contains("state.queue.push({"));
    assert!(html.contains("contextFile: selectedFile"));
    assert!(html.contains("async function processAgentQueue()"));
    assert!(html.contains("const pending = state.queue.shift();"));
    assert!(html.contains("message: pending.message"));
    assert!(html.contains("context_file: pending.contextFile"));
    assert!(html.contains("void processAgentQueue();"));
    assert!(html.contains(
        "send.textContent = state.busy || state.applying || state.dispatching ? \"Queue\" : \"Send\""
    ));
    assert!(html.contains("input.disabled = !state.threadId;"));
    assert!(!html.contains("input.disabled = !state.threadId || state.busy || state.applying"));
    assert!(html.contains("class=\"agent-queued-label\">Queued"));
    assert!(html.contains(".agent-message.user.queued"));
}

#[test]
fn agent_transcript_hides_tool_activity() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("<strong>Codex working…</strong>"));
    assert!(!html.contains("kind: \"activity\""));
    assert!(!html.contains(".agent-activity"));
    assert!(!html.contains("item.aggregatedOutput"));
    assert!(!html.contains("item.command ||"));
    assert!(html.contains("item.type === \"agentMessage\""));
    assert!(html.contains("kind: \"notice\""));
}

#[test]
fn project_focus_has_clear_navigation_and_editable_metadata() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("id=\"home-btn\""));
    assert!(html.contains("title=\"Return to Kanban\""));
    assert!(html.contains("id=\"panel-close\">← Kanban"));
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
    assert!(html.contains("fetch(\"/api/metadata\""));
    assert!(html.contains("document.getElementById(\"home-btn\").addEventListener"));
    assert!(html.contains("[\"parallel\", \"Parallel\"]"));
    assert!(html.contains("[\"serial\", \"Serial\"]"));
    assert!(!html.contains("[\"single\", \"Single\"]"));
    assert!(!html.contains("[\"sequential\", \"Sequential\"]"));
}

#[test]
fn dashboard_exposes_revision_safe_undo() {
    let html = include_str!("../static/index.html");

    assert!(html.contains("id=\"undo-btn\""));
    assert!(html.contains("fetch(\"/api/undo\""));
    assert!(html.contains("async function undoLastAction()"));
    assert!(html.contains("e.key.toLowerCase() === \"z\""));
    assert!(html.contains("!typing"));
    assert!(html.contains("if (affected.size) discardAgentState()"));
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
