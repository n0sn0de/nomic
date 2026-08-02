use nomic_harness_fixture::{Event, Request, Response, CRASH_EXIT_CODE, MAX_CONFIG_BYTES};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const ENV_NAME: &str = "NOMIC_FIXTURE_SYNTHETIC_CANARY";
const TIMEOUT: Duration = Duration::from_secs(3);

struct Running {
    child: Option<Child>,
    stdout_lines: mpsc::Receiver<String>,
    observed_stdout: Vec<String>,
    stderr: ChildStderr,
    endpoint: Option<SocketAddr>,
}

impl Running {
    fn endpoint(&self) -> SocketAddr {
        self.endpoint.expect("startup endpoint has been observed")
    }

    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("subprocess has not been reaped")
    }

    fn wait_deadline(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child_mut().try_wait().unwrap() {
                self.child = None;
                return status;
            }
            if Instant::now() >= deadline {
                let mut child = self.child.take().unwrap();
                child.kill().unwrap();
                let status = child.wait().unwrap();
                panic!("subprocess timed out and was killed: {status}");
            }
            std::thread::yield_now();
        }
    }

    fn kill_and_reap(&mut self) -> ExitStatus {
        let mut child = self.child.take().expect("subprocess has not been reaped");
        child.kill().unwrap();
        child.wait().unwrap()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.kill();
        let (reaped, receiver) = mpsc::sync_channel(1);
        let _ = std::thread::Builder::new()
            .name("fixture-child-reaper".into())
            .spawn(move || {
                let _ = child.wait();
                let _ = reaped.send(());
            });
        let _ = receiver.recv_timeout(TIMEOUT);
    }
}

fn command(mode: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nomic-harness-fixture"));
    command
        .args([
            "--listen",
            "127.0.0.1:0",
            "--mode",
            mode,
            "--argv-canary",
            "argv-private",
            "--log-canary",
            "synthetic-log-marker",
        ])
        .env_clear()
        .env(ENV_NAME, "env-private")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn spawn(mode: &str) -> Running {
    let mut child = command(mode).spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"config_canary":"config-private"}"#)
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (sender, stdout_lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if sender.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    // Establish child ownership before any startup assertion can panic. The
    // Drop guard then kills and reaps malformed or stalled subprocesses.
    let mut running = Running {
        child: Some(child),
        stdout_lines,
        observed_stdout: Vec::new(),
        stderr,
        endpoint: None,
    };
    let startup = running
        .stdout_lines
        .recv_timeout(TIMEOUT)
        .expect("startup event deadline");
    let event: Event = serde_json::from_str(&startup).unwrap();
    assert_eq!(event.action(), "started");
    let endpoint: SocketAddr = event.value().unwrap().parse().unwrap();
    assert!(endpoint.ip().is_loopback());
    assert_ne!(endpoint.port(), 0);
    running.endpoint = Some(endpoint);
    running.observed_stdout.push(startup);
    for _ in 0..4 {
        running.observed_stdout.push(
            running
                .stdout_lines
                .recv_timeout(TIMEOUT)
                .expect("initial event deadline"),
        );
    }
    running
}

fn request(endpoint: SocketAddr, request: &Request) -> Option<Response> {
    let mut stream = TcpStream::connect_timeout(&endpoint, Duration::from_millis(500)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    serde_json::to_writer(&mut stream, request).unwrap();
    stream.write_all(b"\n").unwrap();
    let mut line = String::new();
    match BufReader::new(stream).read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(serde_json::from_str(&line).unwrap()),
    }
}

fn stop(mut running: Running) -> (String, String) {
    assert_eq!(
        request(running.endpoint(), &Request::Stop),
        Some(Response::Stopped)
    );
    assert!(running.wait_deadline(TIMEOUT).success());
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            panic!("stdout reader missed its deadline");
        };
        match running.stdout_lines.recv_timeout(remaining) {
            Ok(line) => running.observed_stdout.push(line),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => panic!("stdout reader missed its deadline"),
        }
    }
    let stdout = running.observed_stdout.join("\n");
    let mut stderr = String::new();
    running.stderr.read_to_string(&mut stderr).unwrap();
    (stdout, stderr)
}

#[test]
fn cli_parsing_and_stdin_config_are_bounded_and_typed() {
    let output = Command::new(env!("CARGO_BIN_EXE_nomic-harness-fixture"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("<loopback>:0"));
    assert!(!help.contains("127.0.0.1"));

    let mut unknown = command("healthy");
    unknown.arg("--unknown").arg("value");
    let output = unknown.output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("config-private"));

    let mut missing_env = command("healthy");
    missing_env.env_remove(ENV_NAME);
    let output = missing_env.output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains(ENV_NAME));

    let mut unknown_config = command("healthy").spawn().unwrap();
    unknown_config
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"config_canary":"private","unknown":true}"#)
        .unwrap();
    let output = unknown_config.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private"));

    let mut oversized = command("healthy").spawn().unwrap();
    oversized
        .stdin
        .take()
        .unwrap()
        .write_all(&vec![b'x'; MAX_CONFIG_BYTES + 1])
        .unwrap();
    let output = oversized.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("exceeds bound"));
}

#[test]
fn startup_endpoint_protocol_events_and_stream_separation_are_real() {
    let running = spawn("healthy");
    assert_eq!(
        request(running.endpoint(), &Request::Readiness),
        Some(Response::Ready)
    );
    assert_eq!(
        request(
            running.endpoint(),
            &Request::Echo {
                payload: "request-private".into()
            }
        ),
        Some(Response::Echo {
            payload: "request-private".into()
        })
    );
    let (stdout, stderr) = stop(running);
    assert!(stderr.is_empty());
    assert!(stdout
        .lines()
        .all(|line| serde_json::from_str::<Event>(line).is_ok()));
    assert!(stdout.contains("synthetic-log-marker"));
    for marker in [
        "argv-private",
        "env-private",
        "config-private",
        "request-private",
    ] {
        assert!(!stdout.contains(marker));
    }
    assert!(stdout.contains("\"action\":\"stopped\""));
}

#[test]
fn panic_and_crash_have_distinct_process_outcomes() {
    let mut panic = spawn("panic-after-readiness");
    assert_eq!(
        request(panic.endpoint(), &Request::Readiness),
        Some(Response::Ready)
    );
    assert_eq!(
        request(
            panic.endpoint(),
            &Request::Echo {
                payload: "x".into()
            }
        ),
        None
    );
    let panic_status = panic.wait_deadline(TIMEOUT);
    assert!(!panic_status.success());
    assert_ne!(panic_status.code(), Some(CRASH_EXIT_CODE));
    let mut panic_stderr = String::new();
    panic.stderr.read_to_string(&mut panic_stderr).unwrap();
    assert!(panic_stderr.contains("panic-after-readiness"));
    assert!(!panic_stderr.contains("synthetic-log-marker"));

    let mut crash = spawn("crash-after-readiness");
    assert_eq!(
        request(crash.endpoint(), &Request::Readiness),
        Some(Response::Ready)
    );
    assert_eq!(
        request(
            crash.endpoint(),
            &Request::Echo {
                payload: "x".into()
            }
        ),
        None
    );
    assert_eq!(crash.wait_deadline(TIMEOUT).code(), Some(CRASH_EXIT_CODE));
    let mut crash_stderr = String::new();
    crash.stderr.read_to_string(&mut crash_stderr).unwrap();
    assert!(crash_stderr.is_empty());
}

#[test]
fn hang_requires_and_receives_bounded_supervisor_cleanup() {
    let mut running = spawn("hang");
    assert_eq!(
        request(running.endpoint(), &Request::Readiness),
        Some(Response::Ready)
    );
    assert_eq!(
        request(
            running.endpoint(),
            &Request::Echo {
                payload: "x".into()
            }
        ),
        None
    );
    let deadline = Instant::now() + Duration::from_millis(150);
    while Instant::now() < deadline {
        assert!(running.child_mut().try_wait().unwrap().is_none());
        std::thread::yield_now();
    }
    let status = running.kill_and_reap();
    assert!(!status.success());
}

#[cfg(target_os = "linux")]
#[test]
fn running_drop_kills_and_reaps_an_unstopped_child() {
    let running = spawn("hang");
    let pid = running.child.as_ref().unwrap().id();
    assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
    drop(running);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}
