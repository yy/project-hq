use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};
use similar::TextDiff;
use tempfile::{Builder, TempDir};
use tokio::sync::broadcast;

use crate::config::Config;
use crate::project::Project;
use crate::project_file::{
    read_project_text, validate_project_file_for_rewrite, validate_project_text,
    write_new_project_text, write_project_text_if_unchanged, ProjectFileError,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const AGENT_INSTRUCTIONS: &str = "\
You are the coding agent embedded in the HQ project manager. The working directory is an \
isolated snapshot of the user's HQ Markdown repository. Read files and use shell tools as \
needed, like a coding agent rather than a generic chat assistant. You may reason across the whole \
repository and create or edit Markdown project files in existing track directories. Do not edit \
configuration or non-project files. Do not commit, push, move, rename, or delete files. When the \
user asks for updates, edit the relevant project files directly. The HQ app automatically applies \
valid project-file changes to the live repository when the turn completes and makes them available \
through Undo. Keep final answers concise.";

#[derive(Debug)]
pub enum AgentError {
    RuntimeUnavailable(String),
    Runtime(String),
    InvalidResponse(String),
    UnknownSession,
    NoChange(String),
    UnsupportedChange(String),
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Project(ProjectFileError),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RuntimeUnavailable(message) | Self::Runtime(message) => write!(f, "{message}"),
            Self::InvalidResponse(message) => {
                write!(f, "Codex returned an invalid response: {message}")
            }
            Self::UnknownSession => write!(f, "No HQ agent session"),
            Self::NoChange(file) => write!(f, "The agent has no proposed change for {file}"),
            Self::UnsupportedChange(message) => write!(f, "{message}"),
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
            Self::Project(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<ProjectFileError> for AgentError {
    fn from(value: ProjectFileError) -> Self {
        Self::Project(value)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentRuntimeStatus {
    pub available: bool,
    pub provider: &'static str,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentSessionInfo {
    pub thread_id: String,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentTurnInfo {
    pub thread_id: String,
    pub turn_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentFileChangeKind {
    Modified,
    Created,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentFileChange {
    pub file: String,
    pub kind: AgentFileChangeKind,
    pub diff: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentChange {
    pub changed: bool,
    pub diff: String,
    pub files: Vec<AgentFileChange>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentApplyResult {
    pub ok: bool,
    pub files: Vec<String>,
    pub created: Vec<String>,
}

#[derive(Clone)]
pub struct AgentManager {
    inner: Arc<AgentManagerInner>,
}

struct AgentManagerInner {
    client: Mutex<Option<Arc<CodexClient>>>,
    session: Mutex<Option<AgentSession>>,
    events: broadcast::Sender<Value>,
}

struct AgentSession {
    _workspace: TempDir,
    workspace_path: PathBuf,
    thread_id: String,
    provider: String,
    model: String,
    scope: WorkspaceScope,
    base_files: BTreeMap<String, String>,
}

#[derive(Clone)]
struct WorkspaceScope {
    tracks: Vec<String>,
    skip_files: HashSet<String>,
}

struct PendingChange {
    public: AgentChange,
    candidates: BTreeMap<String, String>,
}

impl AgentManager {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(512);
        Self {
            inner: Arc::new(AgentManagerInner {
                client: Mutex::new(None),
                session: Mutex::new(None),
                events,
            }),
        }
    }

    pub fn runtime_status(&self) -> AgentRuntimeStatus {
        match codex_version() {
            Ok(version) => AgentRuntimeStatus {
                available: true,
                provider: "codex",
                version: Some(version),
                error: None,
            },
            Err(error) => AgentRuntimeStatus {
                available: false,
                provider: "codex",
                version: None,
                error: Some(error.to_string()),
            },
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.inner.events.subscribe()
    }

    pub fn start_session(&self, hq_dir: &Path) -> Result<AgentSessionInfo, AgentError> {
        if let Some(info) = self.session_info()? {
            return Ok(info);
        }

        let scope = WorkspaceScope::from_dir(hq_dir);
        let base_files = collect_project_files(hq_dir, &scope)?;
        let workspace = create_workspace(hq_dir)?;
        let workspace_path = workspace.path().to_path_buf();
        if collect_project_files(&workspace_path, &scope)? != base_files {
            return Err(AgentError::Runtime(
                "HQ changed while the agent workspace was being prepared; try again".into(),
            ));
        }

        let client = self.client()?;
        let result = client.request(
            "thread/start",
            json!({
                "cwd": workspace_path,
                "sandbox": "workspace-write",
                "approvalPolicy": "never",
                "approvalsReviewer": "user",
                "developerInstructions": AGENT_INSTRUCTIONS,
                "ephemeral": true
            }),
        )?;
        let thread_id = result
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::InvalidResponse("thread/start omitted thread.id".into()))?
            .to_string();
        let provider = result
            .get("modelProvider")
            .and_then(Value::as_str)
            .unwrap_or("openai")
            .to_string();
        let model = result
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_string();

        let info = AgentSessionInfo {
            thread_id: thread_id.clone(),
            provider: provider.clone(),
            model: model.clone(),
        };
        let session = AgentSession {
            _workspace: workspace,
            workspace_path,
            thread_id,
            provider,
            model,
            scope,
            base_files,
        };
        self.inner
            .session
            .lock()
            .map_err(|_| AgentError::Runtime("Agent session lock was poisoned".into()))?
            .replace(session);

        Ok(info)
    }

    pub fn start_turn(
        &self,
        hq_dir: &Path,
        message: &str,
        context_file: Option<&str>,
    ) -> Result<AgentTurnInfo, AgentError> {
        if message.trim().is_empty() {
            return Err(AgentError::Runtime("Agent message cannot be empty".into()));
        }
        self.start_session(hq_dir)?;
        let thread_id = {
            let mut session = self
                .inner
                .session
                .lock()
                .map_err(|_| AgentError::Runtime("Agent session lock was poisoned".into()))?;
            let session = session.as_mut().ok_or(AgentError::UnknownSession)?;
            session.sync_from_live_if_clean(hq_dir)?;
            session.thread_id.clone()
        };
        let message = if let Some(file) = context_file.filter(|file| !file.trim().is_empty()) {
            read_project_text(hq_dir, file)?;
            format!(
                "The user is currently viewing `{file}` beside this conversation. Treat it as \
likely context, but the request may concern any part of HQ. Read the file from the workspace \
before answering.\n\n{message}"
            )
        } else {
            message.to_string()
        };
        let result = self.client()?.request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "input": [{
                    "type": "text",
                    "text": message,
                    "text_elements": []
                }]
            }),
        )?;
        let turn_id = result
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::InvalidResponse("turn/start omitted turn.id".into()))?
            .to_string();

        Ok(AgentTurnInfo { thread_id, turn_id })
    }

    pub fn interrupt(&self, thread_id: &str, turn_id: &str) -> Result<(), AgentError> {
        self.client()?.request(
            "turn/interrupt",
            json!({ "threadId": thread_id, "turnId": turn_id }),
        )?;
        Ok(())
    }

    pub fn change(&self) -> Result<AgentChange, AgentError> {
        let session = self
            .inner
            .session
            .lock()
            .map_err(|_| AgentError::Runtime("Agent session lock was poisoned".into()))?;
        let session = session.as_ref().ok_or(AgentError::UnknownSession)?;
        Ok(session.pending_change()?.public)
    }

    pub fn apply_changes(&self, hq_dir: &Path) -> Result<AgentApplyResult, AgentError> {
        let mut session = self
            .inner
            .session
            .lock()
            .map_err(|_| AgentError::Runtime("Agent session lock was poisoned".into()))?;
        let session = session.as_mut().ok_or(AgentError::UnknownSession)?;
        let pending = session.pending_change()?;
        if !pending.public.changed {
            return Err(AgentError::NoChange("HQ".to_string()));
        }

        for change in &pending.public.files {
            match change.kind {
                AgentFileChangeKind::Modified => {
                    validate_project_file_for_rewrite(hq_dir, &change.file)?;
                    let base = session.base_files.get(&change.file).ok_or_else(|| {
                        AgentError::InvalidResponse(format!(
                            "Missing agent baseline for {}",
                            change.file
                        ))
                    })?;
                    let live = read_project_text(hq_dir, &change.file)?;
                    if &live != base {
                        return Err(ProjectFileError::RevisionConflict {
                            file: change.file.clone(),
                        }
                        .into());
                    }
                }
                AgentFileChangeKind::Created => {
                    let path = hq_dir.join(&change.file);
                    if path.exists() {
                        return Err(ProjectFileError::AlreadyExists {
                            kind: "project",
                            name: change.file.clone(),
                        }
                        .into());
                    }
                }
            }
        }

        let mut files = Vec::new();
        let mut created = Vec::new();
        for change in &pending.public.files {
            let candidate = pending.candidates.get(&change.file).ok_or_else(|| {
                AgentError::InvalidResponse(format!(
                    "Missing proposed contents for {}",
                    change.file
                ))
            })?;
            match change.kind {
                AgentFileChangeKind::Modified => {
                    let base = session
                        .base_files
                        .get(&change.file)
                        .expect("validated above");
                    write_project_text_if_unchanged(hq_dir, &change.file, base, candidate)?;
                }
                AgentFileChangeKind::Created => {
                    write_new_project_text(hq_dir, &change.file, candidate)?;
                    created.push(change.file.clone());
                }
            }
            session
                .base_files
                .insert(change.file.clone(), candidate.clone());
            files.push(change.file.clone());
        }
        commit_workspace_baseline(&session.workspace_path, "Apply HQ agent changes")?;

        Ok(AgentApplyResult {
            ok: true,
            files,
            created,
        })
    }

    pub fn reject_changes(&self) -> Result<(), AgentError> {
        let session = self
            .inner
            .session
            .lock()
            .map_err(|_| AgentError::Runtime("Agent session lock was poisoned".into()))?;
        let session = session.as_ref().ok_or(AgentError::UnknownSession)?;
        if !workspace_is_dirty(&session.workspace_path)? {
            return Err(AgentError::NoChange("HQ".to_string()));
        }
        reset_workspace(&session.workspace_path)
    }

    pub fn invalidate_session(&self) {
        if let Ok(mut session) = self.inner.session.lock() {
            session.take();
        }
    }

    fn session_info(&self) -> Result<Option<AgentSessionInfo>, AgentError> {
        let session = self
            .inner
            .session
            .lock()
            .map_err(|_| AgentError::Runtime("Agent session lock was poisoned".into()))?;
        Ok(session.as_ref().map(|session| AgentSessionInfo {
            thread_id: session.thread_id.clone(),
            provider: session.provider.clone(),
            model: session.model.clone(),
        }))
    }

    fn client(&self) -> Result<Arc<CodexClient>, AgentError> {
        let mut client = self
            .inner
            .client
            .lock()
            .map_err(|_| AgentError::Runtime("Agent runtime lock was poisoned".into()))?;
        if let Some(existing) = client.as_ref() {
            return Ok(existing.clone());
        }
        let started = Arc::new(CodexClient::start(self.inner.events.clone())?);
        *client = Some(started.clone());
        Ok(started)
    }
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentSession {
    fn pending_change(&self) -> Result<PendingChange, AgentError> {
        let candidates = collect_project_files(&self.workspace_path, &self.scope)?;
        let changed_paths = changed_workspace_paths(&self.workspace_path)?;
        let allowed: HashSet<&str> = self
            .base_files
            .keys()
            .chain(candidates.keys())
            .map(String::as_str)
            .collect();
        let unsupported: Vec<_> = changed_paths
            .iter()
            .filter(|file| !allowed.contains(file.as_str()))
            .cloned()
            .collect();
        if !unsupported.is_empty() {
            return Err(AgentError::UnsupportedChange(format!(
                "The agent changed unsupported files: {}. Reject the workspace and try again.",
                unsupported.join(", ")
            )));
        }

        for file in self.base_files.keys() {
            if !candidates.contains_key(file) {
                return Err(AgentError::UnsupportedChange(format!(
                    "The agent deleted or invalidated {file}; project deletion is not supported"
                )));
            }
        }

        let mut files = Vec::new();
        let mut combined_diff = String::new();
        for (file, candidate) in &candidates {
            let (kind, before) = match self.base_files.get(file) {
                Some(base) if base == candidate => continue,
                Some(base) => (AgentFileChangeKind::Modified, base.as_str()),
                None => (AgentFileChangeKind::Created, ""),
            };
            let before_header = if kind == AgentFileChangeKind::Created {
                "/dev/null".to_string()
            } else {
                format!("a/{file}")
            };
            let diff = TextDiff::from_lines(before, candidate)
                .unified_diff()
                .header(&before_header, &format!("b/{file}"))
                .to_string();
            combined_diff.push_str(&diff);
            if !combined_diff.ends_with('\n') {
                combined_diff.push('\n');
            }
            files.push(AgentFileChange {
                file: file.clone(),
                kind,
                diff,
            });
        }

        Ok(PendingChange {
            public: AgentChange {
                changed: !files.is_empty(),
                diff: combined_diff,
                files,
            },
            candidates,
        })
    }

    fn sync_from_live_if_clean(&mut self, hq_dir: &Path) -> Result<(), AgentError> {
        if workspace_is_dirty(&self.workspace_path)? {
            return Ok(());
        }
        copy_workspace(hq_dir, &self.workspace_path, true)?;
        self.scope = WorkspaceScope::from_dir(hq_dir);
        self.base_files = collect_project_files(hq_dir, &self.scope)?;
        commit_workspace_baseline(&self.workspace_path, "Refresh HQ agent snapshot")
    }
}

impl WorkspaceScope {
    fn from_dir(hq_dir: &Path) -> Self {
        let config = Config::load(hq_dir);
        Self {
            tracks: config.tracks,
            skip_files: config.skip_files.into_iter().collect(),
        }
    }
}

fn collect_project_files(
    root: &Path,
    scope: &WorkspaceScope,
) -> Result<BTreeMap<String, String>, AgentError> {
    let mut projects = BTreeMap::new();
    for track in &scope.tracks {
        let track_dir = root.join(track);
        if !track_dir.is_dir() {
            continue;
        }
        let entries = fs::read_dir(&track_dir).map_err(|source| AgentError::Io {
            operation: "read HQ track",
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| AgentError::Io {
                operation: "read HQ track entry",
                source,
            })?;
            if !entry
                .file_type()
                .map_err(|source| AgentError::Io {
                    operation: "inspect HQ track entry",
                    source,
                })?
                .is_file()
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".md") || scope.skip_files.contains(&name) {
                continue;
            }
            let file = format!("{track}/{name}");
            let text = fs::read_to_string(entry.path()).map_err(|source| AgentError::Io {
                operation: "read HQ project candidate",
                source,
            })?;
            if Project::from_text(&text, track, &file).is_none() {
                continue;
            }
            validate_project_text(&file, &text)?;
            projects.insert(file, text);
        }
    }
    Ok(projects)
}

fn changed_workspace_paths(workspace: &Path) -> Result<Vec<String>, AgentError> {
    let tracked = Command::new("/usr/bin/git")
        .args(["diff", "--name-only", "--relative", "HEAD"])
        .current_dir(workspace)
        .output()
        .map_err(|source| AgentError::Io {
            operation: "inspect agent workspace changes",
            source,
        })?;
    if !tracked.status.success() {
        return Err(AgentError::Runtime(format!(
            "Could not inspect agent workspace changes (git exited with {})",
            tracked.status
        )));
    }
    let untracked = Command::new("/usr/bin/git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(workspace)
        .output()
        .map_err(|source| AgentError::Io {
            operation: "inspect new agent workspace files",
            source,
        })?;
    if !untracked.status.success() {
        return Err(AgentError::Runtime(format!(
            "Could not inspect new agent workspace files (git exited with {})",
            untracked.status
        )));
    }

    let mut paths: Vec<String> = String::from_utf8_lossy(&tracked.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&untracked.stdout).lines())
        .map(str::to_string)
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn workspace_is_dirty(workspace: &Path) -> Result<bool, AgentError> {
    Ok(!changed_workspace_paths(workspace)?.is_empty())
}

fn copy_workspace(
    source_dir: &Path,
    destination_dir: &Path,
    delete: bool,
) -> Result<(), AgentError> {
    let source = format!("{}/", source_dir.display());
    let destination = format!("{}/", destination_dir.display());
    let mut command = Command::new("/usr/bin/rsync");
    command
        .arg("-a")
        .arg("--exclude=.git/")
        .arg("--exclude=.hq/conversations/");
    if delete {
        command.arg("--delete");
    }
    let status = command
        .arg(source)
        .arg(destination)
        .status()
        .map_err(|source| AgentError::Io {
            operation: "copy HQ into agent workspace",
            source,
        })?;
    if !status.success() {
        return Err(AgentError::Runtime(format!(
            "Could not prepare the agent workspace (rsync exited with {status})"
        )));
    }
    Ok(())
}

fn commit_workspace_baseline(workspace: &Path, message: &str) -> Result<(), AgentError> {
    let status = Command::new("/usr/bin/git")
        .args(["add", "-A"])
        .current_dir(workspace)
        .status()
        .map_err(|source| AgentError::Io {
            operation: "stage agent workspace baseline",
            source,
        })?;
    if !status.success() {
        return Err(AgentError::Runtime(format!(
            "Could not stage the agent workspace baseline (git exited with {status})"
        )));
    }

    let status = Command::new("/usr/bin/git")
        .args([
            "-c",
            "user.name=HQ",
            "-c",
            "user.email=hq@localhost",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            message,
        ])
        .current_dir(workspace)
        .status()
        .map_err(|source| AgentError::Io {
            operation: "commit agent workspace baseline",
            source,
        })?;
    if !status.success() {
        return Err(AgentError::Runtime(format!(
            "Could not commit the agent workspace baseline (git exited with {status})"
        )));
    }
    Ok(())
}

fn reset_workspace(workspace: &Path) -> Result<(), AgentError> {
    for args in [
        &["reset", "--hard", "-q", "HEAD"][..],
        &["clean", "-fdxq"][..],
    ] {
        let status = Command::new("/usr/bin/git")
            .args(args)
            .current_dir(workspace)
            .status()
            .map_err(|source| AgentError::Io {
                operation: "discard agent workspace changes",
                source,
            })?;
        if !status.success() {
            return Err(AgentError::Runtime(format!(
                "Could not discard agent workspace changes (git exited with {status})"
            )));
        }
    }
    Ok(())
}

struct CodexClient {
    io: Arc<CodexIo>,
    child: Mutex<Child>,
}

struct CodexIo {
    stdin: Mutex<ChildStdin>,
    pending: Mutex<HashMap<String, mpsc::SyncSender<Value>>>,
    next_id: AtomicU64,
    events: broadcast::Sender<Value>,
}

impl CodexClient {
    fn start(events: broadcast::Sender<Value>) -> Result<Self, AgentError> {
        let mut command = codex_command();
        command
            .arg("app-server")
            .arg("--listen")
            .arg("stdio://")
            .env("PATH", augmented_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().map_err(|source| {
            AgentError::RuntimeUnavailable(format!(
                "Could not start Codex. Install or repair the Codex CLI: {source}"
            ))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentError::Runtime("Codex stdin was unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::Runtime("Codex stdout was unavailable".into()))?;
        let io = Arc::new(CodexIo {
            stdin: Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            events,
        });
        spawn_codex_reader(stdout, io.clone());

        let client = Self {
            io,
            child: Mutex::new(child),
        };
        client.request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "project_hq",
                    "title": "HQ",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "experimentalApi": true,
                    "requestAttestation": false
                }
            }),
        )?;
        client.notify("initialized", json!({}))?;
        Ok(client)
    }

    fn request(&self, method: &str, params: Value) -> Result<Value, AgentError> {
        let id = format!("hq-{}", self.io.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = mpsc::sync_channel(1);
        self.io
            .pending
            .lock()
            .map_err(|_| AgentError::Runtime("Codex response lock was poisoned".into()))?
            .insert(id.clone(), tx);
        self.io
            .send(&json!({ "id": id, "method": method, "params": params }))?;

        let response = rx.recv_timeout(REQUEST_TIMEOUT).map_err(|_| {
            if let Ok(mut pending) = self.io.pending.lock() {
                pending.remove(&id);
            }
            AgentError::Runtime(format!("Codex timed out while handling {method}"))
        })?;
        if let Some(error) = response.get("error") {
            return Err(AgentError::Runtime(format!(
                "Codex {method} failed: {}",
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
            )));
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| AgentError::InvalidResponse(format!("{method} omitted result")))
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), AgentError> {
        self.io.send(&json!({ "method": method, "params": params }))
    }
}

impl Drop for CodexClient {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl CodexIo {
    fn send(&self, message: &Value) -> Result<(), AgentError> {
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| AgentError::Runtime("Codex input lock was poisoned".into()))?;
        serde_json::to_writer(&mut *stdin, message).map_err(|error| {
            AgentError::Runtime(format!("Could not encode Codex request: {error}"))
        })?;
        stdin.write_all(b"\n").map_err(|source| AgentError::Io {
            operation: "write Codex request",
            source,
        })?;
        stdin.flush().map_err(|source| AgentError::Io {
            operation: "flush Codex request",
            source,
        })
    }
}

fn spawn_codex_reader(stdout: std::process::ChildStdout, io: Arc<CodexIo>) {
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(id) = message.get("id").and_then(Value::as_str) {
                if let Ok(mut pending) = io.pending.lock() {
                    if let Some(sender) = pending.remove(id) {
                        let _ = sender.send(message);
                        continue;
                    }
                }
            }
            if message.get("method").is_some() {
                let _ = io.events.send(message);
            }
        }
        let _ = io.events.send(json!({
            "method": "hq/runtimeExited",
            "params": { "message": "The Codex runtime exited" }
        }));
    });
}

fn create_workspace(hq_dir: &Path) -> Result<TempDir, AgentError> {
    let workspace = Builder::new()
        .prefix("hq-agent-")
        .tempdir()
        .map_err(|source| AgentError::Io {
            operation: "create agent workspace",
            source,
        })?;
    copy_workspace(hq_dir, workspace.path(), false)?;

    let status = Command::new("/usr/bin/git")
        .args(["init", "-q"])
        .current_dir(workspace.path())
        .status()
        .map_err(|source| AgentError::Io {
            operation: "initialize agent workspace",
            source,
        })?;
    if !status.success() {
        return Err(AgentError::Runtime(format!(
            "Could not initialize the agent workspace (git exited with {status})"
        )));
    }

    commit_workspace_baseline(workspace.path(), "HQ agent snapshot")?;
    Ok(workspace)
}

fn codex_command() -> Command {
    if let Ok(configured) = env::var("HQ_CODEX_BIN") {
        if !configured.trim().is_empty() {
            return Command::new(configured);
        }
    }
    for candidate in ["/opt/homebrew/bin/codex", "/usr/local/bin/codex"] {
        if Path::new(candidate).is_file() {
            return Command::new(candidate);
        }
    }
    Command::new("codex")
}

fn augmented_path() -> String {
    let existing = env::var("PATH").unwrap_or_default();
    format!("/opt/homebrew/bin:/usr/local/bin:{existing}")
}

fn codex_version() -> Result<String, AgentError> {
    let output = codex_command()
        .arg("--version")
        .env("PATH", augmented_path())
        .output()
        .map_err(|source| {
            AgentError::RuntimeUnavailable(format!(
                "Codex is unavailable. Install the Codex CLI or set HQ_CODEX_BIN: {source}"
            ))
        })?;
    if !output.status.success() {
        return Err(AgentError::RuntimeUnavailable(
            "Codex is installed but could not start".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn event_thread_id(event: &Value) -> Option<&str> {
    event.pointer("/params/threadId").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use serde_json::json;
    use tempfile::{tempdir, TempDir};

    use super::{
        collect_project_files, create_workspace, event_thread_id, AgentError, AgentFileChangeKind,
        AgentManager, AgentSession, WorkspaceScope,
    };

    #[test]
    fn extracts_thread_id_from_streamed_notifications() {
        let event = json!({
            "method": "item/completed",
            "params": { "threadId": "thread-1", "turnId": "turn-1" }
        });
        assert_eq!(event_thread_id(&event), Some("thread-1"));
        assert_eq!(event_thread_id(&json!({"method": "warning"})), None);
    }

    #[test]
    fn manager_starts_without_launching_the_runtime() {
        let manager = AgentManager::new();
        let receiver = manager.subscribe();
        assert_eq!(receiver.len(), 0);
    }

    #[test]
    fn workspace_starts_as_a_clean_git_snapshot() {
        let live = tempdir().unwrap();
        fs::create_dir_all(live.path().join("research")).unwrap();
        fs::write(
            live.path().join("research/test.md"),
            "---\ntitle: Test\nstatus: active\n---\n",
        )
        .unwrap();

        let workspace = create_workspace(live.path()).unwrap();
        let status = Command::new("/usr/bin/git")
            .args(["status", "--short"])
            .current_dir(workspace.path())
            .output()
            .unwrap();

        assert!(status.status.success());
        assert!(status.stdout.is_empty());
    }

    #[test]
    fn project_collection_ignores_non_project_markdown() {
        let live = tempdir().unwrap();
        fs::create_dir_all(live.path().join("research")).unwrap();
        fs::write(
            live.path().join("research/project.md"),
            "---\ntitle: Project\nstatus: active\n---\n",
        )
        .unwrap();
        fs::write(
            live.path().join("research/reference.md"),
            "# Reference document\n\nNot an HQ project.\n",
        )
        .unwrap();

        let scope = WorkspaceScope::from_dir(live.path());
        let files = collect_project_files(live.path(), &scope).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files.contains_key("research/project.md"));
        assert!(!files.contains_key("research/reference.md"));
    }

    fn live_hq() -> TempDir {
        let live = tempdir().unwrap();
        fs::create_dir_all(live.path().join("research")).unwrap();
        fs::create_dir_all(live.path().join("personal")).unwrap();
        fs::write(
            live.path().join("research/test.md"),
            "---\ntitle: Test\nstatus: active\n---\n\nOriginal research.\n",
        )
        .unwrap();
        fs::write(
            live.path().join("personal/task.md"),
            "---\ntitle: Task\nstatus: active\n---\n\nOriginal personal.\n",
        )
        .unwrap();
        live
    }

    fn manager_with_workspace(live: &TempDir) -> AgentManager {
        let scope = WorkspaceScope::from_dir(live.path());
        let base_files = collect_project_files(live.path(), &scope).unwrap();
        let workspace = create_workspace(live.path()).unwrap();
        let workspace_path = workspace.path().to_path_buf();
        let manager = AgentManager::new();
        manager.inner.session.lock().unwrap().replace(AgentSession {
            workspace_path,
            _workspace: workspace,
            thread_id: "thread".into(),
            provider: "openai".into(),
            model: "test".into(),
            scope,
            base_files,
        });
        manager
    }

    fn workspace_path(manager: &AgentManager) -> std::path::PathBuf {
        manager
            .inner
            .session
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .workspace_path
            .clone()
    }

    #[test]
    fn repository_change_can_be_rejected_without_touching_live_files() {
        let live = live_hq();
        let manager = manager_with_workspace(&live);
        let workspace = workspace_path(&manager);
        fs::write(
            workspace.join("research/test.md"),
            "---\ntitle: Test\nstatus: active\n---\n\nAgent edit.\n",
        )
        .unwrap();

        let change = manager.change().unwrap();
        assert!(change.changed);
        assert!(change.diff.contains("-Original research."));
        assert!(change.diff.contains("+Agent edit."));
        assert_eq!(change.files.len(), 1);

        manager.reject_changes().unwrap();
        assert!(!manager.change().unwrap().changed);
        assert!(fs::read_to_string(live.path().join("research/test.md"))
            .unwrap()
            .contains("Original research."));
    }

    #[test]
    fn approved_change_can_update_multiple_projects() {
        let live = live_hq();
        let manager = manager_with_workspace(&live);
        let workspace = workspace_path(&manager);
        fs::write(
            workspace.join("research/test.md"),
            "---\ntitle: Test\nstatus: waiting\n---\n\nAgent research edit.\n",
        )
        .unwrap();
        fs::write(
            workspace.join("personal/task.md"),
            "---\ntitle: Task\nstatus: done\n---\n\nAgent personal edit.\n",
        )
        .unwrap();

        let change = manager.change().unwrap();
        assert_eq!(change.files.len(), 2);
        let result = manager.apply_changes(live.path()).unwrap();

        assert_eq!(result.files.len(), 2);
        assert!(result.created.is_empty());
        assert!(fs::read_to_string(live.path().join("research/test.md"))
            .unwrap()
            .contains("status: waiting"));
        assert!(fs::read_to_string(live.path().join("personal/task.md"))
            .unwrap()
            .contains("status: done"));
        assert!(!manager.change().unwrap().changed);
    }

    #[test]
    fn approved_change_can_create_a_project_in_an_existing_track() {
        let live = live_hq();
        let manager = manager_with_workspace(&live);
        let workspace = workspace_path(&manager);
        fs::write(
            workspace.join("personal/new.md"),
            "---\ntitle: New\nstatus: active\n---\n\nCreated by agent.\n",
        )
        .unwrap();

        let change = manager.change().unwrap();
        assert_eq!(change.files.len(), 1);
        assert_eq!(change.files[0].kind, AgentFileChangeKind::Created);
        let result = manager.apply_changes(live.path()).unwrap();

        assert_eq!(result.created, vec!["personal/new.md"]);
        assert!(live.path().join("personal/new.md").is_file());
    }

    #[test]
    fn approval_refuses_to_overwrite_a_newer_live_edit() {
        let live = live_hq();
        let manager = manager_with_workspace(&live);
        let workspace = workspace_path(&manager);
        fs::write(
            workspace.join("research/test.md"),
            "---\ntitle: Test\nstatus: waiting\n---\n\nAgent edit.\n",
        )
        .unwrap();
        fs::write(
            live.path().join("research/test.md"),
            "---\ntitle: Test\nstatus: done\n---\n\nNewer live edit.\n",
        )
        .unwrap();

        let error = manager.apply_changes(live.path()).unwrap_err();

        assert!(matches!(
            error,
            AgentError::Project(crate::project_file::ProjectFileError::RevisionConflict { .. })
        ));
        assert!(fs::read_to_string(live.path().join("research/test.md"))
            .unwrap()
            .contains("Newer live edit."));
    }

    #[test]
    fn non_project_file_changes_are_not_approvable() {
        let live = live_hq();
        let manager = manager_with_workspace(&live);
        let workspace = workspace_path(&manager);
        fs::write(workspace.join("notes.txt"), "not a project").unwrap();

        let error = manager.change().unwrap_err();

        assert!(matches!(error, AgentError::UnsupportedChange(_)));
        manager.reject_changes().unwrap();
        assert!(!workspace.join("notes.txt").exists());
    }
}
