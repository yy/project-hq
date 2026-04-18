use std::fs;
use std::path::Path;

use project_hq::config::Config;
use project_hq::load_all;
use project_hq::project_file::project_body;

fn write_project(base: &Path, track: &str, filename: &str, content: &str) {
    let dir = base.join(track);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(filename), content).unwrap();
}

#[test]
fn load_all_accepts_frontmatter_with_utf8_bom() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    write_project(
        base,
        "research",
        "bom.md",
        "\u{feff}---\ntitle: \"BOM project\"\nstatus: active\n---\n\nNotes\n",
    );

    let config = Config::load(base);
    let projects = load_all(base, &config);

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].title, "BOM project");
}

#[test]
fn project_body_ignores_utf8_bom_before_frontmatter() {
    let text = "\u{feff}---\ntitle: \"BOM project\"\nstatus: active\n---\n\nNotes\n";

    assert_eq!(project_body(text), "Notes\n");
}
