use std::fs;
use std::path::Path;

use project_hq::config::{Config, DEFAULT_STALE_DAYS};
use project_hq::frontmatter::split_frontmatter;
use project_hq::load_all;
use project_hq::mover::{move_project, reorder_projects, MoveOptions};
use project_hq::project::{Project, DEFAULT_PRIORITY};

fn setup_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("failed to create temp dir")
}

fn write_project(base: &Path, track: &str, filename: &str, content: &str) {
    let dir = base.join(track);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(filename), content).unwrap();
}

/// Helper: parse a project directly from markdown text.
fn parse_project(content: &str) -> Option<Project> {
    Project::from_text(content, "test", "test/p.md")
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
fn waiting_days_ignores_future_dates() {
    let content = r#"---
title: "Future wait"
track: research
status: waiting
waiting_on: reviewer
waiting_since: 2999-01-01
---
"#;
    let p = parse_project(content).unwrap();
    assert_eq!(p.waiting_days(), None);
}

#[test]
fn classifies_waiting_like_statuses() {
    let waiting = parse_project("---\ntitle: \"Wait\"\nstatus: waiting\n---\n").unwrap();
    let submitted = parse_project("---\ntitle: \"Sent\"\nstatus: submitted\n---\n").unwrap();
    let active = parse_project("---\ntitle: \"Do\"\nstatus: active\n---\n").unwrap();

    assert!(waiting.is_waiting_like());
    assert!(submitted.is_waiting_like());
    assert!(!active.is_waiting_like());
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
    assert_eq!(p.priority, DEFAULT_PRIORITY);
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

#[test]
fn closing_dashes_must_be_on_own_line() {
    // "priority: 40---" should NOT be treated as closing frontmatter
    let content = "---\ntitle: \"Test\"\nstatus: active\npriority: 40---\nmore: stuff\n---\n";
    let p = parse_project(content).unwrap();
    assert_eq!(p.title, "Test");
    // "40---" fails i32 parse, so falls back to default 50
    assert_eq!(p.priority, 50);
}

#[test]
fn closing_dashes_glued_to_value_no_real_close() {
    // If the only --- is glued to a value, parsing should fail
    let content = "---\ntitle: \"Test\"\nstatus: active\npriority: 40---\n";
    assert!(parse_project(content).is_none());
}

#[test]
fn rejects_opening_delimiter_longer_than_three_dashes() {
    let content = "----\ntitle: \"Test\"\nstatus: active\n---\n";
    assert!(parse_project(content).is_none());
}

#[test]
fn rejects_closing_delimiter_longer_than_three_dashes() {
    let content = "---\ntitle: \"Test\"\nstatus: active\n----\nBody text.\n";
    assert!(parse_project(content).is_none());
}

// === Mover round-trip tests ===

#[test]
fn move_then_reparse_roundtrip() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "t",
        "a.md",
        "---\ntitle: \"A\"\nstatus: active\npriority: 50\n---\n\nNotes here.\n",
    );
    // Move it
    move_project(
        base,
        &MoveOptions {
            file: "t/a.md".to_string(),
            to_status: "waiting".to_string(),
            priority: None,
        },
    )
    .unwrap();
    // Reparse
    let p = Project::from_file(&base.join("t/a.md"), "t", base).unwrap();
    assert_eq!(p.status, "waiting");
    // Move again
    move_project(
        base,
        &MoveOptions {
            file: "t/a.md".to_string(),
            to_status: "done".to_string(),
            priority: Some(10),
        },
    )
    .unwrap();
    let p = Project::from_file(&base.join("t/a.md"), "t", base).unwrap();
    assert_eq!(p.status, "done");
    assert_eq!(p.priority, 10);
}

#[test]
fn reorder_then_reparse_roundtrip() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "t",
        "a.md",
        "---\ntitle: \"A\"\nstatus: active\n---\n\nBody A.\n",
    );
    write_project(
        base,
        "t",
        "b.md",
        "---\ntitle: \"B\"\nstatus: active\n---\n\nBody B.\n",
    );

    reorder_projects(base, &["t/b.md".to_string(), "t/a.md".to_string()]).unwrap();

    // Both should still parse after reorder
    let a = Project::from_file(&base.join("t/a.md"), "t", base).unwrap();
    let b = Project::from_file(&base.join("t/b.md"), "t", base).unwrap();
    assert_eq!(b.priority, 20); // first = highest
    assert_eq!(a.priority, 10);

    // Reorder again — should still roundtrip
    reorder_projects(base, &["t/a.md".to_string(), "t/b.md".to_string()]).unwrap();
    let a = Project::from_file(&base.join("t/a.md"), "t", base).unwrap();
    let b = Project::from_file(&base.join("t/b.md"), "t", base).unwrap();
    assert_eq!(a.priority, 20); // first = highest
    assert_eq!(b.priority, 10);

    // Body preserved
    let text = fs::read_to_string(base.join("t/a.md")).unwrap();
    assert!(text.contains("Body A."));
}

// === Config tests ===

#[test]
fn config_defaults_without_toml() {
    let tmp = setup_dir();
    let config = Config::load(tmp.path());
    assert_eq!(config.stale_days, DEFAULT_STALE_DAYS);
    assert_eq!(
        config.statuses,
        [
            "active",
            "waiting",
            "deferred",
            "submitted",
            "done",
            "dropped"
        ]
    );
    assert!(config.skip_files.is_empty());
}

#[test]
fn config_loads_statuses_from_toml() {
    let tmp = setup_dir();
    fs::write(
        tmp.path().join("hq.toml"),
        "statuses = [\"todo\", \"doing\", \"done\"]\nstale_days = 7\n",
    )
    .unwrap();
    let config = Config::load(tmp.path());
    assert_eq!(config.statuses, ["todo", "doing", "done"]);
    assert_eq!(config.stale_days, 7);
}

#[test]
fn config_autodiscovers_tracks() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "research",
        "p.md",
        "---\ntitle: \"P\"\nstatus: active\n---\n",
    );
    write_project(
        base,
        "funding",
        "q.md",
        "---\ntitle: \"Q\"\nstatus: active\n---\n",
    );
    // Non-track dir (no frontmatter)
    fs::create_dir_all(base.join("scripts")).unwrap();
    fs::write(base.join("scripts/run.sh"), "#!/bin/bash").unwrap();

    let config = Config::load(base);
    assert!(config.tracks.contains(&"research".to_string()));
    assert!(config.tracks.contains(&"funding".to_string()));
    assert!(!config.tracks.contains(&"scripts".to_string()));
}

#[test]
fn config_ignores_files_with_malformed_frontmatter_delimiters() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "notes",
        "bad.md",
        "----\ntitle: \"Looks like a project\"\nstatus: active\n---\n",
    );

    let config = Config::load(base);
    assert!(!config.tracks.contains(&"notes".to_string()));
}

// === Mover tests ===

#[test]
fn move_project_changes_status() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "research",
        "proj.md",
        "---\ntitle: \"Proj\"\nstatus: active\npriority: 10\n---\nBody text.\n",
    );
    let opts = MoveOptions {
        file: "research/proj.md".to_string(),
        to_status: "waiting".to_string(),
        priority: None,
    };
    move_project(base, &opts).unwrap();
    let p = Project::from_file(&base.join("research/proj.md"), "research", base).unwrap();
    assert_eq!(p.status, "waiting");
    assert_eq!(p.priority, 10); // priority unchanged
}

#[test]
fn move_project_changes_status_and_priority() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "research",
        "proj.md",
        "---\ntitle: \"Proj\"\nstatus: active\npriority: 10\n---\n",
    );
    let opts = MoveOptions {
        file: "research/proj.md".to_string(),
        to_status: "deferred".to_string(),
        priority: Some(99),
    };
    move_project(base, &opts).unwrap();
    let p = Project::from_file(&base.join("research/proj.md"), "research", base).unwrap();
    assert_eq!(p.status, "deferred");
    assert_eq!(p.priority, 99);
}

#[test]
fn move_project_inserts_priority_after_indented_status() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "research",
        "proj.md",
        "---\ntitle: \"Proj\"\n  status: active\n---\n",
    );
    let opts = MoveOptions {
        file: "research/proj.md".to_string(),
        to_status: "waiting".to_string(),
        priority: Some(30),
    };
    move_project(base, &opts).unwrap();

    let text = fs::read_to_string(base.join("research/proj.md")).unwrap();
    assert!(text.contains("status: waiting\npriority: 30\n---"));

    let p = Project::from_file(&base.join("research/proj.md"), "research", base).unwrap();
    assert_eq!(p.status, "waiting");
    assert_eq!(p.priority, 30);
}

#[test]
fn move_project_preserves_body() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "research",
        "proj.md",
        "---\ntitle: \"Proj\"\nstatus: active\n---\n\n## Notes\nImportant stuff.\n",
    );
    let opts = MoveOptions {
        file: "research/proj.md".to_string(),
        to_status: "done".to_string(),
        priority: None,
    };
    move_project(base, &opts).unwrap();
    let text = fs::read_to_string(base.join("research/proj.md")).unwrap();
    assert!(text.contains("## Notes"));
    assert!(text.contains("Important stuff."));
}

#[test]
fn move_project_errors_on_missing_file() {
    let tmp = setup_dir();
    let result = move_project(
        tmp.path(),
        &MoveOptions {
            file: "nope/missing.md".to_string(),
            to_status: "active".to_string(),
            priority: None,
        },
    );
    assert!(result.is_err());
}

#[test]
fn move_project_rejects_paths_outside_hq_dir() {
    let tmp = setup_dir();
    let base = tmp.path().join("hq");
    fs::create_dir_all(&base).unwrap();
    let outside = tmp.path().join("outside.md");
    fs::write(&outside, "---\ntitle: \"Outside\"\nstatus: active\n---\n").unwrap();

    let absolute = move_project(
        &base,
        &MoveOptions {
            file: outside.to_string_lossy().to_string(),
            to_status: "done".to_string(),
            priority: None,
        },
    );
    assert!(absolute.is_err());

    let parent = move_project(
        &base,
        &MoveOptions {
            file: "../outside.md".to_string(),
            to_status: "done".to_string(),
            priority: None,
        },
    );
    assert!(parent.is_err());

    let text = fs::read_to_string(&outside).unwrap();
    assert!(text.contains("status: active"));
}

#[test]
fn move_project_rejects_non_markdown_files() {
    let tmp = setup_dir();
    let base = tmp.path();
    fs::write(
        base.join("hq.toml"),
        "---\ntitle: \"Config\"\nstatus: active\n---\n",
    )
    .unwrap();

    let result = move_project(
        base,
        &MoveOptions {
            file: "hq.toml".to_string(),
            to_status: "done".to_string(),
            priority: None,
        },
    );
    assert!(result.is_err());

    let text = fs::read_to_string(base.join("hq.toml")).unwrap();
    assert!(text.contains("status: active"));
}

// === Reorder tests ===

#[test]
fn reorder_assigns_sequential_priorities() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "t",
        "a.md",
        "---\ntitle: \"A\"\nstatus: active\npriority: 50\n---\n",
    );
    write_project(
        base,
        "t",
        "b.md",
        "---\ntitle: \"B\"\nstatus: active\npriority: 50\n---\n",
    );
    write_project(
        base,
        "t",
        "c.md",
        "---\ntitle: \"C\"\nstatus: active\npriority: 50\n---\n",
    );

    let files = vec![
        "t/c.md".to_string(),
        "t/a.md".to_string(),
        "t/b.md".to_string(),
    ];
    reorder_projects(base, &files).unwrap();

    let c = Project::from_file(&base.join("t/c.md"), "t", base).unwrap();
    let a = Project::from_file(&base.join("t/a.md"), "t", base).unwrap();
    let b = Project::from_file(&base.join("t/b.md"), "t", base).unwrap();
    assert_eq!(c.priority, 30); // first in list = highest priority
    assert_eq!(a.priority, 20);
    assert_eq!(b.priority, 10);
}

#[test]
fn reorder_inserts_priority_when_absent() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "t",
        "a.md",
        "---\ntitle: \"A\"\nstatus: active\n---\n",
    );
    write_project(
        base,
        "t",
        "b.md",
        "---\ntitle: \"B\"\nstatus: active\n---\n",
    );

    let files = vec!["t/b.md".to_string(), "t/a.md".to_string()];
    reorder_projects(base, &files).unwrap();

    let b = Project::from_file(&base.join("t/b.md"), "t", base).unwrap();
    let a = Project::from_file(&base.join("t/a.md"), "t", base).unwrap();
    assert_eq!(b.priority, 20); // first in list = highest priority
    assert_eq!(a.priority, 10);
}

#[test]
fn reorder_preserves_body_content() {
    let tmp = setup_dir();
    let base = tmp.path();
    write_project(
        base,
        "t",
        "a.md",
        "---\ntitle: \"A\"\nstatus: active\npriority: 50\n---\n\n## Notes\nKeep this.\n",
    );

    reorder_projects(base, &["t/a.md".to_string()]).unwrap();

    let text = fs::read_to_string(base.join("t/a.md")).unwrap();
    assert!(text.contains("## Notes"));
    assert!(text.contains("Keep this."));
}

#[test]
fn reorder_rejects_paths_outside_hq_dir() {
    let tmp = setup_dir();
    let base = tmp.path().join("hq");
    fs::create_dir_all(&base).unwrap();
    let outside = tmp.path().join("outside.md");
    fs::write(
        &outside,
        "---\ntitle: \"Outside\"\nstatus: active\npriority: 50\n---\n",
    )
    .unwrap();

    let result = reorder_projects(&base, &[outside.to_string_lossy().to_string()]);
    assert!(result.is_err());

    let text = fs::read_to_string(&outside).unwrap();
    assert!(text.contains("priority: 50"));
}

#[test]
fn reorder_rejects_non_markdown_files() {
    let tmp = setup_dir();
    let base = tmp.path();
    fs::write(
        base.join("hq.toml"),
        "---\ntitle: \"Config\"\nstatus: active\npriority: 50\n---\n",
    )
    .unwrap();

    let result = reorder_projects(base, &["hq.toml".to_string()]);
    assert!(result.is_err());

    let text = fs::read_to_string(base.join("hq.toml")).unwrap();
    assert!(text.contains("priority: 50"));
}

// === split_frontmatter tests ===

#[test]
fn split_fm_basic() {
    let text = "---\ntitle: \"A\"\nstatus: active\n---\n\nBody text.\n";
    let (fm, body) = split_frontmatter(text).unwrap();
    assert_eq!(fm, "\ntitle: \"A\"\nstatus: active\n");
    assert_eq!(body, "\n\nBody text.\n");
}

#[test]
fn split_fm_no_body() {
    let text = "---\ntitle: \"A\"\nstatus: active\n---\n";
    let (fm, body) = split_frontmatter(text).unwrap();
    assert!(fm.contains("title:"));
    assert_eq!(body, "\n");
}

#[test]
fn split_fm_rejects_no_frontmatter() {
    assert!(split_frontmatter("Just text").is_err());
}

#[test]
fn split_fm_rejects_unclosed_frontmatter() {
    assert!(split_frontmatter("---\ntitle: \"A\"\nstatus: active\n").is_err());
}

#[test]
fn split_fm_closing_must_be_on_own_line() {
    // "---" glued to a value should not close frontmatter
    let text = "---\ntitle: \"A\"\npriority: 40---\nstatus: active\n---\n";
    let (fm, _body) = split_frontmatter(text).unwrap();
    // The real closing --- is the last one; frontmatter includes the "40---" line
    assert!(fm.contains("40---"));
    assert!(fm.contains("status: active"));
}

#[test]
fn split_fm_agrees_with_project_parser() {
    // Both parsers should successfully parse the same file and agree on fields
    let text = "---\ntitle: \"Test\"\nstatus: active\npriority: 40---\nmore: stuff\n---\n\nBody.\n";

    // split_frontmatter should find the real closing ---
    let (fm, _body) = split_frontmatter(text).unwrap();
    assert!(fm.contains("priority: 40---"));

    // project parser should also parse this successfully with the same field values
    let p = parse_project(text).unwrap();
    assert_eq!(p.title, "Test");
    assert_eq!(p.status, "active");
    // "40---" fails i32 parse, falls back to 50
    assert_eq!(p.priority, 50);
}
