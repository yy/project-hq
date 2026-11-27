use std::fs;
use std::io::Write;
use std::path::Path;

use chrono::{DateTime, FixedOffset, Local, NaiveDate};
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
                "stale": state.stale, "priority": p.priority, "revision": hash(text.as_bytes())
            }));
        }
    }
    let tasks = crate::task::load_tasks(root)?;
    let mut counts = std::collections::HashMap::new();
    for task in &tasks {
        *counts.entry(task.raw.as_str()).or_insert(0) += 1;
    }
    let todos: Vec<_> = tasks
        .iter()
        .filter(|task| task.completed.is_none())
        .map(|task| {
            json!({"line": task.line, "title": task.title, "due": task.due,
            "waiting": task.waiting, "deferred_until": task.deferred_until,
            "revision": (counts[task.raw.as_str()] == 1).then(|| hash(task.raw.as_bytes()))})
        })
        .collect();
    let routines = crate::routine::map_routines(root, |r, source| {
        json!({"id": r.file, "title": r.title, "available_on": r.available_on.iso8601(),
            "next_due": r.next_due.iso8601(), "deferred_until": r.deferred_until, "revision": hash(source.as_bytes())})
    });
    Ok(
        json!({"hq_mobile": 1, "projects": cards, "todos": todos, "statuses": config.statuses,
        "routines": routines}),
    )
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

fn save_receipt(dir: &Path, path: &Path, receipt: &impl Serialize) -> Result<()> {
    let temp = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer(temp.as_file(), receipt)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Called under the same mutation lock as desktop task edits. Only append to todo.txt.
pub fn receive(root: &Path, receipts: &Path, capture: &Capture) -> Result<()> {
    if !valid_id(&capture.id) {
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

#[derive(Serialize, Deserialize)]
pub struct Edit {
    id: String,
    project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    // Preserve hashes in older delivery receipts.
    routine: Option<String>,
    revision: String,
    #[serde(flatten)]
    change: Change,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Change {
    Move {
        status: String,
        waiting_on: Option<String>,
    },
    Defer {
        until: NaiveDate,
    },
    CompleteRoutine {
        at: DateTime<FixedOffset>,
    },
    SkipRoutine {
        at: DateTime<FixedOffset>,
    },
    SetWaiting {
        waiting: bool,
    },
    Complete {
        date: NaiveDate,
    },
}

#[derive(Serialize, Deserialize)]
pub struct EditResult {
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct EditReceipt {
    payload: String,
    result: EditResult,
}

/// A redelivered command must never reapply a move that the user has since undone.
pub fn receive_edit(
    root: &Path,
    receipts: &Path,
    undo: &crate::undo::UndoManager,
    edit: &Edit,
) -> Result<EditResult> {
    if !valid_id(&edit.id) {
        return Err("Invalid edit ID".into());
    }
    let dir = receipts
        .join(hash(root.canonicalize()?.as_os_str().as_encoded_bytes()))
        .join("edits");
    fs::create_dir_all(&dir)?;
    let path = dir.join(edit.id.to_ascii_lowercase());
    let payload = hash(&serde_json::to_vec(edit)?);
    match fs::read(&path) {
        Ok(data) => {
            let receipt: EditReceipt = serde_json::from_slice(&data)?;
            if receipt.payload != payload {
                return Err("Edit ID already used".into());
            }
            return Ok(receipt.result);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut receipt = EditReceipt {
        payload,
        result: EditResult {
            message: Some(
                "Delivery was interrupted. Check the current item before trying again.".into(),
            ),
        },
    };
    save_receipt(&dir, &path, &receipt)?;
    receipt.result.message = apply_edit(root, undo, edit).err().map(|e| e.to_string());
    save_receipt(&dir, &path, &receipt)?;
    Ok(receipt.result)
}

fn apply_edit(root: &Path, undo: &crate::undo::UndoManager, edit: &Edit) -> Result<()> {
    use crate::mover::{defer_project, move_project, MoveOptions};
    use crate::task::{complete_task, defer_task, load_tasks, update_task, TaskInput, DONE_FILE};
    let stale = "This item changed on the Mac. Refresh and try again.";
    let (draft, label) = if let Some(file) = &edit.routine {
        if edit.project.is_some() {
            return Err("Ambiguous mobile edit target.".into());
        }
        let source = crate::routine::read_routine_text(root, file)?;
        if hash(source.as_bytes()) != edit.revision {
            return Err(stale.into());
        }
        let mut routine =
            crate::routine::Routine::from_text_at(&source, file, Local::now().fixed_offset())?;
        let label = match &edit.change {
            Change::CompleteRoutine { at } => {
                routine.advance(at.with_timezone(&Local).fixed_offset(), true)?;
                "Complete routine"
            }
            Change::SkipRoutine { at } => {
                routine.advance(at.with_timezone(&Local).fixed_offset(), false)?;
                "Skip routine"
            }
            Change::Defer { until } => {
                routine.defer_until(&until.to_string())?;
                "Defer routine"
            }
            _ => return Err("This action is unavailable for this routine.".into()),
        };
        let draft = undo.capture_files(root, std::slice::from_ref(file))?;
        crate::routine::write_routine_text_if_unchanged(root, file, &source, &routine.to_text())?;
        (draft, label)
    } else if let Some(file) = &edit.project {
        let config = Config::load(root);
        let (track, name) = file.split_once('/').ok_or(stale)?;
        if !config.tracks.iter().any(|t| t == track)
            || name.contains('/')
            || config.skip_files.iter().any(|n| n == name)
        {
            return Err(stale.into());
        }
        let source = crate::project_file::read_project_text(root, file)?;
        if hash(source.as_bytes()) != edit.revision {
            return Err(stale.into());
        }
        let draft = undo.capture_files(root, std::slice::from_ref(file))?;
        let label = match &edit.change {
            Change::Move { status, waiting_on } if config.statuses.contains(status) => {
                move_project(
                    root,
                    &MoveOptions {
                        file: file.clone(),
                        to_status: status.clone(),
                        priority: None,
                        waiting_on: waiting_on.clone(),
                    },
                )?;
                "Move project"
            }
            Change::Defer { until } => {
                defer_project(root, file, &until.to_string())?;
                "Defer project"
            }
            _ => return Err("This action is unavailable for this project.".into()),
        };
        (draft, label)
    } else {
        // Line numbers shift when other todos move. Only act on a unique, exact source revision.
        let mut matches = load_tasks(root)?
            .into_iter()
            .filter(|t| t.completed.is_none() && hash(t.raw.as_bytes()) == edit.revision);
        let task = matches.next().ok_or(stale)?;
        if matches.next().is_some() {
            return Err("Two identical todos match. Edit one on the Mac first.".into());
        }
        let mut files = vec![TODO_FILE.to_owned()];
        if matches!(edit.change, Change::Complete { .. }) {
            ensure_task_files(root)?;
            files.push(DONE_FILE.to_owned());
        }
        let draft = undo.capture_files(root, &files)?;
        let label = match &edit.change {
            Change::Defer { until } => {
                defer_task(root, task.line, &task.raw, *until)?;
                "Defer task"
            }
            Change::SetWaiting { waiting } => {
                update_task(
                    root,
                    task.line,
                    &task.raw,
                    &TaskInput {
                        text: task.text,
                        priority: task.priority,
                        due: task.due,
                        deferred_until: task.deferred_until,
                        waiting: *waiting,
                    },
                )?;
                "Move task"
            }
            Change::Complete { date } => {
                complete_task(root, task.line, &task.raw, *date)?;
                "Complete task"
            }
            _ => return Err("This action is unavailable for this todo.".into()),
        };
        (draft, label)
    };
    undo.record_files(root, label, draft)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_edits_follow_unique_revisions_and_never_replay_after_undo() {
        let vault = tempfile::tempdir().unwrap();
        let receipts = tempfile::tempdir().unwrap();
        let root = vault.path();
        ensure_task_files(root).unwrap();
        let todo = root.join(TODO_FILE);
        let raw = "Review draft @desk p:40 due:2027-01-01";
        let reordered = format!("Another task\n{raw}\n");
        fs::write(&todo, &reordered).unwrap();
        let undo = crate::undo::UndoManager::new();
        let mut edit = Edit {
            id: "bc630788-f42f-45c1-ada4-aa1bfd8c1d77".into(),
            revision: hash(raw.as_bytes()),
            project: None,
            routine: None,
            change: Change::SetWaiting { waiting: true },
        };
        let run = |edit: &Edit| {
            receive_edit(root, receipts.path(), &undo, edit)
                .unwrap()
                .message
        };
        assert!(run(&edit).is_none());
        assert_eq!(
            fs::read_to_string(&todo).unwrap(),
            format!("Another task\n{raw} status:waiting\n")
        );
        fs::write(&todo, &reordered).unwrap();
        assert!(run(&edit).is_none());
        assert_eq!(fs::read_to_string(&todo).unwrap(), reordered);
        edit.id = "aff30788-f42f-45c1-ada4-aa1bfd8c1d77".into();
        let duplicate = format!("{raw}\n{raw}\n");
        fs::write(&todo, &duplicate).unwrap();
        assert!(run(&edit).is_some());
        assert_eq!(fs::read_to_string(&todo).unwrap(), duplicate);
    }

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
    fn offline_routine_actions_use_the_original_occurrence_and_share_desktop_undo() {
        let vault = tempfile::tempdir().unwrap();
        let receipts = tempfile::tempdir().unwrap();
        let root = vault.path();
        let file = "_routines/review.md";
        let source = "---\ntype: routine\ntitle: Review\narea: work\nrepeat: 2 hours\nrepeat_from: completion\navailable_before: 0 hours\nnext_due: 2026-11-23 09:00\n---\nNotes\n";
        crate::routine::write_new_routine_text(root, file, source).unwrap();
        fs::write(root.join("hq.toml"), "tracks = []").unwrap();
        let snapshot = library(root, "test").unwrap();
        let revision = &snapshot["routines"][0]["revision"];
        assert_eq!(revision, &hash(source.as_bytes()));
        let undo = crate::undo::UndoManager::new();
        let at = DateTime::parse_from_rfc3339("2026-11-25T15:45:00Z")
            .unwrap()
            .with_timezone(&Local)
            .fixed_offset();
        for (action, id) in [
            ("complete_routine", "02a24b38-a65f-4661-97a7-b626d75eb0e0"),
            ("skip_routine", "1021ad36-4862-4fb7-8297-7da59c6edaa3"),
            ("defer", "de946830-723a-4d85-8c68-6cde4e336a03"),
        ] {
            let payload = json!({"id":id, "routine":file, "action":action, "revision":revision, "at":at, "until":"2027-01-01"});
            let edit: Edit = serde_json::from_value(payload.clone()).unwrap();
            let run = |e: &Edit| {
                receive_edit(root, receipts.path(), &undo, e)
                    .unwrap()
                    .message
            };
            assert!(run(&edit).is_none());
            let changed = fs::read_to_string(root.join(file)).unwrap();
            if action == "defer" {
                assert!(changed.contains("deferred_until: 2027-01-01"));
                assert!(!changed.contains("## History"));
            } else {
                let event = if action == "complete_routine" {
                    "completed"
                } else {
                    "skipped"
                };
                assert!(changed.contains(&format!("{} — {event}", at.format("%Y-%m-%d %H:%M"))));
                assert_eq!(
                    changed.contains("last_completed:"),
                    action == "complete_routine"
                );
                assert!(changed.contains(&format!(
                    "next_due: {}",
                    (at + chrono::Duration::hours(2)).format("%Y-%m-%d %H:%M")
                )));
            }
            let mut stale = payload;
            stale["id"] = json!(format!("a{}", &id[1..]));
            assert!(run(&serde_json::from_value(stale).unwrap()).is_some());
            assert_eq!(fs::read_to_string(root.join(file)).unwrap(), changed);
            undo.undo(root).unwrap();
            assert!(run(&edit).is_none());
            assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
        }
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
