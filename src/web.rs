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

use crate::commands::{run_new, NewOptions, NewProjectError};
use crate::config::Config;
use crate::load_all;
use crate::mover::{move_project, reorder_projects, MoveOptions};
use crate::project::Project;
use crate::project_file::{
    create_track, read_project_body, toggle_body_checkbox, write_project_body, ProjectFileError,
};
use crate::timeline::{build_timeline, TimelineResponse};

const INDEX_HTML: &str = include_str!("../static/index.html");

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

#[derive(Clone)]
struct AppState {
    hq_dir: PathBuf,
    tx: broadcast::Sender<()>,
}

#[derive(serde::Serialize)]
struct ProjectsResponse {
    projects: Vec<Project>,
    statuses: Vec<String>,
    tracks: Vec<String>,
    hq_dir: PathBuf,
    default_owner: Option<String>,
    owners: Vec<String>,
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
    let opts = MoveOptions {
        file: req.file,
        to_status: req.to_status,
        priority: req.priority,
    };
    move_project(&state.hq_dir, &opts).map_err(project_file_error_response)?;

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
    reorder_projects(&state.hq_dir, &req.files).map_err(project_file_error_response)?;

    Ok(ok_response())
}

#[derive(serde::Deserialize)]
struct SaveRequest {
    file: String,
    body: String,
}

fn project_file_status(error: &ProjectFileError) -> StatusCode {
    match error {
        ProjectFileError::InvalidPath(_)
        | ProjectFileError::InvalidStatus { .. }
        | ProjectFileError::InvalidName { .. }
        | ProjectFileError::Frontmatter { .. }
        | ProjectFileError::MissingField { .. } => StatusCode::BAD_REQUEST,
        ProjectFileError::Read { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND
        }
        ProjectFileError::Read { .. } | ProjectFileError::Write { .. } => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
        ProjectFileError::AlreadyExists { .. } | ProjectFileError::CheckboxConflict => {
            StatusCode::CONFLICT
        }
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
    toggle_body_checkbox(
        &state.hq_dir,
        &req.file,
        req.line,
        req.expected,
        req.checked,
    )
    .map_err(project_file_error_response)?;
    Ok(ok_response())
}

async fn post_save(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveRequest>,
) -> ApiResult<OkResponse> {
    write_project_body(&state.hq_dir, &req.file, &req.body).map_err(project_file_error_response)?;

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
        .route("/api/reorder", post(post_reorder))
        .route("/api/save", post(post_save))
        .route("/api/checkbox", post(post_checkbox))
        .route("/api/tracks", post(post_new_track))
        .route("/api/events", get(get_events))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(hq_dir: PathBuf, port: u16) {
    let (tx, _) = broadcast::channel::<()>(16);
    spawn_markdown_watcher(hq_dir.clone(), tx.clone());

    let state = Arc::new(AppState { hq_dir, tx });
    let app = build_app(state);

    let addr = format!("127.0.0.1:{port}");
    println!("HQ server listening on http://localhost:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;

    use axum::http::StatusCode;
    use axum::Json;
    use notify::{Event, EventKind};
    use serde_json::json;

    use crate::commands::NewProjectError;
    use crate::project_file::ProjectFileError;

    use super::{
        event_touches_reload_target, new_project_error_response, project_file_error_response,
        project_file_status,
    };

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
