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
