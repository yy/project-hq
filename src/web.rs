use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;

use crate::agent::{
    event_thread_id, AgentApplyResult, AgentChange, AgentError, AgentFileChangeKind, AgentManager,
    AgentRuntimeStatus, AgentSessionInfo, AgentTurnInfo,
};
use crate::commands::{run_new, NewOptions, NewProjectError};
use crate::config::Config;
use crate::load_all;
use crate::mover::{
    defer_project, move_project, reorder_projects, update_project_metadata, MetadataOptions,
    MoveOptions,
};
use crate::project::Project;
use crate::project_file::{
    create_track, read_project_body, toggle_body_checkbox, write_project_body, ProjectFileError,
};
use crate::timeline::{build_timeline, TimelineResponse};
use crate::undo::{UndoError, UndoManager, UndoResult, UndoStatus};

const INDEX_HTML: &str = include_str!("../static/index.html");

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

#[derive(Clone)]
struct AppState {
    hq_dir: PathBuf,
    tx: broadcast::Sender<()>,
    agent: AgentManager,
    undo: UndoManager,
}

#[derive(serde::Serialize)]
struct ProjectsResponse {
    projects: Vec<Project>,
    statuses: Vec<String>,
    tracks: Vec<String>,
    hq_dir: PathBuf,
    default_owner: Option<String>,
    owners: Vec<String>,
    pulse_tracks: Vec<String>,
}

/// Unique owner prefixes (filename segment before the first hyphen) across all
/// projects, plus the configured default owner. Powers owner autocomplete.
fn collect_owners(projects: &[Project], default_owner: Option<&str>) -> Vec<String> {
    let mut owners: Vec<String> = projects
        .iter()
        .filter_map(|p| owner_from_file(&p.file))
        .map(str::to_string)
        .collect();
    if let Some(default) = default_owner {
        owners.push(default.to_string());
    }
    owners.sort();
    owners.dedup();
    owners
}

fn owner_from_file(file: &str) -> Option<&str> {
    let name = file.rsplit(['/', '\\']).next()?;
    let stem = name.strip_suffix(".md").unwrap_or(name);
    stem.split_once('-')
        .map(|(owner, _)| owner)
        .filter(|owner| !owner.is_empty())
}

#[derive(serde::Serialize)]
struct ProjectResponse {
    file: String,
    body: String,
}

#[derive(serde::Serialize)]
struct OkResponse {
    ok: bool,
}

#[derive(serde::Serialize)]
struct ErrorResponse {
    error: String,
}

type ApiError = (StatusCode, Json<ErrorResponse>);
type ApiResult<T> = Result<Json<T>, ApiError>;

fn ok_response() -> Json<OkResponse> {
    Json(OkResponse { ok: true })
}

async fn get_projects(State(state): State<Arc<AppState>>) -> Json<ProjectsResponse> {
    let config = Config::load(&state.hq_dir);
    let projects = load_all(&state.hq_dir, &config);
    let hq_dir_abs = state
        .hq_dir
        .canonicalize()
        .unwrap_or_else(|_| state.hq_dir.clone());
    let owners = collect_owners(&projects, config.default_owner.as_deref());
    Json(ProjectsResponse {
        projects,
        statuses: config.statuses,
        tracks: config.tracks,
        hq_dir: hq_dir_abs,
        default_owner: config.default_owner,
        owners,
        pulse_tracks: config.pulse_tracks,
    })
}

async fn get_timeline(State(state): State<Arc<AppState>>) -> Json<TimelineResponse> {
    let config = Config::load(&state.hq_dir);
    Json(build_timeline(&state.hq_dir, &config))
}

#[derive(serde::Deserialize)]
struct MoveRequest {
    file: String,
    to_status: String,
    priority: Option<f64>,
}

async fn post_move(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MoveRequest>,
) -> ApiResult<OkResponse> {
    let files = vec![req.file.clone()];
    let undo = state
        .undo
        .capture_files(&state.hq_dir, &files)
        .map_err(undo_error_response)?;
    let opts = MoveOptions {
        file: req.file,
        to_status: req.to_status,
        priority: req.priority,
    };
    move_project(&state.hq_dir, &opts).map_err(project_file_error_response)?;
    state
        .undo
        .record_files(&state.hq_dir, "Move project", undo)
        .map_err(undo_error_response)?;

    Ok(ok_response())
}

#[derive(serde::Deserialize)]
struct DeferRequest {
    file: String,
    until: String,
}

async fn post_defer(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeferRequest>,
) -> ApiResult<OkResponse> {
    let files = vec![req.file.clone()];
    let undo = state
        .undo
        .capture_files(&state.hq_dir, &files)
        .map_err(undo_error_response)?;
    defer_project(&state.hq_dir, &req.file, &req.until).map_err(project_file_error_response)?;
    state
        .undo
        .record_files(&state.hq_dir, "Defer project", undo)
        .map_err(undo_error_response)?;
    Ok(ok_response())
}

#[derive(serde::Deserialize)]
struct ReorderRequest {
    files: Vec<String>,
}

async fn post_reorder(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ReorderRequest>,
) -> ApiResult<OkResponse> {
    let undo = state
        .undo
        .capture_files(&state.hq_dir, &req.files)
        .map_err(undo_error_response)?;
    reorder_projects(&state.hq_dir, &req.files).map_err(project_file_error_response)?;
    state
        .undo
        .record_files(&state.hq_dir, "Reorder projects", undo)
        .map_err(undo_error_response)?;

    Ok(ok_response())
}

#[derive(serde::Deserialize)]
struct SaveRequest {
    file: String,
    body: String,
}

#[derive(serde::Deserialize)]
struct MetadataRequest {
    file: String,
    title: String,
    status: String,
    priority: f64,
    owner: String,
    my_next: String,
    waiting_on: String,
    waiting_since: String,
    deadline: String,
    deferred_until: String,
    action_mode: String,
}

fn project_file_status(error: &ProjectFileError) -> StatusCode {
    match error {
        ProjectFileError::InvalidPath(_)
        | ProjectFileError::InvalidStatus { .. }
        | ProjectFileError::InvalidDate { .. }
        | ProjectFileError::InvalidName { .. }
        | ProjectFileError::Frontmatter { .. }
        | ProjectFileError::MissingField { .. } => StatusCode::BAD_REQUEST,
        ProjectFileError::Read { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND
        }
        ProjectFileError::Read { .. } | ProjectFileError::Write { .. } => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
        ProjectFileError::AlreadyExists { .. }
        | ProjectFileError::CheckboxConflict
        | ProjectFileError::RevisionConflict { .. } => StatusCode::CONFLICT,
    }
}

fn project_file_error_response(error: ProjectFileError) -> (StatusCode, Json<ErrorResponse>) {
    let status = project_file_status(&error);
    (
        status,
        Json(ErrorResponse {
            error: error.to_string(),
        }),
    )
}

fn undo_error_response(error: UndoError) -> ApiError {
    let status = match &error {
        UndoError::NothingToUndo | UndoError::Conflict { .. } => StatusCode::CONFLICT,
        UndoError::StateUnavailable | UndoError::Remove { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        UndoError::Project(error) => project_file_status(error),
    };
    user_error(status, error.to_string())
}

#[derive(serde::Deserialize)]
struct CheckboxRequest {
    file: String,
    line: usize,
    expected: bool,
    checked: bool,
}

async fn post_checkbox(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckboxRequest>,
) -> ApiResult<OkResponse> {
    let files = vec![req.file.clone()];
    let undo = state
        .undo
        .capture_files(&state.hq_dir, &files)
        .map_err(undo_error_response)?;
    toggle_body_checkbox(
        &state.hq_dir,
        &req.file,
        req.line,
        req.expected,
        req.checked,
    )
    .map_err(project_file_error_response)?;
    let label = if req.checked {
        "Complete checklist item"
    } else {
        "Uncheck checklist item"
    };
    state
        .undo
        .record_files(&state.hq_dir, label, undo)
        .map_err(undo_error_response)?;
    Ok(ok_response())
}

async fn post_save(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveRequest>,
) -> ApiResult<OkResponse> {
    let files = vec![req.file.clone()];
    let undo = state
        .undo
        .capture_files(&state.hq_dir, &files)
        .map_err(undo_error_response)?;
    write_project_body(&state.hq_dir, &req.file, &req.body).map_err(project_file_error_response)?;
    state
        .undo
        .record_files(&state.hq_dir, "Edit project", undo)
        .map_err(undo_error_response)?;

    Ok(ok_response())
}

async fn post_metadata(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MetadataRequest>,
) -> ApiResult<OkResponse> {
    let files = vec![req.file.clone()];
    let undo = state
        .undo
        .capture_files(&state.hq_dir, &files)
        .map_err(undo_error_response)?;
    update_project_metadata(
        &state.hq_dir,
        &MetadataOptions {
            file: req.file,
            title: req.title,
            status: req.status,
            priority: req.priority,
            owner: req.owner,
            my_next: req.my_next,
            waiting_on: req.waiting_on,
            waiting_since: req.waiting_since,
            deadline: req.deadline,
            deferred_until: req.deferred_until,
            action_mode: req.action_mode,
        },
    )
    .map_err(project_file_error_response)?;
    state
        .undo
        .record_files(&state.hq_dir, "Edit project details", undo)
        .map_err(undo_error_response)?;
    let _ = state.tx.send(());
    Ok(ok_response())
}

#[derive(serde::Deserialize)]
struct NewProjectRequest {
    track: String,
    title: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    priority: Option<f64>,
    #[serde(default)]
    deadline: Option<String>,
    #[serde(default)]
    my_next: Option<String>,
    #[serde(default)]
    action_mode: Option<String>,
    #[serde(default)]
    create_track: bool,
}

#[derive(serde::Serialize)]
struct NewProjectResponse {
    file: String,
    project: Project,
}

fn user_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
}

fn new_project_error_response(error: NewProjectError) -> ApiError {
    let status = match &error {
        NewProjectError::Validation(_) => StatusCode::BAD_REQUEST,
        NewProjectError::UnknownTrack { .. } => StatusCode::NOT_FOUND,
        NewProjectError::ProjectFile(error) => project_file_status(error),
    };
    user_error(status, error.to_string())
}

async fn post_new_project(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NewProjectRequest>,
) -> ApiResult<NewProjectResponse> {
    let opts = NewOptions {
        track: req.track.clone(),
        title: req.title,
        owner: req.owner,
        slug: req.slug,
        status: req.status.unwrap_or_else(|| "active".to_string()),
        priority: req.priority,
        deadline: req.deadline,
        my_next: req.my_next,
        action_mode: req.action_mode,
        edit: false,
        new_track: req.create_track,
    };
    let path = run_new(&state.hq_dir, opts).map_err(new_project_error_response)?;

    let file = path
        .strip_prefix(&state.hq_dir)
        .unwrap_or(&path)
        .to_string_lossy()
        .to_string();

    let project = Project::from_file(&path, &req.track, &state.hq_dir).ok_or_else(|| {
        user_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read back created project at {file}"),
        )
    })?;
    state
        .undo
        .record_created(&state.hq_dir, &file, "Create project")
        .map_err(undo_error_response)?;

    let _ = state.tx.send(());

    Ok(Json(NewProjectResponse { file, project }))
}

#[derive(serde::Deserialize)]
struct NewTrackRequest {
    name: String,
}

#[derive(serde::Serialize)]
struct NewTrackResponse {
    name: String,
}

async fn post_new_track(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NewTrackRequest>,
) -> ApiResult<NewTrackResponse> {
    create_track(&state.hq_dir, &req.name).map_err(project_file_error_response)?;
    let _ = state.tx.send(());
    Ok(Json(NewTrackResponse { name: req.name }))
}

#[derive(serde::Deserialize)]
struct ProjectQuery {
    file: String,
}

async fn get_project(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ProjectQuery>,
) -> ApiResult<ProjectResponse> {
    let body = read_project_body(&state.hq_dir, &q.file).map_err(project_file_error_response)?;

    Ok(Json(ProjectResponse { file: q.file, body }))
}

async fn get_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).map(|_| Ok(Event::default().data("reload")));
    Sse::new(stream)
}

fn agent_error_response(error: AgentError) -> ApiError {
    let status = match &error {
        AgentError::RuntimeUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        AgentError::Runtime(_) | AgentError::InvalidResponse(_) | AgentError::Io { .. } => {
            StatusCode::BAD_GATEWAY
        }
        AgentError::UnknownSession => StatusCode::NOT_FOUND,
        AgentError::NoChange(_) | AgentError::UnsupportedChange(_) => StatusCode::CONFLICT,
        AgentError::Project(error) => project_file_status(error),
    };
    (
        status,
        Json(ErrorResponse {
            error: error.to_string(),
        }),
    )
}

async fn get_agent_status(State(state): State<Arc<AppState>>) -> Json<AgentRuntimeStatus> {
    Json(state.agent.runtime_status())
}

async fn post_agent_session(State(state): State<Arc<AppState>>) -> ApiResult<AgentSessionInfo> {
    let agent = state.agent.clone();
    let hq_dir = state.hq_dir.clone();
    let info = tokio::task::spawn_blocking(move || agent.start_session(&hq_dir))
        .await
        .map_err(|error| {
            agent_error_response(AgentError::Runtime(format!(
                "Could not prepare agent session: {error}"
            )))
        })?
        .map_err(agent_error_response)?;
    Ok(Json(info))
}

#[derive(serde::Deserialize)]
struct AgentTurnRequest {
    message: String,
    #[serde(default)]
    context_file: Option<String>,
}

async fn post_agent_turn(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentTurnRequest>,
) -> ApiResult<AgentTurnInfo> {
    let agent = state.agent.clone();
    let hq_dir = state.hq_dir.clone();
    let info = tokio::task::spawn_blocking(move || {
        agent.start_turn(&hq_dir, &req.message, req.context_file.as_deref())
    })
    .await
    .map_err(|error| {
        agent_error_response(AgentError::Runtime(format!(
            "Could not start agent turn: {error}"
        )))
    })?
    .map_err(agent_error_response)?;
    Ok(Json(info))
}

#[derive(serde::Deserialize)]
struct AgentInterruptRequest {
    thread_id: String,
    turn_id: String,
}

async fn post_agent_interrupt(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentInterruptRequest>,
) -> ApiResult<OkResponse> {
    let agent = state.agent.clone();
    tokio::task::spawn_blocking(move || agent.interrupt(&req.thread_id, &req.turn_id))
        .await
        .map_err(|error| {
            agent_error_response(AgentError::Runtime(format!(
                "Could not interrupt agent turn: {error}"
            )))
        })?
        .map_err(agent_error_response)?;
    Ok(ok_response())
}

#[derive(serde::Deserialize)]
struct AgentEventsQuery {
    thread_id: String,
}

async fn get_agent_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AgentEventsQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.agent.subscribe();
    let thread_id = query.thread_id;
    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let event = result.ok()?;
        let applies = event_thread_id(&event).is_some_and(|id| id == thread_id)
            || event.get("method").and_then(serde_json::Value::as_str) == Some("hq/runtimeExited");
        applies.then(|| {
            Ok(Event::default()
                .event("agent")
                .data(serde_json::to_string(&event).unwrap_or_default()))
        })
    });
    Sse::new(stream)
}

async fn get_agent_change(State(state): State<Arc<AppState>>) -> ApiResult<AgentChange> {
    state.agent.change().map(Json).map_err(agent_error_response)
}

async fn post_agent_apply(State(state): State<Arc<AppState>>) -> ApiResult<AgentApplyResult> {
    let change = state.agent.change().map_err(agent_error_response)?;
    let existing: Vec<String> = change
        .files
        .iter()
        .filter(|change| change.kind == AgentFileChangeKind::Modified)
        .map(|change| change.file.clone())
        .collect();
    let undo = state
        .undo
        .capture_files(&state.hq_dir, &existing)
        .map_err(undo_error_response)?;
    let result = state
        .agent
        .apply_changes(&state.hq_dir)
        .map_err(agent_error_response)?;
    state
        .undo
        .record_files_and_created(&state.hq_dir, "Apply agent update", undo, &result.created)
        .map_err(undo_error_response)?;
    let _ = state.tx.send(());
    Ok(Json(result))
}

async fn post_agent_reject(State(state): State<Arc<AppState>>) -> ApiResult<OkResponse> {
    state.agent.reject_changes().map_err(agent_error_response)?;
    Ok(ok_response())
}

async fn get_undo_status(State(state): State<Arc<AppState>>) -> Json<UndoStatus> {
    Json(state.undo.status())
}

async fn post_undo(State(state): State<Arc<AppState>>) -> ApiResult<UndoResult> {
    let result = state
        .undo
        .undo(&state.hq_dir)
        .map_err(undo_error_response)?;
    if !result.files.is_empty() {
        state.agent.invalidate_session();
    }
    let _ = state.tx.send(());
    Ok(Json(result))
}

fn event_touches_reload_target(event: &notify::Event) -> bool {
    event.paths.iter().any(|path| {
        path.extension().is_some_and(|ext| ext == "md")
            || path.file_name().is_some_and(|name| name == "hq.toml")
    })
}

fn spawn_markdown_watcher(hq_dir: PathBuf, tx: broadcast::Sender<()>) {
    tokio::task::spawn_blocking(move || {
        use notify::{recommended_watcher, RecursiveMode, Watcher};

        let mut watcher = recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if event_touches_reload_target(&event) {
                    let _ = tx.send(());
                }
            }
        })
        .expect("failed to create file watcher");

        watcher
            .watch(&hq_dir, RecursiveMode::Recursive)
            .expect("failed to watch directory");

        // Park the thread to keep the watcher alive.
        std::thread::park();
    });
}

fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/projects", get(get_projects).post(post_new_project))
        .route("/api/project", get(get_project))
        .route("/api/timeline", get(get_timeline))
        .route("/api/move", post(post_move))
        .route("/api/defer", post(post_defer))
        .route("/api/reorder", post(post_reorder))
        .route("/api/save", post(post_save))
        .route("/api/metadata", post(post_metadata))
        .route("/api/checkbox", post(post_checkbox))
        .route("/api/tracks", post(post_new_track))
        .route("/api/events", get(get_events))
        .route("/api/agent/status", get(get_agent_status))
        .route("/api/agent/session", post(post_agent_session))
        .route("/api/agent/turn", post(post_agent_turn))
        .route("/api/agent/interrupt", post(post_agent_interrupt))
        .route("/api/agent/events", get(get_agent_events))
        .route("/api/agent/change", get(get_agent_change))
        .route("/api/agent/apply", post(post_agent_apply))
        .route("/api/agent/reject", post(post_agent_reject))
        .route("/api/undo", get(get_undo_status).post(post_undo))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(hq_dir: PathBuf, port: u16) {
    let (tx, _) = broadcast::channel::<()>(16);
    spawn_markdown_watcher(hq_dir.clone(), tx.clone());

    let state = Arc::new(AppState {
        hq_dir,
        tx,
        agent: AgentManager::new(),
        undo: UndoManager::new(),
    });
    let app = build_app(state);

    let addr = format!("127.0.0.1:{port}");
    println!("HQ server listening on http://localhost:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::action::ActionMode;
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::Json;
    use notify::{Event, EventKind};
    use serde_json::json;
    use tempfile::tempdir;
    use tokio::sync::broadcast;

    use crate::commands::NewProjectError;
    use crate::project_file::ProjectFileError;
    use crate::undo::UndoManager;

    use super::{
        event_touches_reload_target, new_project_error_response, post_defer, post_metadata,
        post_new_project, post_undo, project_file_error_response, project_file_status,
        AgentManager, AppState, DeferRequest, MetadataRequest, NewProjectRequest,
    };

    #[tokio::test]
    async fn new_project_api_preserves_action_mode() {
        let temp = tempdir().unwrap();
        let (tx, _) = broadcast::channel(1);
        let state = Arc::new(AppState {
            hq_dir: temp.path().to_path_buf(),
            tx,
            agent: AgentManager::new(),
            undo: UndoManager::new(),
        });
        let request = NewProjectRequest {
            track: "personal".to_string(),
            title: "Household".to_string(),
            owner: None,
            slug: None,
            status: None,
            priority: None,
            deadline: None,
            my_next: None,
            action_mode: Some("serial".to_string()),
            create_track: true,
        };

        let Json(response) = match post_new_project(State(state), Json(request)).await {
            Ok(response) => response,
            Err((status, _)) => panic!("project creation failed with {status}"),
        };

        assert_eq!(response.project.action_mode, ActionMode::Serial);
        assert!(response.project.visible);
        assert_eq!(response.project.file, "personal/yy-household.md");
    }

    #[tokio::test]
    async fn defer_api_writes_requested_instant() {
        let temp = tempdir().unwrap();
        let track = temp.path().join("personal");
        fs::create_dir(&track).unwrap();
        fs::write(
            track.join("call.md"),
            "---\ntitle: Call\nstatus: active\n---\n",
        )
        .unwrap();
        let (tx, _) = broadcast::channel(1);
        let state = Arc::new(AppState {
            hq_dir: temp.path().to_path_buf(),
            tx,
            agent: AgentManager::new(),
            undo: UndoManager::new(),
        });

        let result = post_defer(
            State(state.clone()),
            Json(DeferRequest {
                file: "personal/call.md".to_string(),
                until: "2026-07-26T17:00:00.000Z".to_string(),
            }),
        )
        .await;
        assert!(result.is_ok());

        let text = fs::read_to_string(track.join("call.md")).unwrap();
        assert!(text.contains("status: active"));
        assert!(text.contains("deferred_until: 2026-07-26T17:00:00.000Z"));

        let Json(undo) = match post_undo(State(state)).await {
            Ok(response) => response,
            Err((status, _)) => panic!("undo failed with {status}"),
        };
        assert_eq!(undo.label, "Defer project");
        let restored = fs::read_to_string(track.join("call.md")).unwrap();
        assert!(restored.contains("status: active"));
        assert!(!restored.contains("deferred_until"));
    }

    #[tokio::test]
    async fn metadata_api_updates_and_clears_project_fields() {
        let temp = tempdir().unwrap();
        let track = temp.path().join("personal");
        fs::create_dir(&track).unwrap();
        fs::write(
            track.join("task.md"),
            "---\ntitle: Old title\nstatus: active\npriority: 50\nowner: yy\nmy_next: Old next\nwaiting_on: Someone\n---\n\nNotes.\n",
        )
        .unwrap();
        let (tx, _) = broadcast::channel(1);
        let state = Arc::new(AppState {
            hq_dir: temp.path().to_path_buf(),
            tx,
            agent: AgentManager::new(),
            undo: UndoManager::new(),
        });

        let result = post_metadata(
            State(state.clone()),
            Json(MetadataRequest {
                file: "personal/task.md".to_string(),
                title: "New: title".to_string(),
                status: "waiting".to_string(),
                priority: 72.5,
                owner: "yy".to_string(),
                my_next: "Call electrician".to_string(),
                waiting_on: String::new(),
                waiting_since: "2026-07-27".to_string(),
                deadline: "2026-08-01".to_string(),
                deferred_until: String::new(),
                action_mode: "serial".to_string(),
            }),
        )
        .await;

        assert!(result.is_ok());
        let text = fs::read_to_string(track.join("task.md")).unwrap();
        assert!(text.contains("title: \"New: title\""));
        assert!(text.contains("status: waiting"));
        assert!(text.contains("priority: 72.5"));
        assert!(text.contains("my_next: Call electrician"));
        assert!(!text.contains("waiting_on:"));
        assert!(text.contains("waiting_since: 2026-07-27"));
        assert!(text.contains("deadline: 2026-08-01"));
        assert!(text.contains("action_mode: serial"));

        let Json(undo) = match post_undo(State(state)).await {
            Ok(response) => response,
            Err((status, _)) => panic!("undo failed with {status}"),
        };
        assert_eq!(undo.label, "Edit project details");
        let restored = fs::read_to_string(track.join("task.md")).unwrap();
        assert!(restored.contains("title: Old title"));
        assert!(restored.contains("waiting_on: Someone"));
    }

    #[test]
    fn bad_request_errors_map_to_400() {
        assert_eq!(
            project_file_status(&ProjectFileError::InvalidPath("bad".to_string())),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn missing_files_map_to_404() {
        let error = ProjectFileError::Read {
            file: "missing.md".to_string(),
            source: io::Error::new(io::ErrorKind::NotFound, "missing"),
        };
        assert_eq!(project_file_status(&error), StatusCode::NOT_FOUND);
    }

    #[test]
    fn checkbox_conflicts_map_to_409() {
        assert_eq!(
            project_file_status(&ProjectFileError::CheckboxConflict),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn error_responses_keep_the_existing_json_shape() {
        let error = ProjectFileError::InvalidPath("bad.md".to_string());
        let (status, Json(body)) = project_file_error_response(error);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            serde_json::to_value(body).unwrap(),
            json!({ "error": "Invalid file path: bad.md" })
        );
    }

    #[test]
    fn new_project_errors_have_explicit_statuses() {
        let (status, Json(body)) = new_project_error_response(NewProjectError::Validation(
            "--title cannot be empty".into(),
        ));
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            serde_json::to_value(body).unwrap(),
            json!({ "error": "--title cannot be empty" })
        );

        let (status, Json(body)) = new_project_error_response(NewProjectError::UnknownTrack {
            track: "ideas".to_string(),
            known: "research".to_string(),
        });
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            serde_json::to_value(body).unwrap(),
            json!({ "error": "Unknown track \"ideas\". Existing tracks: research. Pass --new-track to create it." })
        );
    }

    #[test]
    fn markdown_events_trigger_reload() {
        let markdown_event = Event {
            kind: EventKind::Any,
            paths: vec![PathBuf::from("research/project.md")],
            attrs: Default::default(),
        };
        let config_event = Event {
            kind: EventKind::Any,
            paths: vec![PathBuf::from("hq.toml")],
            attrs: Default::default(),
        };
        let non_markdown_event = Event {
            kind: EventKind::Any,
            paths: vec![PathBuf::from("research/project.txt")],
            attrs: Default::default(),
        };

        assert!(event_touches_reload_target(&markdown_event));
        assert!(event_touches_reload_target(&config_event));
        assert!(!event_touches_reload_target(&non_markdown_event));
    }
}
