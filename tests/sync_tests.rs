use project_hq::sync::{
    reconcile, Conflict, Document, NextStep, Observation::Known, Observation::Unavailable, Plan,
    Reconciliation, Replica, Version,
};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
};

const CARD: &str = "work/project.md";
const ARCHIVED: &str = "work/archive/project.md";
const BASE: &str =
    "---\ntitle: Fixture\nstatus: active\n---\n\n# Notes\n\nDetails.\n\n- [ ] Draft\n";

fn document(path: &str, text: &str) -> Version {
    Some(Document {
        path: path.into(),
        text: text.into(),
    })
}

// These fixture folders contain one record at one of two known paths. Unlike
// an iCloud directory, the fixture can confirm absence with a local read.
fn observe(folder: &Path) -> Version {
    let mut result = None;
    for path in [CARD, ARCHIVED] {
        match fs::read_to_string(folder.join(path)) {
            Ok(text) => {
                assert!(result.is_none(), "record appears at two paths");
                result = document(path, &text);
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => panic!("cannot read fixture: {err}"),
        }
    }
    result
}

fn write(folder: &Path, path: &str, text: &str) {
    let path = folder.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

struct Fixture {
    root: tempfile::TempDir,
    desktop: PathBuf,
    cloud: PathBuf,
    base: Version,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let desktop = root.path().join("desktop");
        let cloud = root.path().join("cloud");
        for folder in [&desktop, &cloud] {
            write(folder, CARD, BASE);
        }
        Self {
            root,
            desktop,
            cloud,
            base: document(CARD, BASE),
        }
    }

    fn plan(&self) -> Plan {
        match reconcile(
            &self.base,
            Known(&observe(&self.desktop)),
            Known(&observe(&self.cloud)),
        )
        .unwrap()
        {
            Reconciliation::Ready(plan) => plan,
            other => panic!("expected a plan, got {other:?}"),
        }
    }

    fn step(&self, plan: &Plan) -> Option<Version> {
        let desktop = observe(&self.desktop);
        let cloud = observe(&self.cloud);
        match plan.next_step(Known(&desktop), Known(&cloud)) {
            NextStep::Converged(version) => Some(version.clone()),
            NextStep::Write {
                replica,
                expected,
                target,
            } => {
                let folder = match replica {
                    Replica::Desktop => &self.desktop,
                    Replica::Cloud => &self.cloud,
                };
                assert_eq!(observe(folder).as_ref(), expected.as_ref());
                if let Some(doc) = target {
                    write(folder, &doc.path, &doc.text);
                }
                if let Some(old) = expected {
                    if target.as_ref().is_none_or(|doc| doc.path != old.path) {
                        fs::remove_file(folder.join(&old.path)).unwrap();
                    }
                }
                None
            }
            other => panic!("unexpected step: {other:?}"),
        }
    }

    fn finish(&mut self, plan: &Plan) {
        for _ in 0..3 {
            if let Some(version) = self.step(plan) {
                self.base = version;
                return;
            }
        }
        panic!("plan failed to converge");
    }
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn round_trip_between_a_cloud_fixture_and_a_git_remote() {
    let mut f = Fixture::new();
    let origin = f.root.path().join("origin.git");
    let agent = f.root.path().join("agent");
    let origin = origin.to_str().unwrap();
    git(f.root.path(), &["init", "--bare", "-b", "main", origin]);
    git(&f.desktop, &["init", "-b", "main"]);
    git(&f.desktop, &["add", CARD]);
    git(&f.desktop, &["commit", "-m", "Base"]);
    git(&f.desktop, &["push", origin, "main"]);

    let captured = format!("{BASE}- [ ] Captured on phone\n");
    write(&f.cloud, CARD, &captured);
    f.finish(&f.plan());
    git(&f.desktop, &["add", CARD]);
    git(&f.desktop, &["commit", "-m", "Phone capture"]);
    git(&f.desktop, &["push", origin, "main"]);
    git(f.root.path(), &["clone", origin, agent.to_str().unwrap()]);
    assert_eq!(observe(&agent), document(CARD, &captured));

    let edited = captured.replace("status: active", "status: waiting");
    write(&agent, CARD, &edited);
    git(&agent, &["commit", "-am", "Agent edit"]);
    git(&agent, &["push", "origin", "main"]);
    git(&f.desktop, &["pull", "--ff-only", origin, "main"]);
    f.finish(&f.plan());
    assert_eq!(observe(&f.cloud), document(CARD, &edited));
    assert!(!f.cloud.join(".git").exists());
}

#[test]
fn only_known_absence_can_propagate_a_deletion() {
    let mut f = Fixture::new();
    fs::remove_file(f.cloud.join(CARD)).unwrap();
    let local = observe(&f.desktop);
    assert_eq!(
        reconcile(&f.base, Known(&local), Unavailable).unwrap(),
        Reconciliation::Waiting
    );
    assert_eq!(observe(&f.desktop), f.base);
    f.finish(&f.plan());
    assert_eq!(observe(&f.desktop), None);
    assert_eq!(f.base, None);
}

#[test]
fn resumes_a_saved_plan_for_concurrent_rename_and_text_edits() {
    let mut f = Fixture::new();
    let desktop_edit = BASE.replace("status: active", "status: waiting");
    let cloud_edit = BASE.replace("[ ] Draft", "[x] Draft");
    let merged = desktop_edit.replace("[ ] Draft", "[x] Draft");
    fs::remove_file(f.desktop.join(CARD)).unwrap();
    write(&f.desktop, ARCHIVED, &desktop_edit);
    write(&f.cloud, CARD, &cloud_edit);
    let journal = f.root.path().join("pending.json");
    {
        let plan = f.plan();
        fs::write(&journal, serde_json::to_vec(&plan).unwrap()).unwrap();
        assert_eq!(f.step(&plan), None);
    }
    assert_eq!(observe(&f.desktop), document(ARCHIVED, &merged));
    assert_eq!(observe(&f.cloud), document(CARD, &cloud_edit));
    assert_eq!(f.base, document(CARD, BASE));
    let resumed = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
    f.finish(&resumed);
    assert_eq!(observe(&f.cloud), document(ARCHIVED, &merged));
    assert_eq!(f.base, document(ARCHIVED, &merged));
}

#[test]
fn refuses_to_resume_a_plan_over_a_newer_edit() {
    let f = Fixture::new();
    let cloud_edit = BASE.replace("[ ] Draft", "[x] Draft");
    write(&f.cloud, CARD, &cloud_edit);
    let pending = f.plan();
    let desktop_edit = BASE.replace("status: active", "status: waiting");
    write(&f.desktop, CARD, &desktop_edit);
    assert_eq!(
        pending.next_step(Known(&observe(&f.desktop)), Known(&observe(&f.cloud))),
        NextStep::Stale
    );
    assert_eq!(observe(&f.desktop), document(CARD, &desktop_edit));
    assert_eq!(observe(&f.cloud), document(CARD, &cloud_edit));
}

#[test]
fn conflicting_changes_produce_no_write_plan() {
    let base = document(CARD, BASE);
    let desktop = document(CARD, &BASE.replace("status: active", "status: waiting"));
    let cloud = document(CARD, &BASE.replace("status: active", "status: done"));
    let renamed = document(ARCHIVED, BASE);
    let moved = document("other/project.md", BASE);
    for (base, desktop, cloud, conflict) in [
        (&base, &desktop, &cloud, Conflict::Content),
        (&base, &None, &cloud, Conflict::Deletion),
        (&base, &renamed, &moved, Conflict::Rename),
        (&None, &desktop, &cloud, Conflict::Creation),
    ] {
        assert_eq!(
            reconcile(base, Known(desktop), Known(cloud)).unwrap(),
            Reconciliation::Conflict(conflict)
        );
    }
}
