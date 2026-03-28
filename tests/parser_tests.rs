use std::fs;
use std::path::Path;

use project_hq::config::Config;
use project_hq::load_all;
use project_hq::project::Project;

fn setup_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("failed to create temp dir")
}

fn write_project(base: &Path, track: &str, filename: &str, content: &str) {
    let dir = base.join(track);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(filename), content).unwrap();
}

/// Helper: write a project file and parse it via Project::from_file
fn parse_project(content: &str) -> Option<Project> {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(base, "test", "p.md", content);
    Project::from_file(&base.join("test/p.md"), "test", base)
}

// === Parser tests ===

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
    let p = parse_project(content).unwrap();
    assert_eq!(p.title, "My Project");
    assert_eq!(p.track, "research");
    assert_eq!(p.status, "active");
    assert_eq!(p.waiting_on, "me");
    assert_eq!(p.my_next, "write tests");
    assert_eq!(p.deadline.as_deref(), Some("2026-04-01"));
    assert_eq!(p.priority, 90);
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
    let p = parse_project(content).unwrap();
    assert_eq!(p.title, "Another Project");
    assert_eq!(p.status, "waiting");
    assert_eq!(p.waiting_on, "reviewer");
    assert!(p.waiting_since.is_some());
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
    let p = parse_project(content).unwrap();
    assert_eq!(p.title, "Side thing");
    assert_eq!(p.track, "personal");
    assert_eq!(p.status, "deferred");
    assert!(p.deferred_until.is_some());
}

#[test]
fn handles_numeric_priority() {
    let content = "---\ntitle: \"Grant\"\ntrack: funding\nstatus: active\npriority: 25\n---\n";
    let p = parse_project(content).unwrap();
    assert_eq!(p.priority, 25);
}

#[test]
fn default_priority_when_absent() {
    let content = "---\ntitle: \"Grant D\"\ntrack: funding\nstatus: active\n---\n";
    let p = parse_project(content).unwrap();
    assert_eq!(p.priority, 50);
}

#[test]
fn returns_none_for_files_without_frontmatter() {
    let content = "# Just a heading\n\nSome text.\n";
    assert!(parse_project(content).is_none());
}

#[test]
fn returns_none_for_missing_required_fields() {
    let content = "---\nowner: YY\npriority: 50\n---\n";
    assert!(parse_project(content).is_none());
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
    let p = parse_project(content).unwrap();
    assert_eq!(p.title, "My Project");
}

#[test]
fn skips_comment_lines() {
    let content = "---\ntitle: \"Test\"\n# this is a comment\nstatus: active\n---\n";
    let p = parse_project(content).unwrap();
    assert_eq!(p.title, "Test");
    assert_eq!(p.status, "active");
}

#[test]
fn skips_empty_values() {
    let content = "---\ntitle: \"Test\"\nstatus: active\nowner: \n---\n";
    let p = parse_project(content).unwrap();
    assert_eq!(p.owner, ""); // empty because the field was skipped
}

// === Load tests ===

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

    let config = Config::load(base);
    let projects = load_all(base, &config);
    assert_eq!(projects.len(), 2);
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

    let config = Config::load(base);
    let projects = load_all(base, &config);
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].title, "Good");
}

#[test]
fn preserves_body_after_frontmatter() {
    let content = "---\ntitle: \"Test\"\nstatus: active\n---\n\nSome notes here.\n";
    let p = parse_project(content).unwrap();
    assert_eq!(p.title, "Test");
}
