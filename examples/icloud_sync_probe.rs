//! Opt-in transport smoke test: cargo run --example icloud_sync_probe -- --icloud
use project_hq::sync::{
    reconcile, Document, NextStep, Observation::Known, Reconciliation, Replica, Version,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
#[derive(Deserialize)]
struct Snapshot {
    state: String,
    text: Option<String>,
    revision: Option<String>,
    ubiquitous: Option<bool>,
    uploaded: Option<bool>,
}
fn call(helper: &Path, request: serde_json::Value) -> Result<Snapshot> {
    let mut child = Command::new(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("missing stdin")?
        .write_all(&serde_json::to_vec(&request)?)?;
    let started = Instant::now();
    while child.try_wait()?.is_none() {
        if started.elapsed() > Duration::from_secs(30) {
            child.kill()?;
            child.wait()?;
            return Err("File coordination timed out; probe stopped".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn read(helper: &Path, path: &Path) -> Result<Snapshot> {
    for _ in 0..15 {
        let snapshot = call(helper, json!({"operation":"read", "path":path}))?;
        match snapshot.state.as_str() {
            "available" => return Ok(snapshot),
            "unavailable" => thread::sleep(Duration::from_secs(1)),
            _ => return Err(format!("Read paused: {}", snapshot.state).into()),
        }
    }
    Err("Document not materialized; try again when iCloud is available".into())
}
fn version(snapshot: &Snapshot) -> Result<Version> {
    Ok(Some(Document {
        path: "project.md".into(),
        text: snapshot.text.clone().ok_or("missing document text")?,
    }))
}
fn replace(helper: &Path, path: &Path, text: &str, revision: &Option<String>) -> Result<()> {
    let result = call(
        helper,
        json!({"operation":"replace", "path":path, "text":text, "revision":revision}),
    )?;
    if result.state != "written" {
        return Err(format!("Write paused: {}", result.state).into());
    }
    Ok(())
}
fn sync(
    helper: &Path,
    desktop: &Path,
    cloud: &Path,
    base: &Version,
    journal: &Path,
) -> Result<Version> {
    let local = read(helper, desktop)?;
    let remote = read(helper, cloud)?;
    let Reconciliation::Ready(plan) =
        reconcile(base, Known(&version(&local)?), Known(&version(&remote)?))?
    else {
        return Err("Reconciliation needs review".into());
    };
    fs::write(journal, serde_json::to_vec(&plan)?)?;
    for _ in 0..3 {
        let local = read(helper, desktop)?;
        let remote = read(helper, cloud)?;
        match plan.next_step(Known(&version(&local)?), Known(&version(&remote)?)) {
            NextStep::Converged(version) => return Ok(version.clone()),
            NextStep::Write {
                replica,
                target: Some(target),
                ..
            } => {
                let (path, snapshot) = match replica {
                    Replica::Desktop => (desktop, &local),
                    Replica::Cloud => (cloud, &remote),
                };
                replace(helper, path, &target.text, &snapshot.revision)?;
            }
            _ => return Err("Replica changed or became unavailable; probe stopped".into()),
        }
    }
    Err("Probe did not converge".into())
}
fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() != Some("--icloud") {
        return Err("Opt-in required: cargo run --example icloud_sync_probe -- --icloud".into());
    }
    let helper = PathBuf::from(".build/debug/HQSyncFile")
        .canonicalize()
        .map_err(|_| "Run swift build --product HQSyncFile first")?;
    let cloud_root =
        PathBuf::from(std::env::var("HOME")?).join("Library/Mobile Documents/com~apple~CloudDocs");
    if !cloud_root.is_dir() {
        return Err("iCloud Drive is unavailable on this Mac".into());
    }
    let folder = cloud_root.join(format!(
        "HQ-Sync-Probe-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&folder)?;
    println!(
        "Synthetic iCloud document: {}",
        folder.join("project.md").display()
    );
    let local = tempfile::tempdir()?;
    let desktop = local.path().canonicalize()?.join("project.md");
    let cloud = folder
        .join("project.md")
        .canonicalize()
        .unwrap_or(folder.join("project.md"));
    let initial = "---\ntitle: HQ sync probe\nstatus: active\n---\n\n- [ ] Synthetic capture\n";
    for path in [&desktop, &cloud] {
        let written = call(
            &helper,
            json!({"operation":"create", "path":path, "text":initial}),
        )?;
        if written.state != "written" {
            return Err("Could not create fixture".into());
        }
    }
    let base = version(&read(&helper, &desktop)?)?;
    let capture = format!("{initial}- [ ] Captured in cloud fixture\n");
    replace(&helper, &cloud, &capture, &read(&helper, &cloud)?.revision)?;
    let base = sync(
        &helper,
        &desktop,
        &cloud,
        &base,
        &local.path().join("pending.json"),
    )?;
    assert_eq!(
        read(&helper, &desktop)?.text.as_deref(),
        Some(capture.as_str())
    );
    let edit = capture.replace("status: active", "status: waiting");
    replace(&helper, &desktop, &edit, &read(&helper, &desktop)?.revision)?;
    sync(
        &helper,
        &desktop,
        &cloud,
        &base,
        &local.path().join("pending.json"),
    )?;
    assert_eq!(read(&helper, &cloud)?.text.as_deref(), Some(edit.as_str()));
    println!("Coordinated cloud → desktop → cloud reconciliation passed.");
    // Poll outside every coordination block, with a bounded wait.
    for _ in 0..30 {
        let snapshot = read(&helper, &cloud)?;
        if snapshot.ubiquitous == Some(true) && snapshot.uploaded == Some(true) {
            println!("iCloud reports the final document uploaded. Cross-device delivery still needs an iPhone check.");
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
    println!("Local reconciliation passed; iCloud upload is still unconfirmed. Fixture retained for inspection.");
    Ok(())
}
