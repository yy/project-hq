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
        .find("function getColumnItems(")
        .expect("static index should define getColumnItems");
    let rest = &html[start..];
    let end = rest
        .find("\n\nfunction computePriority")
        .expect("getColumnItems should end before computePriority");

    rest[..end].to_string()
}

fn days_since_source() -> String {
    let html = include_str!("../static/index.html");
    let start = html
        .find("function daysSince(")
        .expect("static index should define daysSince");
    let rest = &html[start..];
    let end = rest
        .find("\n\n// SSE live reload")
        .expect("daysSince should end before the SSE setup");

    rest[..end].to_string()
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
fn get_column_items_sorts_and_filters_like_rendered_columns() {
    let script = format!(
        r#"
let projects = [
  {{ file: "research-low.md", track: "research", status: "active", priority: 10 }},
  {{ file: "admin-high.md", track: "admin", status: "active", priority: 30 }},
  {{ file: "research-high.md", track: "research", status: "active", priority: 20 }},
  {{ file: "waiting.md", track: "research", status: "waiting", priority: 99 }},
];
let activeTrack = null;

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
"#,
        get_column_items_source()
    );

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
