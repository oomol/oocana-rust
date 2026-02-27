use assert_cmd::prelude::*;
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn oocana_cmd() -> Command {
    let mut cmd = Command::cargo_bin("oocana").unwrap();
    cmd.stdin(Stdio::null());
    cmd
}

/// Spawn the mock_server binary and wait until it accepts TCP connections.
/// Returns the child process (caller must kill it).
fn start_mock_server(port: u16) -> std::process::Child {
    let child = Command::cargo_bin("mock_server")
        .unwrap()
        .env("PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start mock_server");

    // Poll until the server accepts connections.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if Instant::now() > deadline {
            panic!("mock_server did not become ready within 10s");
        }
        if TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    child
}

#[test]
fn remote_task_flow() {
    let port: u16 = 23579;
    let mut server = start_mock_server(port);

    oocana_cmd()
        .args([
            "run",
            "examples/remote_task",
            "--task-api-url",
            &format!("http://127.0.0.1:{port}"),
            "--search-paths",
            "examples/remote_task",
        ])
        .assert()
        .success();

    server.kill().ok();
    server.wait().ok();
}
