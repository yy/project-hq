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
use crate::project_file::{read_project_body, write_project_body, ProjectFileError};

const INDEX_HTML: &str = include_str!("../static/index.html");

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

#[derive(Clone)]
struct AppState {
    hq_dir: PathBuf,
    tx: broadcast::Sender<()>,
}

async fn get_projects(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let config = Config::load(&state.hq_dir);
    let projects = load_all(&state.hq_dir, &config);
    let hq_dir_abs = state
        .hq_dir
        .canonicalize()
        .unwrap_or_else(|_| state.hq_dir.clone());
    Json(
        serde_json::json!({ "projects": projects, "statuses": config.statuses, "hq_dir": hq_dir_abs }),
    )
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
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let opts = MoveOptions {
        file: req.file,
        to_status: req.to_status,
        priority: req.priority,
    };
    match move_project(&state.hq_dir, &opts) {
        Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
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
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match reorder_projects(&state.hq_dir, &req.files) {
        Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
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
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

fn project_file_error_response(error: ProjectFileError) -> (StatusCode, Json<serde_json::Value>) {
    let status = project_file_status(&error);
    (
        status,
        Json(serde_json::json!({ "error": error.to_string() })),
    )
}

async fn post_save(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    write_project_body(&state.hq_dir, &req.file, &req.body).map_err(project_file_error_response)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(serde::Deserialize)]
struct ProjectQuery {
    file: String,
}

async fn get_project(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let body = read_project_body(&state.hq_dir, &q.file).map_err(project_file_error_response)?;

    Ok(Json(serde_json::json!({ "file": q.file, "body": body })))
}

async fn get_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).map(|_| Ok(Event::default().data("reload")));
    Sse::new(stream)
}

pub async fn serve(hq_dir: PathBuf, port: u16) {
    let (tx, _) = broadcast::channel::<()>(16);

    // Spawn file watcher in a background thread
    let watcher_tx = tx.clone();
    let watcher_dir = hq_dir.clone();
    tokio::task::spawn_blocking(move || {
        use notify::{recommended_watcher, RecursiveMode, Watcher};

        let tx = watcher_tx;
        let mut watcher = recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                // Only broadcast for .md file changes
                let is_md = event
                    .paths
                    .iter()
                    .any(|p| p.extension().is_some_and(|ext| ext == "md"));
                if is_md {
                    let _ = tx.send(());
                }
            }
        })
        .expect("failed to create file watcher");

        watcher
            .watch(&watcher_dir, RecursiveMode::Recursive)
            .expect("failed to watch directory");

        // Park the thread to keep the watcher alive
        std::thread::park();
    });

    let state = Arc::new(AppState { hq_dir, tx });

    let app = Router::new()
        .route("/", get(index))
        .route("/api/projects", get(get_projects))
        .route("/api/project", get(get_project))
        .route("/api/move", post(post_move))
        .route("/api/reorder", post(post_reorder))
        .route("/api/save", post(post_save))
        .route("/api/events", get(get_events))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    println!("HQ server listening on http://localhost:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use std::io;

    use axum::http::StatusCode;

    use crate::project_file::ProjectFileError;

    use super::project_file_status;

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
}
