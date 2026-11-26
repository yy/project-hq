use std::fs;
use std::io::Write;
use std::path::Path;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::overview::OverviewManager;
use crate::project::Project;
use crate::project_file::project_body;
use crate::task::{ensure_task_files, resolve_task_path, validate_task_text, TODO_FILE};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn library(root: &Path, model: &str) -> Result<Value> {
    fs::read_dir(root)?;
    let config = Config::load(root);
    let cache = OverviewManager::new();
    let mut cards = Vec::new();
    for track in &config.tracks {
        // An unreadable track must not replace yesterday's complete library.
        let mut files = fs::read_dir(root.join(track))?.collect::<std::io::Result<Vec<_>>>()?;
        files.sort_by_key(|entry| entry.file_name());
        for entry in files {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !entry.file_type()?.is_file()
                || !name.ends_with(".md")
                || config.skip_files.contains(&name)
            {
                continue;
            }
            let file = format!("{track}/{name}");
            let text = fs::read_to_string(entry.path())?;
            let Some(p) = Project::from_text(&text, track, &file) else {
                continue;
            };
            let state = cache.state(root, model, &file, &text);
            let next = p
                .actions
                .iter()
                .find(|a| a.available && !a.completed)
                .map(|a| &a.text);
            cards.push(json!({
                "id": file, "title": p.title, "status": p.status, "notes": project_body(&text),
                "deferred_until": p.deferred_until, "next": next, "summary": state.cached,
                "stale": state.stale, "priority": p.priority
            }));
        }
    }
    let todos: Vec<_> = crate::task::load_tasks(root)?
        .into_iter()
        .filter(|task| task.completed.is_none())
        .map(|task| {
            json!({"line": task.line, "title": task.title, "due": task.due,
            "waiting": task.waiting, "deferred_until": task.deferred_until})
        })
        .collect();
    Ok(json!({"hq_mobile": 1, "projects": cards, "todos": todos}))
}

#[derive(Deserialize, Serialize)]
pub struct Capture {
    pub id: String,
    pub date: NaiveDate,
    pub text: String,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    payload: String,
    before: String,
    after: String,
    done: bool,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn save_receipt(dir: &Path, path: &Path, receipt: &Receipt) -> Result<()> {
    let temp = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer(temp.as_file(), receipt)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}

/// Called under the same mutation lock as desktop task edits. Only append to todo.txt.
pub fn receive(root: &Path, receipts: &Path, capture: &Capture) -> Result<()> {
    if capture.id.len() != 36
        || !capture.id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err("Invalid mobile capture ID".into());
    }
    if capture.text.trim().is_empty() || capture.text.contains(['\r', '\n']) {
        return Err("A mobile capture must be a single task title".into());
    }
    let line = format!("{} {}\n", capture.date, capture.text.trim());
    validate_task_text(TODO_FILE, &line)?;
    let payload = hash(line.as_bytes());
    let dir = receipts.join(hash(root.canonicalize()?.as_os_str().as_encoded_bytes()));
    fs::create_dir_all(&dir)?;
    let path = dir.join(capture.id.to_ascii_lowercase());
    ensure_task_files(root)?;
    let todo = resolve_task_path(root, TODO_FILE)?;
    let current = fs::read_to_string(&todo)?;
    let before = hash(current.as_bytes());
    let append = if current.is_empty() || current.ends_with('\n') {
        line
    } else {
        format!("\n{line}")
    };
    let mut receipt: Receipt = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let receipt = Receipt {
                payload: payload.clone(),
                before: before.clone(),
                after: hash([current.as_bytes(), append.as_bytes()].concat().as_slice()),
                done: false,
            };
            save_receipt(&dir, &path, &receipt)?;
            receipt
        }
        Err(error) => return Err(error.into()),
    };
    if receipt.payload != payload {
        return Err("Mobile capture ID reused with a different title or date".into());
    }
    if receipt.done {
        return Ok(());
    }
    if receipt.before == before {
        let mut out = fs::OpenOptions::new().append(true).open(&todo)?;
        out.write_all(append.as_bytes())?;
        out.sync_all()?;
    } else if receipt.after != before {
        return Err("Todos changed during an interrupted mobile import. Check that capture before retrying.".into());
    }
    receipt.done = true;
    save_receipt(&dir, &path, &receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_retries_do_not_recreate_edited_or_completed_tasks() {
        let vault = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let mut capture = Capture {
            id: "3b64fc53-6ce2-4731-959a-71974ffb0e60".into(),
            date: "2026-11-26".parse().unwrap(),
            text: "Confirm the meeting".into(),
        };
        receive(vault.path(), state.path(), &capture).unwrap();
        let todo = vault.path().join(TODO_FILE);
        assert_eq!(
            fs::read_to_string(&todo).unwrap(),
            "2026-11-26 Confirm the meeting\n"
        );
        fs::write(&todo, "A different task\n").unwrap();
        receive(vault.path(), state.path(), &capture).unwrap();
        capture.text = "Changed payload".into();
        assert!(receive(vault.path(), state.path(), &capture).is_err());
        assert_eq!(fs::read_to_string(todo).unwrap(), "A different task\n");
    }

    #[test]
    fn library_matches_configured_project_files_and_preserves_recorded_action() {
        let vault = tempfile::tempdir().unwrap();
        let root = vault.path();
        fs::create_dir(root.join("work")).unwrap();
        fs::write(
            root.join("hq.toml"),
            "tracks = ['work']\nskip_files = ['general.md']",
        )
        .unwrap();
        let src = "---\ntitle: Project\nstatus: my-plate\n---\n- [ ] Look at the draft";
        for name in ["one.md", "general.md"] {
            fs::write(root.join("work").join(name), src).unwrap();
        }
        let data = library(root, "test-model").unwrap();
        let projects = data["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["id"], "work/one.md");
        assert_eq!(projects[0]["next"], "Look at the draft");
    }
}
