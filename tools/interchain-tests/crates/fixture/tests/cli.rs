use nomic_harness_fixture::{
    CanonicalId, Event, Request, Response, CRASH_EXIT_CODE, MAX_CONFIG_BYTES,
};
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

const STDIN_CONFIG: &[u8] = br#"{"config_canary":"config-private","component_id":"fixture","run_id":"run-a","network_id":"testnet","capabilities":["echo"]}"#;

fn spawn(mode: &str) -> Running {
    let mut child = command(mode).spawn().unwrap();
    child.stdin.take().unwrap().write_all(STDIN_CONFIG).unwrap();
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
    let event: nomic_harness_fixture::StartupEvent = serde_json::from_str(&startup).unwrap();
    let nomic_harness_fixture::StartupEvent::Listening { endpoint, .. } = event;
    let endpoint: SocketAddr = endpoint.parse().unwrap();
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
        .write_all(br#"{"config_canary":"private","component_id":"fixture","run_id":"run-a","network_id":"testnet","capabilities":["echo"],"unknown":true}"#)
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
        Some(Response::Readiness {
            report: nomic_harness_fixture::ReadinessIdentity::ready(
                nomic_harness_fixture::CanonicalId::new("fixture").unwrap(),
                nomic_harness_fixture::CanonicalId::new("run-a").unwrap(),
                nomic_harness_fixture::CanonicalId::new("testnet").unwrap(),
                [nomic_harness_fixture::CanonicalId::new("echo").unwrap()]
            )
            .unwrap()
        })
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
    let mut lines = stdout.lines();
    assert!(
        serde_json::from_str::<nomic_harness_fixture::StartupEvent>(lines.next().unwrap()).is_ok()
    );
    assert!(lines.all(|line| serde_json::from_str::<Event>(line).is_ok()));
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
fn production_process_driver_drains_overflow_and_rejects_invalid_specs_before_spawn() {
    use nomic_bridge_harness::driver::process::{
        ProcessContainment, ProcessDriver, ProcessError, ProcessSpec,
    };
    use nomic_bridge_harness::readiness::{AwaitBudget, ExpectedReadiness};
    use std::ffi::OsString;

    let make_spec = |component: &str| ProcessSpec {
        program: OsString::from(env!("CARGO_BIN_EXE_nomic-harness-fixture")),
        args: [
            "--listen",
            "127.0.0.1:0",
            "--mode",
            "healthy",
            "--argv-canary",
            "argv-private",
            "--log-canary",
            "synthetic-log-marker",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        env: vec![(OsString::from(ENV_NAME), OsString::from("env-private"))],
        stdin: STDIN_CONFIG.to_vec(),
        component_id: nomic_harness_fixture::CanonicalId::new(component).unwrap(),
        run_id: nomic_harness_fixture::CanonicalId::new("run-a").unwrap(),
        network_id: nomic_harness_fixture::CanonicalId::new("testnet").unwrap(),
        startup_deadline: TIMEOUT,
        containment: ProcessContainment::NoProcessGroupOrSessionEscape,
    };

    let driver = ProcessDriver::spawn(make_spec("fixture")).unwrap();
    let expected = ExpectedReadiness::new(
        nomic_harness_fixture::CanonicalId::new("fixture").unwrap(),
        nomic_harness_fixture::CanonicalId::new("run-a").unwrap(),
        nomic_harness_fixture::CanonicalId::new("testnet").unwrap(),
        [nomic_harness_fixture::CanonicalId::new("echo").unwrap()],
    )
    .unwrap();

    for _ in 0..40 {
        assert!(matches!(
            request(driver.endpoint().socket_addr(), &Request::Readiness),
            Some(Response::Readiness { .. })
        ));
    }
    let drop_deadline = Instant::now() + TIMEOUT;
    while driver.dropped_event_count() == 0 && Instant::now() < drop_deadline {
        std::thread::yield_now();
    }
    assert!(driver.dropped_event_count() > 0);
    assert_eq!(
        driver
            .await_ready(
                &expected,
                AwaitBudget::new(4, TIMEOUT, Duration::from_millis(10)).unwrap(),
            )
            .unwrap()
            .state,
        nomic_harness_fixture::ReadinessState::Ready
    );
    assert!(!driver.terminate().unwrap().success());

    assert!(ProcessDriver::spawn(make_spec("other")).is_err());

    let mut unsupported = make_spec("fixture");
    unsupported.program = OsString::from("program-must-not-exist");
    unsupported.containment = ProcessContainment::StrongerIsolationRequired;
    assert!(matches!(
        ProcessDriver::spawn(unsupported),
        Err(ProcessError::UnsupportedContainment)
    ));

    for env in [
        vec![
            (OsString::from(ENV_NAME), OsString::from("first")),
            (OsString::from(ENV_NAME), OsString::from("second")),
        ],
        vec![(OsString::from("invalid-name"), OsString::from("value"))],
        vec![(OsString::new(), OsString::from("value"))],
    ] {
        let mut spec = make_spec("fixture");
        spec.program = OsString::from("program-must-not-exist");
        spec.env = env;
        assert!(matches!(
            ProcessDriver::spawn(spec),
            Err(ProcessError::InvalidSpec)
        ));
    }
}

#[test]
fn watchdog_cancel_has_strict_deadline_precedence() {
    use nomic_bridge_harness::driver::process::{ProcessContainment, ProcessDriver, ProcessSpec};
    use nomic_bridge_harness::watchdog::{ArmedWatchdog, WatchdogOutcome};
    use std::ffi::OsString;

    let driver = ProcessDriver::spawn(ProcessSpec {
        program: OsString::from(env!("CARGO_BIN_EXE_nomic-harness-fixture")),
        args: [
            "--listen",
            "127.0.0.1:0",
            "--mode",
            "healthy",
            "--argv-canary",
            "argv-private",
            "--log-canary",
            "synthetic-log-marker",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        env: vec![(OsString::from(ENV_NAME), OsString::from("env-private"))],
        stdin: STDIN_CONFIG.to_vec(),
        component_id: CanonicalId::new("fixture").unwrap(),
        run_id: CanonicalId::new("run-a").unwrap(),
        network_id: CanonicalId::new("testnet").unwrap(),
        startup_deadline: TIMEOUT,
        containment: ProcessContainment::NoProcessGroupOrSessionEscape,
    })
    .unwrap();
    let stale_handle = driver.termination_handle();
    let receipt = ArmedWatchdog::arm(Duration::from_millis(200), stale_handle.clone())
        .unwrap()
        .cancel()
        .unwrap();
    assert_eq!(receipt.outcome(), WatchdogOutcome::Cancelled);
    assert!(driver.has_pending_cleanup().unwrap());
    assert!(matches!(
        request(driver.endpoint().socket_addr(), &Request::Readiness),
        Some(Response::Readiness { .. })
    ));
    driver.terminate().unwrap();
    assert!(stale_handle.terminate().unwrap().is_none());

    let watchdog = ArmedWatchdog::arm(Duration::from_millis(20), stale_handle).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let receipt = watchdog.cancel().unwrap();
    assert_eq!(receipt.outcome(), WatchdogOutcome::TimedOut);
}

#[cfg(target_os = "linux")]
#[test]
fn termination_handle_reaps_an_already_exited_direct_child() {
    use nomic_bridge_harness::driver::process::{ProcessContainment, ProcessDriver, ProcessSpec};
    use std::ffi::OsString;

    let driver = ProcessDriver::spawn(ProcessSpec {
        program: OsString::from(env!("CARGO_BIN_EXE_nomic-harness-fixture")),
        args: [
            "--listen",
            "127.0.0.1:0",
            "--mode",
            "healthy",
            "--argv-canary",
            "argv-private",
            "--log-canary",
            "synthetic-log-marker",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        env: vec![(OsString::from(ENV_NAME), OsString::from("env-private"))],
        stdin: STDIN_CONFIG.to_vec(),
        component_id: CanonicalId::new("fixture").unwrap(),
        run_id: CanonicalId::new("run-a").unwrap(),
        network_id: CanonicalId::new("testnet").unwrap(),
        startup_deadline: TIMEOUT,
        containment: ProcessContainment::NoProcessGroupOrSessionEscape,
    })
    .unwrap();
    let terminator = driver.termination_handle();
    assert!(matches!(
        request(driver.endpoint().socket_addr(), &Request::Stop),
        Some(Response::Stopped)
    ));

    let exit_deadline = Instant::now() + TIMEOUT;
    while !has_exited_without_reaping(driver.id()).unwrap() && Instant::now() < exit_deadline {
        std::thread::yield_now();
    }
    assert!(has_exited_without_reaping(driver.id()).unwrap());
    assert!(terminator.terminate().unwrap().unwrap().success());
    assert!(terminator.terminate().unwrap().is_none());
}

#[cfg(unix)]
#[test]
fn outer_watchdog_kills_and_reaps_hung_parent_and_descendant_group() {
    use nomic_bridge_harness::driver::process::{ProcessContainment, ProcessDriver, ProcessSpec};
    use nomic_bridge_harness::readiness::{AwaitBudget, ExpectedReadiness};
    use nomic_bridge_harness::watchdog::{ArmedWatchdog, WatchdogOutcome};
    use std::ffi::OsString;

    let driver = ProcessDriver::spawn(ProcessSpec {
        program: OsString::from(env!("CARGO_BIN_EXE_nomic-harness-fixture")),
        args: [
            "--listen",
            "127.0.0.1:0",
            "--mode",
            "hang-with-descendant",
            "--argv-canary",
            "argv-private",
            "--log-canary",
            "synthetic-log-marker",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        env: vec![(OsString::from(ENV_NAME), OsString::from("env-private"))],
        stdin: STDIN_CONFIG.to_vec(),
        component_id: CanonicalId::new("fixture").unwrap(),
        run_id: CanonicalId::new("run-a").unwrap(),
        network_id: CanonicalId::new("testnet").unwrap(),
        startup_deadline: TIMEOUT,
        containment: ProcessContainment::NoProcessGroupOrSessionEscape,
    })
    .unwrap();
    let parent_pid = driver.id();
    let expected = ExpectedReadiness::new(
        CanonicalId::new("fixture").unwrap(),
        CanonicalId::new("run-a").unwrap(),
        CanonicalId::new("testnet").unwrap(),
        [CanonicalId::new("echo").unwrap()],
    )
    .unwrap();
    driver
        .await_ready(
            &expected,
            AwaitBudget::new(4, TIMEOUT, Duration::from_millis(5)).unwrap(),
        )
        .unwrap();

    let watchdog =
        ArmedWatchdog::arm(Duration::from_millis(60), driver.termination_handle()).unwrap();
    let endpoint = driver.endpoint().socket_addr();
    let blocked_request = std::thread::spawn(move || {
        request(
            endpoint,
            &Request::Echo {
                payload: "hang".into(),
            },
        )
    });
    let child_pid = (0..16)
        .find_map(|_| {
            let diagnostic = driver.next_event(TIMEOUT).ok()?;
            let event: Event = serde_json::from_slice(&diagnostic).ok()?;
            (event.channel() == nomic_harness_fixture::EventChannel::Lifecycle
                && event.action() == "descendant_spawned")
                .then(|| event.value()?.parse().ok())
                .flatten()
        })
        .expect("bounded descendant diagnostic");
    let receipt = watchdog.wait().unwrap();
    assert_eq!(receipt.outcome(), WatchdogOutcome::TimedOut);
    assert!(receipt.elapsed() >= Duration::from_millis(50));
    blocked_request.join().unwrap();
    assert!(!driver.has_pending_cleanup().unwrap());

    assert!(!process_exists(parent_pid));
    assert!(!process_exists(child_pid));
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    unsafe { kill(pid, 0) == 0 }
}

#[cfg(target_os = "linux")]
fn has_exited_without_reaping(pid: u32) -> std::io::Result<bool> {
    let pid = libc::pid_t::try_from(pid)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid child pid"))?;
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid.cast_unsigned(),
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { info.assume_init().si_pid() } == pid)
}

#[test]
fn panic_and_crash_have_distinct_process_outcomes() {
    let mut panic = spawn("panic-after-readiness");
    assert_eq!(
        request(panic.endpoint(), &Request::Readiness),
        Some(Response::Readiness {
            report: nomic_harness_fixture::ReadinessIdentity::ready(
                nomic_harness_fixture::CanonicalId::new("fixture").unwrap(),
                nomic_harness_fixture::CanonicalId::new("run-a").unwrap(),
                nomic_harness_fixture::CanonicalId::new("testnet").unwrap(),
                [nomic_harness_fixture::CanonicalId::new("echo").unwrap()]
            )
            .unwrap()
        })
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
        Some(Response::Readiness {
            report: nomic_harness_fixture::ReadinessIdentity::ready(
                nomic_harness_fixture::CanonicalId::new("fixture").unwrap(),
                nomic_harness_fixture::CanonicalId::new("run-a").unwrap(),
                nomic_harness_fixture::CanonicalId::new("testnet").unwrap(),
                [nomic_harness_fixture::CanonicalId::new("echo").unwrap()]
            )
            .unwrap()
        })
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
        Some(Response::Readiness {
            report: nomic_harness_fixture::ReadinessIdentity::ready(
                nomic_harness_fixture::CanonicalId::new("fixture").unwrap(),
                nomic_harness_fixture::CanonicalId::new("run-a").unwrap(),
                nomic_harness_fixture::CanonicalId::new("testnet").unwrap(),
                [nomic_harness_fixture::CanonicalId::new("echo").unwrap()]
            )
            .unwrap()
        })
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
    assert!(process_exists(pid));
    drop(running);
    assert!(!process_exists(pid));
}
