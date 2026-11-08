//! Per-document reconciliation for the mobile sync prototype.
//!
//! The caller matches stable record identities and supplies observations. The
//! planner never scans or writes either replica. A transport must coordinate
//! writes, validate paths and contents, and persist plans before applying them.

use serde::{Deserialize, Serialize};
use std::{fs, io, process::Command};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub path: String,
    pub text: String,
}

/// None means known absent, including an explicit deletion.
pub type Version = Option<Document>;

#[derive(Clone, Copy)]
pub enum Observation<'a> {
    Known(&'a Version),
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Conflict {
    Creation,
    Deletion,
    Rename,
    Content,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reconciliation {
    Waiting,
    Conflict(Conflict),
    Ready(Plan),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    desktop: Version,
    cloud: Version,
    target: Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replica {
    Desktop,
    Cloud,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NextStep<'a> {
    Waiting,
    Stale,
    Write {
        replica: Replica,
        expected: &'a Version,
        target: &'a Version,
    },
    Converged(&'a Version),
}

/// Plan from the last common version. Conflicts leave both versions intact.
pub fn reconcile(
    base: &Version,
    desktop: Observation<'_>,
    cloud: Observation<'_>,
) -> io::Result<Reconciliation> {
    use Reconciliation::{Conflict as Unresolved, Ready, Waiting};

    let (Observation::Known(desktop), Observation::Known(cloud)) = (desktop, cloud) else {
        return Ok(Waiting);
    };
    let target = if let Some(version) = choose(base, desktop, cloud) {
        version.clone()
    } else {
        let (Some(desktop), Some(cloud)) = (desktop, cloud) else {
            return Ok(Unresolved(Conflict::Deletion));
        };
        let Some(base) = base else {
            return Ok(Unresolved(Conflict::Creation));
        };
        let Some(path) = choose(&base.path, &desktop.path, &cloud.path) else {
            return Ok(Unresolved(Conflict::Rename));
        };
        let text = if let Some(text) = choose(&base.text, &desktop.text, &cloud.text) {
            text.clone()
        } else {
            let Some(text) = merge_text(&base.text, &desktop.text, &cloud.text)? else {
                return Ok(Unresolved(Conflict::Content));
            };
            text
        };
        Some(Document {
            path: path.clone(),
            text,
        })
    };
    Ok(Ready(Plan {
        desktop: desktop.clone(),
        cloud: cloud.clone(),
        target,
    }))
}

impl Plan {
    /// Recheck both copies before each write, including when resuming a saved
    /// plan. Apply a write only under a fresh expected-version check. Advance
    /// the common version after both replicas are observed at the target.
    pub fn next_step(&self, desktop: Observation<'_>, cloud: Observation<'_>) -> NextStep<'_> {
        let (Observation::Known(desktop), Observation::Known(cloud)) = (desktop, cloud) else {
            return NextStep::Waiting;
        };
        if (desktop != &self.desktop && desktop != &self.target)
            || (cloud != &self.cloud && cloud != &self.target)
        {
            return NextStep::Stale;
        }
        let (replica, expected) = if desktop != &self.target {
            (Replica::Desktop, &self.desktop)
        } else if cloud != &self.target {
            (Replica::Cloud, &self.cloud)
        } else {
            return NextStep::Converged(&self.target);
        };
        NextStep::Write {
            replica,
            expected,
            target: &self.target,
        }
    }
}

fn choose<'a, T: Eq>(base: &T, desktop: &'a T, cloud: &'a T) -> Option<&'a T> {
    if desktop == cloud || cloud == base {
        Some(desktop)
    } else if desktop == base {
        Some(cloud)
    } else {
        None
    }
}

fn merge_text(base: &str, desktop: &str, cloud: &str) -> io::Result<Option<String>> {
    let dir = tempfile::tempdir()?;
    for (name, text) in [("base", base), ("desktop", desktop), ("cloud", cloud)] {
        fs::write(dir.path().join(name), text)?;
    }
    let merged = Command::new("git")
        .current_dir(dir.path())
        .args(["merge-file", "--stdout", "desktop", "base", "cloud"])
        .output()?;
    match merged.status.code() {
        Some(0) => String::from_utf8(merged.stdout)
            .map(Some)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
        Some(1..=127) if merged.stderr.is_empty() => Ok(None),
        _ => Err(io::Error::other(format!(
            "Git could not merge document: {}",
            String::from_utf8_lossy(&merged.stderr).trim()
        ))),
    }
}
