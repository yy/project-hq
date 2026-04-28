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
