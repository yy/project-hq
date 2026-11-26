use sha2::{Digest, Sha256};
use std::{fs, process::Command};

#[test]
fn exports_only_a_current_saved_review_and_preserves_existing_files() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let vault = dir.join("vault");
    let settings = dir.join("net.yulab.project-hq");
    let cache = settings.join("overviews");
    fs::create_dir_all(vault.join("projects")).unwrap();
    fs::create_dir_all(&cache).unwrap();
    let file = "projects/neighborhood-walk.md";
    let source = include_str!("../ios/SampleHQ/projects/neighborhood-walk.md");
    fs::write(vault.join(file), source).unwrap();
    let summary = include_str!("../ios/SampleHQ/projects/neighborhood-walk.md.overview.json");
    let summary: serde_json::Value = serde_json::from_str(summary).unwrap();
    fs::write(
        settings.join("ai-settings.json"),
        serde_json::json!({ "model": summary["model"] }).to_string(),
    )
    .unwrap();
    let key = Sha256::digest(format!(
        "overview-v2\0{}\0{file}",
        vault.canonicalize().unwrap().display()
    ));
    let output = dir.join("review.json");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_hq"))
            .env("XDG_CACHE_HOME", dir)
            .env("XDG_CONFIG_HOME", dir)
            .arg("--dir")
            .arg(&vault)
            .args([
                "export-review",
                "./projects/neighborhood-walk.md",
                "--output",
            ])
            .arg(&output)
            .output()
            .unwrap()
            .status
    };

    assert!(!run().success());
    assert!(!output.exists());
    fs::write(cache.join(format!("{key:x}.json")), summary.to_string()).unwrap();
    assert!(run().success());
    let review: serde_json::Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(review["hq_review"], 1);
    assert_eq!(review["title"], "Neighborhood walk");
    assert_eq!(review["summary"], summary);
    fs::write(&output, "keep this file").unwrap();
    assert!(!run().success());
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep this file");
    fs::remove_file(&output).unwrap();
    fs::write(vault.join(file), format!("{source}\nUpdated.")).unwrap();
    assert!(!run().success());
    assert!(!output.exists());
}
