use std::fs;
use std::path::Path;

// We need access to the project module from the binary crate.
// Since this is a binary, we test via integration tests that
// exercise the same parsing logic by importing a library module.
// For now, we test by creating temp files and running the binary,
// or by duplicating the parse logic in tests.

// Since project.rs is in a binary crate, we replicate the frontmatter
// parsing here to test it. In the future, extracting to a lib crate
// would be cleaner.

use std::collections::BTreeMap;

fn parse_frontmatter(text: &str) -> Option<BTreeMap<String, String>> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut fields = BTreeMap::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().trim_matches('"').to_string();
            if !value.is_empty() {
                fields.insert(key, value);
            }
        }
    }
    if fields.contains_key("title") && fields.contains_key("status") {
        Some(fields)
    } else {
        None
    }
}

fn setup_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("failed to create temp dir")
}

fn write_project(base: &Path, track: &str, filename: &str, content: &str) {
    let dir = base.join(track);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(filename), content).unwrap();
}

// === Parser tests (matching TypeScript parser.test.ts) ===

#[test]
fn parses_basic_project_fields() {
    let content = r#"---
title: "My Project"
track: research
status: active
waiting_on: me
my_next: write tests
deadline: 2026-04-01
priority: 90
---
"#;
    let fields = parse_frontmatter(content).unwrap();
    assert_eq!(fields.get("title").unwrap(), "My Project");
    assert_eq!(fields.get("track").unwrap(), "research");
    assert_eq!(fields.get("status").unwrap(), "active");
    assert_eq!(fields.get("waiting_on").unwrap(), "me");
    assert_eq!(fields.get("my_next").unwrap(), "write tests");
    assert_eq!(fields.get("deadline").unwrap(), "2026-04-01");
    assert_eq!(fields.get("priority").unwrap(), "90");
}

#[test]
fn parses_waiting_since_field() {
    let content = r#"---
title: "Another Project"
track: research
status: waiting
waiting_on: reviewer
waiting_since: 2026-02-15
my_next: wait
---
"#;
    let fields = parse_frontmatter(content).unwrap();
    assert_eq!(fields.get("title").unwrap(), "Another Project");
    assert_eq!(fields.get("status").unwrap(), "waiting");
    assert_eq!(fields.get("waiting_on").unwrap(), "reviewer");
    assert_eq!(fields.get("waiting_since").unwrap(), "2026-02-15");
}

#[test]
fn handles_deferred_until_field() {
    let content = r#"---
title: "Side thing"
track: personal
status: deferred
deferred_until: 2026-06-01
my_next: revisit later
---
"#;
    let fields = parse_frontmatter(content).unwrap();
    assert_eq!(fields.get("title").unwrap(), "Side thing");
    assert_eq!(fields.get("track").unwrap(), "personal");
    assert_eq!(fields.get("status").unwrap(), "deferred");
    assert_eq!(fields.get("deferred_until").unwrap(), "2026-06-01");
}

#[test]
fn handles_numeric_priority() {
    let content = "---\ntitle: \"Grant\"\ntrack: funding\nstatus: active\npriority: 25\n---\n";
    let fields = parse_frontmatter(content).unwrap();
    assert_eq!(fields.get("priority").unwrap(), "25");
}

#[test]
fn default_priority_when_absent() {
    let content = "---\ntitle: \"Grant D\"\ntrack: funding\nstatus: active\n---\n";
    let fields = parse_frontmatter(content).unwrap();
    assert!(fields.get("priority").is_none());
    // The Project struct defaults to 50 when priority is absent
}

#[test]
fn returns_none_for_files_without_frontmatter() {
    let content = "# Just a heading\n\nSome text.\n";
    assert!(parse_frontmatter(content).is_none());
}

#[test]
fn returns_none_for_missing_required_fields() {
    // Has frontmatter delimiters but no title or status
    let content = "---\nowner: YY\npriority: 50\n---\n";
    assert!(parse_frontmatter(content).is_none());
}

#[test]
fn handles_colons_in_values() {
    let content = r#"---
title: "My Project"
track: research
status: active
paper: "https://www.overleaf.com/project/123"
notes: "POC: Chrissie Holt-Hull"
---
"#;
    let fields = parse_frontmatter(content).unwrap();
    assert_eq!(
        fields.get("paper").unwrap(),
        "https://www.overleaf.com/project/123"
    );
    assert_eq!(fields.get("notes").unwrap(), "POC: Chrissie Holt-Hull");
}

#[test]
fn skips_comment_lines() {
    let content = "---\ntitle: \"Test\"\n# this is a comment\nstatus: active\n---\n";
    let fields = parse_frontmatter(content).unwrap();
    assert_eq!(fields.get("title").unwrap(), "Test");
    assert_eq!(fields.get("status").unwrap(), "active");
    assert!(!fields.contains_key("#"));
}

#[test]
fn skips_empty_values() {
    let content = "---\ntitle: \"Test\"\nstatus: active\nowner: \n---\n";
    let fields = parse_frontmatter(content).unwrap();
    assert!(fields.get("owner").is_none());
}

// === Load tests (matching TypeScript loadAll tests) ===

#[test]
fn load_from_multiple_track_directories() {
    let tmp = setup_dir();
    let base = tmp.path();

    write_project(
        base,
        "research",
        "r1.md",
        "---\ntitle: \"R1\"\ntrack: research\nstatus: active\n---\n",
    );
    write_project(
        base,
        "funding",
        "f1.md",
        "---\ntitle: \"F1\"\ntrack: funding\nstatus: waiting\nwaiting_on: NSF\n---\n",
    );

    // Verify both files exist and parse correctly
    let r1 = fs::read_to_string(base.join("research/r1.md")).unwrap();
    let f1 = fs::read_to_string(base.join("funding/f1.md")).unwrap();
    assert!(parse_frontmatter(&r1).is_some());
    assert!(parse_frontmatter(&f1).is_some());
}

#[test]
fn skips_files_without_frontmatter_in_load() {
    let tmp = setup_dir();
    let base = tmp.path();

    write_project(
        base,
        "research",
        "good.md",
        "---\ntitle: \"Good\"\ntrack: research\nstatus: active\n---\n",
    );
    write_project(base, "research", "general.md", "# General notes\n");
    write_project(base, "research", "no-fm.md", "Just text\n");

    // good.md parses, others don't
    let good = fs::read_to_string(base.join("research/good.md")).unwrap();
    let general = fs::read_to_string(base.join("research/general.md")).unwrap();
    let no_fm = fs::read_to_string(base.join("research/no-fm.md")).unwrap();

    assert!(parse_frontmatter(&good).is_some());
    assert!(parse_frontmatter(&general).is_none());
    assert!(parse_frontmatter(&no_fm).is_none());
}

#[test]
fn preserves_body_after_frontmatter() {
    let content = "---\ntitle: \"Test\"\nstatus: active\n---\n\nSome notes here.\n";
    let fields = parse_frontmatter(content).unwrap();
    assert_eq!(fields.get("title").unwrap(), "Test");
    // Body is not in fields — it's preserved in the file but not parsed
}
