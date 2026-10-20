#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Fixture {
    _root: tempfile::TempDir,
    source: PathBuf,
    publisher: PathBuf,
    cache: PathBuf,
    installed: String,
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let remote = root.path().join("origin.git");
        let source = root.path().join("source checkout");
        let publisher = root.path().join("publisher");
        let cache = root.path().join("update cache");
        for path in [&remote, &source] {
            std::fs::create_dir(path).unwrap();
        }
        git(&remote, &["init", "--bare", "--initial-branch=main"]);
        git(&source, &["init", "--initial-branch=main"]);
        git(&source, &["config", "user.name", "Test fixture"]);
        git(&source, &["config", "user.email", "test@example.invalid"]);
        git(
            &source,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        std::fs::create_dir(source.join("script")).unwrap();
        std::fs::write(
            source.join("script/build_and_run.sh"),
            r#"#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p dist/HQ.app/Contents/MacOS
cp /usr/bin/true dist/HQ.app/Contents/MacOS/HQ
cat > dist/HQ.app/Contents/Info.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>net.yulab.hq.update-fixture</string>
<key>CFBundleExecutable</key><string>HQ</string>
<key>HQRevision</key><string>$HQ_BUILD_REVISION</string>
<key>HQSourceDirectory</key><string>$HQ_BUILD_SOURCE_DIR</string>
</dict></plist>
PLIST
/usr/bin/codesign --force --sign - dist/HQ.app
"#,
        )
        .unwrap();
        git(&source, &["add", "script/build_and_run.sh"]);
        git(&source, &["commit", "-m", "Initial fixture"]);
        git(&source, &["push", "-u", "origin", "main"]);
        let installed = git(&source, &["rev-parse", "HEAD"]);
        git(
            root.path(),
            &[
                "clone",
                remote.to_str().unwrap(),
                publisher.to_str().unwrap(),
            ],
        );
        git(&publisher, &["config", "user.name", "Test fixture"]);
        git(
            &publisher,
            &["config", "user.email", "test@example.invalid"],
        );
        Self {
            _root: root,
            source,
            publisher,
            cache,
            installed,
        }
    }

    fn push_update(&self, broken: bool) -> String {
        if broken {
            std::fs::write(self.publisher.join("script/build_and_run.sh"), "exit 23\n").unwrap();
        } else {
            std::fs::write(self.publisher.join("change.txt"), "pushed change\n").unwrap();
        }
        git(&self.publisher, &["add", "."]);
        git(&self.publisher, &["commit", "-m", "Pushed update"]);
        git(&self.publisher, &["push", "origin", "main"]);
        git(&self.publisher, &["rev-parse", "HEAD"])
    }

    fn check(&self, revision: &str, validate: bool) -> Output {
        let mut command = Command::new("/bin/bash");
        command
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/script/prepare_update.sh"
            ))
            .arg(&self.source)
            .arg(revision)
            .arg(&self.cache)
            .arg(self._root.path().join("data"));
        if validate {
            command.arg("--validate");
        }
        command.output().unwrap()
    }
}

#[test]
fn pushed_update_builds_a_signed_bundle_without_mutating_source_or_data() {
    let fixture = Fixture::new();
    let current = fixture.check(&fixture.installed, false);
    assert!(current.status.success());
    assert!(String::from_utf8_lossy(&current.stdout).contains("HQ_UPDATE current"));
    let revision = fixture.push_update(false);
    let output = fixture.check(&fixture.installed, false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(&format!("HQ_UPDATE ready {revision}"))
    );
    assert_eq!(
        git(&fixture.source, &["rev-parse", "HEAD"]),
        fixture.installed
    );
    assert!(git(&fixture.source, &["status", "--porcelain"]).is_empty());
    assert!(!fixture._root.path().join("data").exists());
    let plist =
        std::fs::read_to_string(fixture.cache.join("source/dist/HQ.app/Contents/Info.plist"))
            .unwrap();
    assert!(plist.contains(&revision));
    assert!(fixture.check(&revision, true).status.success());
    std::fs::write(fixture.source.join("local.txt"), "local change").unwrap();
    let output = fixture.check(&revision, true);
    assert!(String::from_utf8_lossy(&output.stdout).contains("HQ_UPDATE deferred"));
}

#[test]
fn dirty_wrong_branch_ahead_and_diverged_sources_defer_without_building() {
    let fixture = Fixture::new();
    std::fs::write(fixture.source.join("local.txt"), "local change").unwrap();
    let output = fixture.check(&fixture.installed, false);
    assert!(String::from_utf8_lossy(&output.stdout).contains("HQ_UPDATE deferred"));
    git(&fixture.source, &["add", "local.txt"]);
    git(&fixture.source, &["commit", "-m", "Local work"]);
    let output = fixture.check(&git(&fixture.source, &["rev-parse", "HEAD"]), false);
    assert!(String::from_utf8_lossy(&output.stdout).contains("HQ_UPDATE deferred"));
    fixture.push_update(false);
    let output = fixture.check(&fixture.installed, false);
    assert!(String::from_utf8_lossy(&output.stdout).contains("HQ_UPDATE deferred"));
    git(&fixture.source, &["switch", "-c", "feature"]);
    let output = fixture.check(&fixture.installed, false);
    assert!(String::from_utf8_lossy(&output.stdout).contains("HQ_UPDATE deferred"));
    assert!(!fixture.cache.exists());
}

#[test]
fn build_failure_is_reported_without_advancing_checkout() {
    let fixture = Fixture::new();
    fixture.push_update(true);
    let output = fixture.check(&fixture.installed, false);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("HQ_UPDATE ready"));
    assert_eq!(
        git(&fixture.source, &["rev-parse", "HEAD"]),
        fixture.installed
    );
    assert!(git(&fixture.source, &["status", "--porcelain"]).is_empty());
}
