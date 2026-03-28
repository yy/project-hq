use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
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
use crate::mover::{move_project, MoveOptions};

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
    Json(serde_json::json!({ "projects": projects }))
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
        .route("/api/move", post(post_move))
        .route("/api/events", get(get_events))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    println!("HQ server listening on http://localhost:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
