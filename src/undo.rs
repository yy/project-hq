use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::project_file::{
    read_project_text, resolve_project_path, write_project_text_if_unchanged, ProjectFileError,
};

const UNDO_LIMIT: usize = 50;

#[derive(Debug)]
pub enum UndoError {
    NothingToUndo,
    Conflict {
        file: String,
    },
    StateUnavailable,
    Remove {
        file: String,
        source: std::io::Error,
    },
    Project(ProjectFileError),
}

impl std::fmt::Display for UndoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToUndo => write!(f, "Nothing to undo"),
            Self::Conflict { file } => write!(
                f,
                "{file} changed after this action; reload before trying to undo it"
            ),
            Self::StateUnavailable => write!(f, "Undo history is unavailable"),
            Self::Remove { file, source } => {
                write!(f, "Could not undo creation of {file}: {source}")
            }
            Self::Project(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for UndoError {}

impl From<ProjectFileError> for UndoError {
    fn from(value: ProjectFileError) -> Self {
        Self::Project(value)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UndoStatus {
    pub available: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UndoResult {
    pub ok: bool,
    pub label: String,
    pub files: Vec<String>,
}

pub struct UndoDraft {
    files: Vec<FileRevision>,
}

struct FileRevision {
    file: String,
    before: Option<String>,
    after: String,
}

struct UndoEntry {
    label: String,
    files: Vec<FileRevision>,
}

#[derive(Clone, Default)]
pub struct UndoManager {
    entries: Arc<Mutex<VecDeque<UndoEntry>>>,
}

impl UndoManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn capture_files(&self, hq_dir: &Path, files: &[String]) -> Result<UndoDraft, UndoError> {
        let files = files
            .iter()
            .map(|file| {
                Ok(FileRevision {
                    file: file.clone(),
                    before: Some(read_project_text(hq_dir, file)?),
                    after: String::new(),
                })
            })
            .collect::<Result<_, UndoError>>()?;
        Ok(UndoDraft { files })
    }

    pub fn record_files(
        &self,
        hq_dir: &Path,
        label: impl Into<String>,
        draft: UndoDraft,
    ) -> Result<bool, UndoError> {
        self.record_files_and_created(hq_dir, label, draft, &[])
    }

    pub fn record_files_and_created(
        &self,
        hq_dir: &Path,
        label: impl Into<String>,
        mut draft: UndoDraft,
        created: &[String],
    ) -> Result<bool, UndoError> {
        for revision in &mut draft.files {
            revision.after = read_project_text(hq_dir, &revision.file)?;
        }
        draft
            .files
            .retain(|revision| revision.before.as_ref() != Some(&revision.after));
        for file in created {
            draft.files.push(FileRevision {
                file: file.clone(),
                before: None,
                after: read_project_text(hq_dir, file)?,
            });
        }
        if draft.files.is_empty() {
            return Ok(false);
        }
        self.push(UndoEntry {
            label: label.into(),
            files: draft.files,
        })?;
        Ok(true)
    }

    pub fn record_created(
        &self,
        hq_dir: &Path,
        file: &str,
        label: impl Into<String>,
    ) -> Result<(), UndoError> {
        let after = read_project_text(hq_dir, file)?;
        self.push(UndoEntry {
            label: label.into(),
            files: vec![FileRevision {
                file: file.to_string(),
                before: None,
                after,
            }],
        })
    }

    pub fn status(&self) -> UndoStatus {
        match self.entries.lock() {
            Ok(entries) => UndoStatus {
                available: !entries.is_empty(),
                label: entries.back().map(|entry| entry.label.clone()),
            },
            Err(_) => UndoStatus {
                available: false,
                label: None,
            },
        }
    }

    pub fn undo(&self, hq_dir: &Path) -> Result<UndoResult, UndoError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| UndoError::StateUnavailable)?;
        let entry = entries.back().ok_or(UndoError::NothingToUndo)?;

        for revision in &entry.files {
            let current = read_project_text(hq_dir, &revision.file)?;
            if current != revision.after {
                return Err(UndoError::Conflict {
                    file: revision.file.clone(),
                });
            }
        }

        for revision in &entry.files {
            if let Some(before) = &revision.before {
                write_project_text_if_unchanged(hq_dir, &revision.file, &revision.after, before)?;
            } else {
                let path = resolve_project_path(hq_dir, &revision.file)?;
                fs::remove_file(path).map_err(|source| UndoError::Remove {
                    file: revision.file.clone(),
                    source,
                })?;
            }
        }

        let entry = entries.pop_back().expect("checked non-empty undo history");
        Ok(UndoResult {
            ok: true,
            label: entry.label,
            files: entry
                .files
                .into_iter()
                .map(|revision| revision.file)
                .collect(),
        })
    }

    fn push(&self, entry: UndoEntry) -> Result<(), UndoError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| UndoError::StateUnavailable)?;
        if entries.len() == UNDO_LIMIT {
            entries.pop_front();
        }
        entries.push_back(entry);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{UndoError, UndoManager};

    fn write_project(root: &Path, file: &str, status: &str) {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            format!("---\ntitle: Test\nstatus: {status}\n---\n\nNotes.\n"),
        )
        .unwrap();
    }

    use std::path::Path;

    #[test]
    fn restores_the_last_recorded_file_revision() {
        let temp = tempdir().unwrap();
        let manager = UndoManager::new();
        write_project(temp.path(), "personal/test.md", "active");
        let files = vec!["personal/test.md".to_string()];
        let draft = manager.capture_files(temp.path(), &files).unwrap();
        write_project(temp.path(), "personal/test.md", "waiting");
        manager
            .record_files(temp.path(), "Move project", draft)
            .unwrap();

        assert_eq!(manager.status().label.as_deref(), Some("Move project"));
        let result = manager.undo(temp.path()).unwrap();

        assert_eq!(result.files, files);
        assert!(fs::read_to_string(temp.path().join("personal/test.md"))
            .unwrap()
            .contains("status: active"));
        assert!(!manager.status().available);
    }

    #[test]
    fn supports_multiple_undo_steps() {
        let temp = tempdir().unwrap();
        let manager = UndoManager::new();
        let files = vec!["personal/test.md".to_string()];
        write_project(temp.path(), &files[0], "active");

        let first = manager.capture_files(temp.path(), &files).unwrap();
        write_project(temp.path(), &files[0], "waiting");
        manager
            .record_files(temp.path(), "First move", first)
            .unwrap();
        let second = manager.capture_files(temp.path(), &files).unwrap();
        write_project(temp.path(), &files[0], "done");
        manager
            .record_files(temp.path(), "Second move", second)
            .unwrap();

        assert_eq!(manager.undo(temp.path()).unwrap().label, "Second move");
        assert_eq!(manager.undo(temp.path()).unwrap().label, "First move");
        assert!(fs::read_to_string(temp.path().join(&files[0]))
            .unwrap()
            .contains("status: active"));
    }

    #[test]
    fn refuses_to_overwrite_a_newer_external_edit() {
        let temp = tempdir().unwrap();
        let manager = UndoManager::new();
        let files = vec!["personal/test.md".to_string()];
        write_project(temp.path(), &files[0], "active");
        let draft = manager.capture_files(temp.path(), &files).unwrap();
        write_project(temp.path(), &files[0], "waiting");
        manager
            .record_files(temp.path(), "Move project", draft)
            .unwrap();
        write_project(temp.path(), &files[0], "done");

        let error = manager.undo(temp.path()).unwrap_err();

        assert!(matches!(error, UndoError::Conflict { .. }));
        assert!(manager.status().available);
        assert!(fs::read_to_string(temp.path().join(&files[0]))
            .unwrap()
            .contains("status: done"));
    }

    #[test]
    fn undoing_project_creation_removes_only_the_unchanged_file() {
        let temp = tempdir().unwrap();
        let manager = UndoManager::new();
        write_project(temp.path(), "personal/new.md", "active");
        manager
            .record_created(temp.path(), "personal/new.md", "Create project")
            .unwrap();

        manager.undo(temp.path()).unwrap();

        assert!(!temp.path().join("personal/new.md").exists());
    }

    #[test]
    fn one_undo_restores_a_mixed_multi_file_agent_update() {
        let temp = tempdir().unwrap();
        let manager = UndoManager::new();
        let existing = vec!["personal/test.md".to_string()];
        write_project(temp.path(), &existing[0], "active");
        let draft = manager.capture_files(temp.path(), &existing).unwrap();
        write_project(temp.path(), &existing[0], "waiting");
        write_project(temp.path(), "personal/new.md", "active");

        manager
            .record_files_and_created(
                temp.path(),
                "Apply agent update",
                draft,
                &["personal/new.md".to_string()],
            )
            .unwrap();
        let result = manager.undo(temp.path()).unwrap();

        assert_eq!(result.files.len(), 2);
        assert!(fs::read_to_string(temp.path().join(&existing[0]))
            .unwrap()
            .contains("status: active"));
        assert!(!temp.path().join("personal/new.md").exists());
    }

    #[test]
    fn no_op_changes_do_not_enter_history() {
        let temp = tempdir().unwrap();
        let manager = UndoManager::new();
        let files = vec!["personal/test.md".to_string()];
        write_project(temp.path(), &files[0], "active");
        let draft = manager.capture_files(temp.path(), &files).unwrap();

        assert!(!manager
            .record_files(temp.path(), "No change", draft)
            .unwrap());
        assert!(!manager.status().available);
    }
}
