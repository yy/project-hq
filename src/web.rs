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

use crate::config::Config;
use crate::load_all;
use crate::mover::{move_project, reorder_projects, MoveOptions};
use crate::project::Project;
use crate::project_file::{
    read_project_body, toggle_body_checkbox, write_project_body, ProjectFileError,
};

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
    hq_dir: PathBuf,
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
    Json(ProjectsResponse {
        projects,
        statuses: config.statuses,
        hq_dir: hq_dir_abs,
    })
}

#[derive(serde::Deserialize)]
struct MoveRequest {
    file: String,
    to_status: String,
    priority: Option<i32>,
}

async fn post_move(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorResponse>)> {
    let opts = MoveOptions {
        file: req.file,
        to_status: req.to_status,
        priority: req.priority,
    };
    match move_project(&state.hq_dir, &opts) {
        Ok(()) => Ok(ok_response()),
        Err(e) => Err(project_file_error_response(e)),
    }
}

#[derive(serde::Deserialize)]
struct ReorderRequest {
    files: Vec<String>,
}

async fn post_reorder(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ReorderRequest>,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorResponse>)> {
    match reorder_projects(&state.hq_dir, &req.files) {
        Ok(()) => Ok(ok_response()),
        Err(e) => Err(project_file_error_response(e)),
    }
}

#[derive(serde::Deserialize)]
struct SaveRequest {
    file: String,
    body: String,
}

fn project_file_status(error: &ProjectFileError) -> StatusCode {
    if error.is_bad_request() {
        StatusCode::BAD_REQUEST
    } else if error.is_not_found() {
        StatusCode::NOT_FOUND
    } else if error.is_conflict() {
        StatusCode::CONFLICT
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
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
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorResponse>)> {
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
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorResponse>)> {
    write_project_body(&state.hq_dir, &req.file, &req.body).map_err(project_file_error_response)?;

    Ok(ok_response())
}

#[derive(serde::Deserialize)]
struct ProjectQuery {
    file: String,
}

async fn get_project(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<ProjectResponse>, (StatusCode, Json<ErrorResponse>)> {
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
        .route("/api/projects", get(get_projects))
        .route("/api/project", get(get_project))
        .route("/api/move", post(post_move))
        .route("/api/reorder", post(post_reorder))
        .route("/api/save", post(post_save))
        .route("/api/checkbox", post(post_checkbox))
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

    use crate::project_file::ProjectFileError;

    use super::{event_touches_reload_target, project_file_error_response, project_file_status};

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
