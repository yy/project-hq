use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_chat_server(
    data: &std::path::Path,
    config: &std::path::Path,
    runtime: &std::path::Path,
    log: &std::path::Path,
) -> (Server, u16) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hq"))
        .args(["--dir", data.to_str().unwrap(), "serve", "--port", "0"])
        .env("XDG_CONFIG_HOME", config)
        .env("HQ_CODEX_BIN", runtime)
        .env("HQ_TEST_RUNTIME_LOG", log)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).unwrap();
        tx.send(line).unwrap();
    });
    let line = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let ready: serde_json::Value =
        serde_json::from_str(line.strip_prefix("HQ_READY ").unwrap()).unwrap();
    (Server(child), ready["port"].as_u64().unwrap() as u16)
}

fn chat_request(
    port: u16,
    path: &str,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let method = if body.is_some() { "POST" } else { "GET" };
    let body = body.map(|body| body.to_string()).unwrap_or_default();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (
        status,
        serde_json::from_str(body).unwrap_or_else(|_| panic!("Invalid response: {response}")),
    )
}

#[test]
fn saved_chats_survive_server_restart_without_replaying_requests() {
    use serde_json::json;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("hq");
    let config = temp.path().join("settings");
    let log = temp.path().join("runtime.jsonl");
    let runtime = temp.path().join("fake-codex");
    fs::create_dir_all(data.join("personal")).unwrap();
    fs::write(
        data.join("personal/project.md"),
        "---\ntitle: Project\nstatus: active\n---\n\nNotes.\n\n- [ ] Review draft &alex\n",
    )
    .unwrap();
    fs::write(&runtime, r#"#!/usr/bin/env -S uv run --no-project python
import sys, json, os, pathlib, uuid
def send(value):
    print(json.dumps(value), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    method = request.get('method')
    params = request.get('params', {})
    with open(os.environ['HQ_TEST_RUNTIME_LOG'], 'a') as log:
        log.write(json.dumps(request) + '\n')
    if 'id' not in request:
        continue
    result = {}
    if method in ('thread/start', 'thread/resume'):
        workspace = pathlib.Path(params['cwd'])
        result = {'thread': {'id':'thread-test'}, 'model':'test-model', 'modelProvider':'test', 'reasoningEffort':'low'}
    elif method == 'turn/start':
        result = {'turn': {'id': 'turn-test'}}
    send({'id':request['id'], 'result':result})
    if method == 'turn/start':
        if 'stage edit' in params['input'][0]['text']:
            (workspace / 'personal/project.md').write_text('---\ntitle: Project\nstatus: active\n---\n\nAgent edit.\n')
        send({'method':'turn/started','params':{'threadId':'thread-test','turn':{'id':'turn-test'}}})
        send({'method':'item/agentMessage/delta','params':{'threadId':'thread-test','itemId':'reply-' + str(uuid.uuid4()),'delta':'Saved reply'}})
        if 'pause here' not in params['input'][0]['text']:
            send({'method':'turn/completed','params':{'threadId':'thread-test','turn':{'id':'turn-test','status':'completed'}}})
"#).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    let (server, port) = start_chat_server(&data, &config, &runtime, &log);
    let (status, session) = chat_request(
        port,
        "/api/agent/session",
        Some(json!({"model":"test-model","reasoning_effort":"low"})),
    );
    assert_eq!(status, 200, "{session}");
    let ui = json!({"draft":"Draft survives restart","context_file":"personal/project.md",
        "task_context":{"title":"Review draft","file":"personal/project.md","line":2,"expected":"- [ ] Review draft &alex"},
        "queue":[{"id":"request-1","message":"First request","context_file":"personal/project.md",
            "context_task":{"file":"personal/project.md","line":2,"expected":"- [ ] Review draft &alex"},"context_title":"Project: Review draft"},
                 {"id":"request-2","message":"Held request","context_file":null,"context_task":null,"context_title":null}]});
    assert_eq!(
        chat_request(
            port,
            "/api/agent/conversation?thread_id=thread-test",
            Some(json!({"revision":0,"selected":true,"ui":ui}))
        )
        .0,
        200
    );
    assert_eq!(chat_request(port, "/api/agent/turn", Some(json!({"thread_id":"thread-test","request_id":"request-1","message":"First request",
        "context_task":{"file":"personal/project.md","line":2,"expected":"- [ ] Review draft &alex"}}))).0, 200);
    // No browser/SSE connection is needed to save a reply.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let saved = loop {
        assert!(
            std::time::Instant::now() < deadline,
            "reply was not persisted"
        );
        let (_, chats) = chat_request(port, "/api/agent/conversations", None);
        if chats[0]["status"] == "review" {
            break chats;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(saved[0]["entries"][1]["text"], "Saved reply");
    assert_eq!(
        chat_request(
            port,
            "/api/agent/apply?thread_id=thread-test",
            Some(json!({}))
        )
        .0,
        200
    );
    assert_eq!(
        chat_request(
            port,
            "/api/agent/settings?thread_id=thread-test",
            Some(json!({"model":"chat-override","reasoning_effort":"high"}))
        )
        .0,
        200
    );
    drop(server);
    let (server, port) = start_chat_server(&data, &config, &runtime, &log);
    let (_, saved) = chat_request(port, "/api/agent/conversations", None);
    assert_eq!(saved[0]["ui"]["draft"], "Draft survives restart");
    assert_eq!(saved[0]["ui"]["task_context"]["title"], "Review draft");
    assert_eq!(
        saved[0]["ui"]["task_context"]["file"],
        "personal/project.md"
    );
    assert_eq!(saved[0]["entries"][0]["contextKind"], "checklist");
    assert_eq!(
        saved[0]["entries"][0]["contextTitle"],
        "Project: Review draft"
    );
    assert_eq!(saved[0]["ui"]["queue"].as_array().unwrap().len(), 1);
    assert_eq!(saved[0]["model"], "chat-override");
    let (status, resumed) = chat_request(
        port,
        "/api/agent/session",
        Some(json!({"thread_id":"thread-test"})),
    );
    assert_eq!(status, 200, "{resumed}");
    assert_eq!(resumed["model"], "chat-override");
    let calls: Vec<serde_json::Value> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        calls
            .iter()
            .filter(|call| call["method"] == "turn/start")
            .count(),
        1
    );
    let started = calls
        .iter()
        .find(|call| call["method"] == "thread/start")
        .unwrap();
    let resumed = calls
        .iter()
        .find(|call| call["method"] == "thread/resume")
        .unwrap();
    assert_eq!(started["params"]["cwd"], resumed["params"]["cwd"]);
    let prompt = calls
        .iter()
        .find(|call| call["method"] == "turn/start")
        .unwrap()["params"]["input"][0]["text"]
        .as_str()
        .unwrap();
    assert!(prompt.contains("project checklist item"));
    assert!(prompt.contains("- [ ] Review draft &alex"));
    assert!(prompt.contains("personal/project.md"));
    assert!(std::path::Path::new(started["params"]["cwd"].as_str().unwrap()).is_dir());
    assert_eq!(
        chat_request(
            port,
            "/api/agent/turn",
            Some(json!({"thread_id":"thread-test","request_id":"request-1","message":"Duplicate"}))
        )
        .0,
        502
    );
    let (status, error) = chat_request(
        port,
        "/api/agent/turn",
        Some(
            json!({"thread_id":"thread-test","request_id":"stale-reference","message":"Help",
        "context_task":{"file":"personal/project.md","line":2,"expected":"- [ ] Edited item"}}),
        ),
    );
    assert_eq!(status, 409, "{error}");
    // An unapplied edit and its old revision baseline survive another restart.
    assert_eq!(chat_request(port, "/api/agent/turn", Some(json!({"thread_id":"thread-test","request_id":"edit-request","message":"stage edit"}))).0, 200);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        assert!(std::time::Instant::now() < deadline, "edit did not finish");
        if chat_request(port, "/api/agent/conversations", None).1[0]["status"] == "review" {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let newer = "---\ntitle: Project\nstatus: active\n---\n\nNew live edit.\n";
    fs::write(data.join("personal/project.md"), newer).unwrap();
    drop(server);
    let (server, port) = start_chat_server(&data, &config, &runtime, &log);
    assert_eq!(
        chat_request(
            port,
            "/api/agent/session",
            Some(json!({"thread_id":"thread-test"}))
        )
        .0,
        200
    );
    assert_eq!(
        chat_request(
            port,
            "/api/agent/apply?thread_id=thread-test",
            Some(json!({}))
        )
        .0,
        409
    );
    assert_eq!(
        fs::read_to_string(data.join("personal/project.md")).unwrap(),
        newer
    );
    let (_, pending) = chat_request(port, "/api/agent/change?thread_id=thread-test", None);
    assert_eq!(pending["changed"], true);
    assert!(pending["diff"].as_str().unwrap().contains("Agent edit"));
    assert_eq!(
        chat_request(
            port,
            "/api/agent/reject?thread_id=thread-test",
            Some(json!({}))
        )
        .0,
        200
    );
    assert_eq!(
        chat_request(
            port,
            "/api/agent/turn",
            Some(
                json!({"thread_id":"thread-test","request_id":"request-3","message":"pause here"})
            )
        )
        .0,
        200
    );
    drop(server);
    let (server, port) = start_chat_server(&data, &config, &runtime, &log);
    let (_, saved) = chat_request(port, "/api/agent/conversations", None);
    assert_eq!(saved[0]["status"], "interrupted");
    assert!(saved[0]["active_turn_id"].is_null());
    assert_eq!(saved[0]["ui"]["queue"][0]["id"], "request-2");
    let (status, archived) = chat_request(
        port,
        "/api/agent/conversation?thread_id=thread-test",
        Some(json!({
            "revision": saved[0]["ui_revision"], "ui": saved[0]["ui"],
            "title": "Review later", "archived": true
        })),
    );
    assert_eq!(status, 200, "{archived}");
    let (status, error) = chat_request(
        port,
        "/api/agent/apply?thread_id=thread-test",
        Some(json!({})),
    );
    assert_eq!(status, 502);
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("Restore this archived chat"));
    drop(server);
    let (_server, port) = start_chat_server(&data, &config, &runtime, &log);
    let (_, saved) = chat_request(port, "/api/agent/conversations", None);
    assert_eq!(saved[0]["title"], "Review later");
    assert_eq!(saved[0]["archived"], true);
    assert_eq!(saved[0]["ui"]["queue"][0]["id"], "request-2");
    assert_eq!(saved[0]["ui"]["draft"], "Draft survives restart");
    assert_eq!(saved[0]["entries"], archived["entries"]);
    let calls = fs::read_to_string(log).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(
                |line| serde_json::from_str::<serde_json::Value>(line).unwrap()["method"]
                    == "turn/start"
            )
            .count(),
        3
    );
}

#[test]
fn private_server_announces_port_and_requires_auth_for_uncached_health() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_hq"))
            .args([
                "--dir",
                dir.path().to_str().unwrap(),
                "serve",
                "--port",
                "0",
                "--auth-token",
                "test-token",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = server.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).unwrap();
        let _ = tx.send(line);
    });
    let line = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("startup handshake");
    let ready: serde_json::Value =
        serde_json::from_str(line.strip_prefix("HQ_READY ").unwrap()).unwrap();
    let port = ready["port"].as_u64().unwrap();
    assert!((1..=65535).contains(&port));

    let request = |auth: &str| {
        let mut stream = TcpStream::connect(("127.0.0.1", port as u16)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(
            stream,
            "GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{auth}\r\n"
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    };
    assert!(request("").starts_with("HTTP/1.1 401"));
    let response = request("X-HQ-Token: test-token\r\n");
    assert!(response.starts_with("HTTP/1.1 204"));
    assert!(response.contains("cache-control: no-store\r\n"));
    assert!(response.ends_with("\r\n\r\n"));
}
