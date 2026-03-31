use std::convert::Infallible;
use std::path::{Component, Path, PathBuf};
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
use crate::mover::{move_project, reorder_projects, split_frontmatter, MoveOptions};

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
    let hq_dir_abs = state.hq_dir.canonicalize().unwrap_or_else(|_| state.hq_dir.clone());
    Json(serde_json::json!({ "projects": projects, "statuses": config.statuses, "hq_dir": hq_dir_abs }))
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
        Err(e) => {
            let status = if e.contains("No such file") || e.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            Err((status, Json(serde_json::json!({ "error": e }))))
        }
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
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )),
    }
}

#[derive(serde::Deserialize)]
struct SaveRequest {
    file: String,
    body: String,
}

fn resolve_web_project_path(hq_dir: &Path, file: &str) -> Result<PathBuf, &'static str> {
    let path = Path::new(file);
    if !file.ends_with(".md")
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Invalid file path");
    }

    Ok(hq_dir.join(path))
}

async fn post_save(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let filepath = resolve_web_project_path(&state.hq_dir, &req.file).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
    })?;
    let text = std::fs::read_to_string(&filepath).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("{}: {e}", req.file) })),
        )
    })?;

    let (fm_text, _old_body) = split_frontmatter(&text).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("{e} in {}", req.file) })),
        )
    })?;

    let new_body = req.body.trim_end();
    let result = if new_body.is_empty() {
        format!("---{fm_text}---\n")
    } else {
        format!("---{fm_text}---\n\n{new_body}\n")
    };

    std::fs::write(&filepath, result).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Write failed: {e}") })),
        )
    })?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(serde::Deserialize)]
struct ProjectQuery {
    file: String,
}

fn project_body(text: &str) -> &str {
    if text.starts_with("---") {
        split_frontmatter(text)
            .map(|(_, body)| body)
            .unwrap_or(text)
            .trim()
    } else {
        text.trim()
    }
}

async fn get_project(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let filepath = resolve_web_project_path(&state.hq_dir, &q.file).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
    })?;
    let text = std::fs::read_to_string(&filepath).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("{}: {e}", q.file) })),
        )
    })?;

    Ok(Json(serde_json::json!({ "file": q.file, "body": project_body(&text) })))
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
                let is_md = event.paths.iter().any(|p| {
                    p.extension().is_some_and(|ext| ext == "md")
                });
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
    use std::path::Path;

    use super::{project_body, resolve_web_project_path};

    #[test]
    fn project_body_ignores_dashes_inside_frontmatter_values() {
        let text = r#"---
title: "Bug repro"
status: active
priority: 40---
notes: keep this in frontmatter
---

Actual body text.
"#;

        assert_eq!(project_body(text), "Actual body text.");
    }

    #[test]
    fn resolve_web_project_path_rejects_absolute_paths() {
        let hq_dir = Path::new("/tmp/hq");
        assert!(resolve_web_project_path(hq_dir, "/tmp/outside.md").is_err());
    }

    #[test]
    fn resolve_web_project_path_rejects_parent_traversal() {
        let hq_dir = Path::new("/tmp/hq");
        assert!(resolve_web_project_path(hq_dir, "../outside.md").is_err());
    }

    #[test]
    fn resolve_web_project_path_accepts_relative_markdown_paths() {
        let hq_dir = Path::new("/tmp/hq");
        let resolved = resolve_web_project_path(hq_dir, "research/project.md").unwrap();
        assert_eq!(resolved, hq_dir.join("research/project.md"));
    }
}
