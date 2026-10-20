//! HQ-owned chat history and drafts. Runtime events are saved before delivery to browsers.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::agent::{AgentError, AgentSessionInfo};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ChatUi {
    pub draft: String,
    pub context_file: Option<String>,
    pub task_context: Option<Value>,
    pub queue: Vec<QueuedMessage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QueuedMessage {
    pub id: String,
    pub message: String,
    pub context_file: Option<String>,
    pub context_task: Option<Value>,
    pub context_title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatEntry {
    pub id: String,
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub streaming: bool,
    pub item_id: Option<String>,
    pub context_title: Option<String>,
    #[serde(default)]
    pub context_kind: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SavedChat {
    pub thread_id: String,
    pub title: String,
    #[serde(default)]
    pub title_custom: bool,
    #[serde(default)]
    pub archived: bool,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub entries: Vec<ChatEntry>,
    pub ui: ChatUi,
    pub status: String,
    pub active_turn_id: Option<String>,
    pub error: String,
    pub sequence: u64,
    pub ui_revision: u64,
    pub created_at: i64,
    pub selected_at: i64,
    #[serde(default)]
    hidden_entries: Vec<String>,
}

impl SavedChat {
    pub fn visible(mut self) -> Self {
        self.entries
            .retain(|entry| !self.hidden_entries.contains(&entry.id));
        self.hidden_entries.clear();
        self
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct StoredChat {
    pub chat: SavedChat,
    pub workspace: String,
    pub base_files: BTreeMap<String, String>,
}

#[derive(Deserialize, Serialize)]
struct SessionFiles {
    workspace: String,
    base_files: BTreeMap<String, String>,
}

#[derive(Default, Deserialize)]
pub struct SaveChatUi {
    pub revision: u64,
    pub ui: ChatUi,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub clear_finished: bool,
    pub title: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Clone)]
pub struct ConversationStore {
    root: Option<PathBuf>,
    lock: Arc<Mutex<()>>,
}

fn storage_error(error: impl std::fmt::Display) -> AgentError {
    AgentError::Runtime(format!("Could not save or restore HQ chats: {error}"))
}

fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
}

impl ConversationStore {
    pub fn for_hq(hq_dir: &Path) -> Self {
        let canonical = hq_dir
            .canonicalize()
            .unwrap_or_else(|_| hq_dir.to_path_buf());
        let key = format!(
            "{:x}",
            Sha256::digest(canonical.to_string_lossy().as_bytes())
        );
        Self {
            root: crate::ai_settings::application_support_dir()
                .map(|root| root.join("net.yulab.project-hq/conversations").join(key)),
            lock: Arc::new(Mutex::new(())),
        }
    }

    #[cfg(test)]
    pub fn at(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            lock: Arc::new(Mutex::new(())),
        }
    }

    fn root(&self) -> Result<&Path, AgentError> {
        self.root
            .as_deref()
            .ok_or_else(|| storage_error("application settings directory unavailable"))
    }

    pub fn workspaces(&self) -> Result<PathBuf, AgentError> {
        let path = self.root()?.join("workspaces");
        fs::create_dir_all(&path).map_err(storage_error)?;
        Ok(path)
    }

    pub fn workspace(&self, key: &str) -> Result<PathBuf, AgentError> {
        if !valid_key(key) {
            return Err(storage_error("invalid workspace identifier"));
        }
        Ok(self.root()?.join("workspaces").join(key))
    }

    fn path(&self, id: &str) -> Result<PathBuf, AgentError> {
        if !valid_key(id) {
            return Err(storage_error("invalid conversation identifier"));
        }
        Ok(self.root()?.join("threads").join(format!("{id}.json")))
    }

    fn read_unlocked(&self, id: &str) -> Result<SavedChat, AgentError> {
        let bytes = fs::read(self.path(id)?).map_err(storage_error)?;
        let record: SavedChat = serde_json::from_slice(&bytes).map_err(storage_error)?;
        if record.thread_id != id {
            return Err(storage_error("conversation identifier mismatch"));
        }
        Ok(record)
    }

    fn session_path(&self, id: &str) -> Result<PathBuf, AgentError> {
        self.path(id)?;
        Ok(self.root()?.join("sessions").join(format!("{id}.json")))
    }

    fn write_json(path: &Path, value: &impl Serialize) -> Result<(), AgentError> {
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).map_err(storage_error)?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(storage_error)?;
        serde_json::to_writer(&mut file, value).map_err(storage_error)?;
        file.as_file().sync_all().map_err(storage_error)?;
        file.persist(path).map_err(storage_error)?;
        Ok(())
    }

    fn write_unlocked(&self, record: &SavedChat) -> Result<(), AgentError> {
        Self::write_json(&self.path(&record.thread_id)?, record)
    }

    pub fn chat(&self, id: &str) -> Result<SavedChat, AgentError> {
        let _lock = self.lock.lock().unwrap();
        self.read_unlocked(id)
    }

    pub fn read(&self, id: &str) -> Result<StoredChat, AgentError> {
        let _lock = self.lock.lock().unwrap();
        let chat = self.read_unlocked(id)?;
        let session: SessionFiles =
            serde_json::from_slice(&fs::read(self.session_path(id)?).map_err(storage_error)?)
                .map_err(storage_error)?;
        Ok(StoredChat {
            chat,
            workspace: session.workspace,
            base_files: session.base_files,
        })
    }

    pub fn save_session(
        &self,
        id: &str,
        base_files: BTreeMap<String, String>,
        model: String,
        effort: Option<String>,
    ) -> Result<(), AgentError> {
        let _lock = self.lock.lock().unwrap();
        let path = self.session_path(id)?;
        let mut session: SessionFiles =
            serde_json::from_slice(&fs::read(&path).map_err(storage_error)?)
                .map_err(storage_error)?;
        session.base_files = base_files;
        Self::write_json(&path, &session)?;
        let mut chat = self.read_unlocked(id)?;
        chat.model = model;
        chat.reasoning_effort = effort;
        chat.sequence += 1;
        self.write_unlocked(&chat)
    }

    pub fn update<T>(
        &self,
        id: &str,
        edit: impl FnOnce(&mut SavedChat) -> Result<T, AgentError>,
    ) -> Result<T, AgentError> {
        let _lock = self.lock.lock().unwrap();
        let mut record = self.read_unlocked(id)?;
        let value = edit(&mut record)?;
        record.sequence += 1;
        self.write_unlocked(&record)?;
        Ok(value)
    }

    pub fn create(
        &self,
        info: &AgentSessionInfo,
        workspace: String,
        base_files: BTreeMap<String, String>,
    ) -> Result<(), AgentError> {
        let _lock = self.lock.lock().unwrap();
        if self.path(&info.thread_id)?.exists() {
            return Err(storage_error("conversation already exists"));
        }
        let now = chrono::Utc::now().timestamp_millis();
        Self::write_json(
            &self.session_path(&info.thread_id)?,
            &SessionFiles {
                workspace,
                base_files,
            },
        )?;
        self.write_unlocked(&SavedChat {
            thread_id: info.thread_id.clone(),
            title: "New chat".into(),
            title_custom: false,
            archived: false,
            provider: info.provider.clone(),
            model: info.model.clone(),
            reasoning_effort: info.reasoning_effort.clone(),
            entries: Vec::new(),
            ui: ChatUi::default(),
            status: "idle".into(),
            active_turn_id: None,
            error: String::new(),
            sequence: 0,
            ui_revision: 0,
            created_at: now,
            selected_at: now,
            hidden_entries: Vec::new(),
        })
    }

    pub fn list(&self) -> Result<Vec<SavedChat>, AgentError> {
        let _lock = self.lock.lock().unwrap();
        let path = self.root()?.join("threads");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut chats = Vec::new();
        for entry in fs::read_dir(path).map_err(storage_error)? {
            let entry = entry.map_err(storage_error)?;
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                let id = entry
                    .path()
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                chats.push(self.read_unlocked(&id)?.visible());
            }
        }
        chats.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.thread_id.cmp(&b.thread_id))
        });
        Ok(chats)
    }

    pub fn recover_interrupted(&self) -> Result<(), AgentError> {
        for chat in self.list()? {
            if matches!(chat.status.as_str(), "running" | "applying") {
                self.update(&chat.thread_id, |record| {
                    record.status = "interrupted".into();
                    record.active_turn_id = None;
                    for entry in &mut record.entries { entry.streaming = false; }
                    record.error = "HQ stopped during this request. Send a new message to continue; queued requests remain paused.".into();
                    Ok(())
                })?;
            }
        }
        Ok(())
    }

    pub fn save_ui(&self, id: &str, req: SaveChatUi) -> Result<SavedChat, AgentError> {
        self.update(id, |record| {
            if record.ui_revision != req.revision {
                return Err(AgentError::Runtime(
                    "This chat was updated in another window. Reload before saving this draft."
                        .into(),
                ));
            }
            if let Some(title) = req.title {
                let title = title.trim();
                if title.is_empty()
                    || title.chars().count() > 120
                    || title.chars().any(char::is_control)
                {
                    return Err(AgentError::Runtime(
                        "Chat names must be 1–120 characters on one line.".into(),
                    ));
                }
                record.title = title.into();
                record.title_custom = true;
            }
            if let Some(archived) = req.archived {
                if archived && matches!(record.status.as_str(), "running" | "applying") {
                    return Err(AgentError::Runtime(
                        "Wait for this chat's current work to finish before archiving it.".into(),
                    ));
                }
                record.archived = archived;
            }
            let mut ui = req.ui;
            ui.queue.retain(|queued| {
                !record
                    .entries
                    .iter()
                    .any(|entry| entry.kind == "user" && entry.id == queued.id)
            });
            record.ui = ui;
            record.ui_revision += 1;
            if req.selected {
                record.selected_at = chrono::Utc::now().timestamp_millis();
            }
            if req.clear_finished {
                record.hidden_entries.extend(
                    record
                        .entries
                        .iter()
                        .filter(|entry| !entry.streaming)
                        .map(|entry| entry.id.clone()),
                );
                record.hidden_entries.sort();
                record.hidden_entries.dedup();
            }
            Ok(())
        })?;
        Ok(self.chat(id)?.visible())
    }

    pub fn prepare_turn(&self, id: &str, mut request: QueuedMessage) -> Result<(), AgentError> {
        self.update(id, |record| {
            if record.archived {
                return Err(AgentError::Runtime(
                    "Restore this archived chat before sending another request.".into(),
                ));
            }
            if matches!(record.status.as_str(), "running" | "applying" | "review") {
                return Err(AgentError::Runtime(
                    "Finish or review this chat's current work before sending another request."
                        .into(),
                ));
            }
            if record
                .entries
                .iter()
                .any(|entry| entry.kind == "user" && entry.id == request.id)
            {
                return Err(AgentError::Runtime(
                    "This request was already submitted. Reload the chat to see its result.".into(),
                ));
            }
            if let Some(queued) = record
                .ui
                .queue
                .iter()
                .find(|queued| queued.id == request.id)
            {
                request.context_title.clone_from(&queued.context_title);
            }
            if record.entries.is_empty() && !record.title_custom {
                record.title = request.context_title.clone().unwrap_or_else(|| {
                    request
                        .message
                        .lines()
                        .next()
                        .unwrap_or("New chat")
                        .chars()
                        .take(64)
                        .collect()
                });
            }
            record.entries.push(ChatEntry {
                id: request.id.clone(),
                kind: "user".into(),
                text: request.message,
                item_id: None,
                streaming: false,
                context_title: request.context_title,
                context_kind: request.context_task.as_ref().map(|task| {
                    if task.get("file").and_then(Value::as_str).is_some() {
                        "checklist"
                    } else {
                        "todo"
                    }
                    .into()
                }),
            });
            record.ui.queue.retain(|queued| queued.id != request.id);
            record.status = "running".into();
            record.error.clear();
            Ok(())
        })
    }

    pub fn observe(&self, event: &Value) -> Result<Option<u64>, AgentError> {
        let method = event["method"].as_str().unwrap_or("");
        if !matches!(
            method,
            "item/agentMessage/delta"
                | "item/started"
                | "item/completed"
                | "turn/started"
                | "turn/completed"
        ) {
            return Ok(None);
        }
        let Some(id) = crate::agent::event_thread_id(event) else {
            return Ok(None);
        };
        if !self.path(id)?.exists() {
            return Ok(None);
        }
        let params = &event["params"];
        let item = &params["item"];
        if method.starts_with("item/")
            && method != "item/agentMessage/delta"
            && item["type"] != "agentMessage"
        {
            return Ok(None);
        }
        self.update(id, |record| {
            if method.starts_with("item/") {
                let item_id = params["itemId"]
                    .as_str()
                    .or_else(|| item["id"].as_str())
                    .unwrap_or("");
                let position = record.entries.iter().position(|entry| {
                    entry.kind == "agent" && entry.item_id.as_deref() == Some(item_id)
                });
                let index = position.unwrap_or_else(|| {
                    record.entries.push(ChatEntry {
                        id: format!("agent-{item_id}"),
                        kind: "agent".into(),
                        text: String::new(),
                        item_id: Some(item_id.into()),
                        streaming: true,
                        context_title: None,
                        context_kind: None,
                    });
                    record.entries.len() - 1
                });
                let entry = &mut record.entries[index];
                if method == "item/agentMessage/delta" {
                    entry.text.push_str(params["delta"].as_str().unwrap_or(""));
                } else if let Some(text) = item["text"].as_str() {
                    entry.text = text.into();
                }
                entry.streaming = method != "item/completed";
            } else if method == "turn/started" {
                record.active_turn_id = params["turn"]["id"].as_str().map(str::to_string);
            } else {
                record.active_turn_id = None;
                for entry in &mut record.entries {
                    entry.streaming = false;
                }
                record.status = if params["turn"]["status"] == "completed" {
                    "review"
                } else {
                    "interrupted"
                }
                .into();
                if record.status == "interrupted" {
                    record.error = params["turn"]["error"]["message"]
                        .as_str()
                        .unwrap_or("Agent stopped. Send a new message to continue.")
                        .into();
                }
            }
            Ok(Some(record.sequence + 1))
        })
    }

    pub fn settle(&self, id: &str, error: Option<String>) -> Result<(), AgentError> {
        self.update(id, |record| {
            record.status = if error.is_some() {
                "interrupted"
            } else {
                "idle"
            }
            .into();
            record.error = error.unwrap_or_default();
            record.active_turn_id = None;
            Ok(())
        })
    }

    pub fn snapshot_event(&self, id: &str) -> Result<Value, AgentError> {
        Ok(json!({ "method": "hq/snapshot", "params": self.chat(id)?.visible() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ConversationStore) {
        let temp = tempfile::tempdir().unwrap();
        let store = ConversationStore::at(temp.path().to_path_buf());
        store
            .create(
                &AgentSessionInfo {
                    thread_id: "chat-1".into(),
                    provider: "test".into(),
                    model: "chosen-model".into(),
                    reasoning_effort: Some("high".into()),
                },
                "workspace-1".into(),
                BTreeMap::new(),
            )
            .unwrap();
        (temp, store)
    }

    fn queued(id: &str) -> QueuedMessage {
        QueuedMessage {
            id: id.into(),
            message: "Help with this".into(),
            context_file: Some("personal/project.md".into()),
            context_task: Some(json!({"line":3,"expected":"Review draft","title":"Review draft"})),
            context_title: Some("Review draft".into()),
        }
    }

    #[test]
    fn restart_preserves_drafts_context_model_and_offline_streamed_replies() {
        let (temp, store) = store();
        store
            .save_ui(
                "chat-1",
                SaveChatUi {
                    revision: 0,
                    selected: true,
                    clear_finished: false,
                    ui: ChatUi {
                        draft: "My unsent draft".into(),
                        context_file: Some("personal/project.md".into()),
                        task_context: queued("request-1").context_task,
                        queue: vec![queued("request-1"), queued("request-2")],
                    },
                    ..SaveChatUi::default()
                },
            )
            .unwrap();
        store.prepare_turn("chat-1", queued("request-1")).unwrap();
        for event in [
            json!({"method":"turn/started","params":{"threadId":"chat-1","turn":{"id":"turn-1"}}}),
            json!({"method":"item/agentMessage/delta","params":{"threadId":"chat-1","itemId":"reply","delta":"Hello "}}),
            json!({"method":"item/agentMessage/delta","params":{"threadId":"chat-1","itemId":"reply","delta":"again"}}),
        ] {
            store.observe(&event).unwrap();
        }
        let restarted = ConversationStore::at(temp.path().to_path_buf());
        restarted.recover_interrupted().unwrap();
        let saved = restarted.list().unwrap().remove(0);
        assert_eq!(saved.model, "chosen-model");
        assert_eq!(saved.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(saved.ui.draft, "My unsent draft");
        assert_eq!(
            saved.ui.context_file.as_deref(),
            Some("personal/project.md")
        );
        assert_eq!(saved.ui.task_context.unwrap()["title"], "Review draft");
        assert_eq!(saved.ui.queue.len(), 1);
        assert_eq!(saved.ui.queue[0].id, "request-2");
        assert_eq!(saved.entries[1].text, "Hello again");
        assert!(!saved.entries[1].streaming);
        assert_eq!(saved.status, "interrupted");
        assert!(saved.active_turn_id.is_none());
    }

    #[test]
    fn old_ui_saves_cannot_erase_replies_or_requeue_submitted_requests() {
        let (_temp, store) = store();
        store.prepare_turn("chat-1", queued("request-1")).unwrap();
        store.observe(&json!({"method":"item/completed","params":{"threadId":"chat-1","item":{"id":"reply","type":"agentMessage","text":"Finished"}}})).unwrap();
        store.observe(&json!({"method":"turn/completed","params":{"threadId":"chat-1","turn":{"status":"completed"}}})).unwrap();
        let ui = ChatUi {
            draft: "New draft".into(),
            queue: vec![queued("request-1")],
            ..ChatUi::default()
        };
        store
            .save_ui(
                "chat-1",
                SaveChatUi {
                    revision: 0,
                    ui: ui.clone(),
                    selected: false,
                    clear_finished: false,
                    ..SaveChatUi::default()
                },
            )
            .unwrap();
        assert!(store
            .save_ui(
                "chat-1",
                SaveChatUi {
                    revision: 0,
                    ui,
                    selected: false,
                    clear_finished: false,
                    ..SaveChatUi::default()
                }
            )
            .is_err());
        let saved = store.read("chat-1").unwrap();
        assert!(saved.chat.ui.queue.is_empty());
        assert_eq!(saved.chat.entries[1].text, "Finished");
        assert_eq!(saved.chat.status, "review");
        store.settle("chat-1", None).unwrap();
        store
            .save_ui(
                "chat-1",
                SaveChatUi {
                    revision: 1,
                    ui: ChatUi::default(),
                    selected: false,
                    clear_finished: true,
                    ..SaveChatUi::default()
                },
            )
            .unwrap();
        assert!(store.list().unwrap()[0].entries.is_empty());
        assert!(
            store.prepare_turn("chat-1", queued("request-1")).is_err(),
            "clearing visible history must not permit a duplicate execution"
        );
    }

    #[test]
    fn rename_and_archive_preserve_chat_contents_across_restart() {
        let (temp, store) = store();
        let ui = ChatUi {
            draft: "Keep this draft".into(),
            context_file: Some("personal/project.md".into()),
            queue: vec![queued("held-request")],
            ..ChatUi::default()
        };
        let saved = store
            .save_ui(
                "chat-1",
                SaveChatUi {
                    title: Some("  Review next steps  ".into()),
                    archived: Some(true),
                    ui: ui.clone(),
                    ..SaveChatUi::default()
                },
            )
            .unwrap();
        assert_eq!(saved.title, "Review next steps");
        assert!(saved.title_custom && saved.archived);
        let restarted = ConversationStore::at(temp.path().to_path_buf());
        let restored = restarted.read("chat-1").unwrap();
        assert_eq!(restored.chat.ui.draft, ui.draft);
        assert_eq!(restored.chat.ui.queue[0].id, "held-request");
        assert_eq!(restored.workspace, "workspace-1");
        assert!(restarted
            .prepare_turn("chat-1", queued("held-request"))
            .is_err());
        assert!(restarted
            .save_ui(
                "chat-1",
                SaveChatUi {
                    title: Some("Stale rename".into()),
                    ..SaveChatUi::default()
                }
            )
            .is_err());
        let saved = restarted
            .save_ui(
                "chat-1",
                SaveChatUi {
                    revision: 1,
                    ui,
                    archived: Some(false),
                    ..SaveChatUi::default()
                },
            )
            .unwrap();
        assert_eq!(saved.status, "idle");
        assert_eq!(
            saved.ui.queue.len(),
            1,
            "restoring must not submit queued work"
        );
        restarted
            .prepare_turn("chat-1", queued("held-request"))
            .unwrap();
        assert_eq!(
            restarted.chat("chat-1").unwrap().title,
            "Review next steps",
            "the first prompt must preserve a custom name"
        );
        assert!(
            restarted
                .save_ui(
                    "chat-1",
                    SaveChatUi {
                        revision: 2,
                        archived: Some(true),
                        ..SaveChatUi::default()
                    }
                )
                .is_err(),
            "running work must not be hidden by archiving"
        );
        let saved = restarted.chat("chat-1").unwrap();
        assert!(!saved.archived);
        assert_eq!(saved.ui_revision, 2);
        assert_eq!(saved.entries[0].id, "held-request");
    }

    #[test]
    fn legacy_chats_remain_active_and_invalid_names_leave_them_unchanged() {
        let (temp, store) = store();
        let path = temp.path().join("threads/chat-1.json");
        let mut legacy: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        legacy.as_object_mut().unwrap().remove("archived");
        legacy.as_object_mut().unwrap().remove("title_custom");
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let saved = store.chat("chat-1").unwrap();
        assert!(!saved.archived && !saved.title_custom);
        for title in ["  ".into(), "Name\nwith a newline".into(), "x".repeat(121)] {
            assert!(store
                .save_ui(
                    "chat-1",
                    SaveChatUi {
                        title: Some(title),
                        ..SaveChatUi::default()
                    }
                )
                .is_err());
        }
        let saved = store.chat("chat-1").unwrap();
        assert_eq!(saved.title, "New chat");
        assert_eq!(saved.ui_revision, 0);
    }

    #[test]
    fn corrupt_records_and_invalid_paths_are_preserved_and_reported() {
        let (temp, store) = store();
        assert!(store.read("../escape").is_err());
        assert!(store.workspace("../../escape").is_err());
        let path = temp.path().join("threads/chat-1.json");
        fs::write(&path, "corrupt saved chat").unwrap();
        assert!(store.list().is_err());
        assert!(store.recover_interrupted().is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "corrupt saved chat");
    }
}
