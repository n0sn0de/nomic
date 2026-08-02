use nomic_bridge_harness::driver::DynamicEndpoint;
use nomic_bridge_harness::readiness::{
    await_ready, await_ready_with_receipt, AwaitBudget, ExpectedReadiness, ReadinessErrorClass,
};
use nomic_bridge_harness::retry::RetryOutcome;
use nomic_bridge_harness::topology::Topology;
use nomic_harness_protocol::{CanonicalId, ReadinessIdentity, ReadinessState, Response};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

fn id(value: &str) -> CanonicalId {
    CanonicalId::new(value).unwrap()
}

#[test]
fn topology_is_typed_and_bounded() {
    let source = include_str!("../../../scenarios/topology/smoke.toml");
    let topology = Topology::parse(source).unwrap();
    assert_eq!(topology.id().as_str(), "smoke");
    assert_eq!(topology.nodes().len(), 1);
    for forbidden in ["port", "path", "address", "network"] {
        let altered = format!("{source}\n{forbidden} = \"forbidden\"\n");
        assert!(Topology::parse(&altered).is_err());
    }
}

#[test]
fn concurrent_dynamic_endpoints_are_unique_and_validated() {
    let listeners = (0..8)
        .map(|_| TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap())
        .collect::<Vec<_>>();
    let endpoints = listeners
        .iter()
        .map(|listener| DynamicEndpoint::from_reserved_listener(listener).unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(endpoints.len(), listeners.len());
}

#[test]
fn missing_listener_exhausts_a_stable_budget() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let endpoint = DynamicEndpoint::from_reserved_listener(&listener).unwrap();
    drop(listener);
    let expected =
        ExpectedReadiness::new(id("fixture"), id("run-a"), id("testnet"), [id("echo")]).unwrap();
    let error = await_ready(
        endpoint,
        &expected,
        AwaitBudget::new(3, Duration::from_millis(80), Duration::from_millis(5)).unwrap(),
    )
    .unwrap_err();
    assert_eq!(error.class(), ReadinessErrorClass::AttemptsExhausted);
    assert_eq!(error.attempts(), 3);
    assert_eq!(
        error.receipt().unwrap().outcome(),
        RetryOutcome::AttemptsExhausted
    );
}

#[test]
fn expected_identity_constructs_the_shared_report_contract() {
    let identity =
        ReadinessIdentity::ready(id("fixture"), id("run-a"), id("testnet"), [id("echo")]).unwrap();
    let encoded = serde_json::to_string(&identity).unwrap();
    assert!(encoded.contains("readiness-v1"));
}

fn expected() -> ExpectedReadiness {
    ExpectedReadiness::new(id("fixture"), id("run-a"), id("testnet"), [id("echo")]).unwrap()
}

struct MockServer {
    done: mpsc::Receiver<()>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl MockServer {
    fn finish(mut self) {
        self.done
            .recv_timeout(Duration::from_secs(1))
            .expect("mock completion deadline");
        self.thread.take().unwrap().join().unwrap();
    }
}
impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if self.done.recv_timeout(Duration::from_secs(1)).is_ok() {
            if let Some(thread) = self.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

fn serve_responses(lines: Vec<Vec<u8>>) -> (DynamicEndpoint, MockServer) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = DynamicEndpoint::from_reserved_listener(&listener).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let (done_tx, done) = mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        for line in lines {
            let deadline = Instant::now() + Duration::from_secs(1);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline
                            && !thread_stop.load(Ordering::Acquire) =>
                    {
                        std::thread::yield_now();
                    }
                    Err(_) => {
                        let _ = done_tx.send(());
                        return;
                    }
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .unwrap();
            let mut byte = [0_u8; 1];
            while stream.read_exact(&mut byte).is_ok() && byte[0] != b'\n' {}
            stream.write_all(&line).unwrap();
        }
        let _ = done_tx.send(());
    });
    (
        endpoint,
        MockServer {
            done,
            stop,
            thread: Some(thread),
        },
    )
}

fn encoded(report: ReadinessIdentity) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&Response::Readiness { report }).unwrap();
    bytes.push(b'\n');
    bytes
}

#[test]
fn exact_identity_and_delayed_not_ready_succeed_within_budget() {
    let not_ready =
        ReadinessIdentity::not_ready(id("fixture"), id("run-a"), id("testnet"), [id("echo")])
            .unwrap();
    let ready =
        ReadinessIdentity::ready(id("fixture"), id("run-a"), id("testnet"), [id("echo")]).unwrap();
    let (endpoint, server) = serve_responses(vec![encoded(not_ready), encoded(ready)]);
    let report = await_ready(
        endpoint,
        &expected(),
        AwaitBudget::new(2, Duration::from_millis(200), Duration::from_millis(5)).unwrap(),
    )
    .unwrap();
    assert_eq!(report.state, ReadinessState::Ready);
    server.finish();
}

#[test]
fn not_ready_success_exposes_the_exact_retry_receipt() {
    let not_ready =
        ReadinessIdentity::not_ready(id("fixture"), id("run-a"), id("testnet"), [id("echo")])
            .unwrap();
    let ready =
        ReadinessIdentity::ready(id("fixture"), id("run-a"), id("testnet"), [id("echo")]).unwrap();
    let (endpoint, server) = serve_responses(vec![
        encoded(not_ready.clone()),
        encoded(not_ready),
        encoded(ready),
    ]);
    let success = await_ready_with_receipt(
        endpoint,
        &expected(),
        AwaitBudget::new(3, Duration::from_millis(200), Duration::from_millis(2)).unwrap(),
    )
    .unwrap();
    assert_eq!(success.receipt.attempts(), 3);
    assert_eq!(success.receipt.outcome(), RetryOutcome::Succeeded);
    server.finish();
}

#[test]
fn identity_schema_and_capability_errors_are_permanent_on_first_attempt() {
    let cases = [
        (
            ReadinessIdentity::ready(id("other"), id("run-a"), id("testnet"), [id("echo")])
                .unwrap(),
            ReadinessErrorClass::IdentityMismatch,
        ),
        (
            ReadinessIdentity::ready(id("fixture"), id("run-b"), id("testnet"), [id("echo")])
                .unwrap(),
            ReadinessErrorClass::IdentityMismatch,
        ),
        (
            ReadinessIdentity::ready(id("fixture"), id("run-a"), id("mainnet"), [id("echo")])
                .unwrap(),
            ReadinessErrorClass::IdentityMismatch,
        ),
        (
            ReadinessIdentity::new(
                "readiness-v2".into(),
                id("fixture"),
                id("run-a"),
                id("testnet"),
                ReadinessState::Ready,
                [id("echo")],
            )
            .unwrap(),
            ReadinessErrorClass::WrongSchema,
        ),
        (
            ReadinessIdentity::ready(id("fixture"), id("run-a"), id("testnet"), []).unwrap(),
            ReadinessErrorClass::CapabilityMismatch,
        ),
    ];
    for (report, class) in cases {
        assert!(report.validate().is_ok());
        let wire = encoded(report);
        assert!(serde_json::from_slice::<Response>(&wire).is_ok());
        let (endpoint, server) = serve_responses(vec![wire]);
        let error = await_ready(
            endpoint,
            &expected(),
            AwaitBudget::new(4, Duration::from_millis(200), Duration::from_millis(5)).unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.class(), class);
        assert_eq!(error.attempts(), 1);
        assert!(error.is_permanent());
        assert_eq!(error.receipt().unwrap().outcome(), RetryOutcome::Permanent);
        server.finish();
    }
}

#[test]
fn malformed_and_oversized_responses_fail_permanently() {
    for (reply, class) in [
        (
            b"not-json\n".to_vec(),
            ReadinessErrorClass::MalformedResponse,
        ),
        (vec![b'x'; 2049], ReadinessErrorClass::OversizedResponse),
    ] {
        let (endpoint, server) = serve_responses(vec![reply]);
        let error = await_ready(
            endpoint,
            &expected(),
            AwaitBudget::new(3, Duration::from_millis(200), Duration::from_millis(5)).unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.class(), class);
        assert_eq!(error.attempts(), 1);
        assert!(error.is_permanent());
        assert_eq!(error.receipt().unwrap().outcome(), RetryOutcome::Permanent);
        server.finish();
    }
}

#[test]
fn capability_vectors_are_exact_canonical_and_permanent() {
    let reports = [
        (
            r#"{"type":"readiness","report":{"schema":"readiness-v1","component_id":"fixture","run_id":"run-a","network_id":"testnet","state":"ready","capabilities":[]}}
"#,
            ReadinessErrorClass::CapabilityMismatch,
        ),
        (
            r#"{"type":"readiness","report":{"schema":"readiness-v1","component_id":"fixture","run_id":"run-a","network_id":"testnet","state":"ready","capabilities":["echo","extra"]}}
"#,
            ReadinessErrorClass::CapabilityMismatch,
        ),
        (
            r#"{"type":"readiness","report":{"schema":"readiness-v1","component_id":"fixture","run_id":"run-a","network_id":"testnet","state":"ready","capabilities":["echo","echo"]}}
"#,
            ReadinessErrorClass::MalformedResponse,
        ),
        (
            r#"{"type":"readiness","report":{"schema":"readiness-v1","component_id":"fixture","run_id":"run-a","network_id":"testnet","state":"ready","capabilities":["zeta","echo"]}}
"#,
            ReadinessErrorClass::MalformedResponse,
        ),
        (
            r#"{"type":"readiness","report":{"schema":"readiness-v1","component_id":"fixture","run_id":"run-a","network_id":"testnet","state":"ready","capabilities":["Not-Canonical"]}}
"#,
            ReadinessErrorClass::MalformedResponse,
        ),
    ];
    for (wire, class) in reports {
        let (endpoint, server) = serve_responses(vec![wire.as_bytes().to_vec()]);
        let error = await_ready(
            endpoint,
            &expected(),
            AwaitBudget::new(3, Duration::from_millis(200), Duration::from_millis(5)).unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.class(), class);
        assert_eq!(error.attempts(), 1);
        assert!(error.is_permanent());
        server.finish();
    }
    let invalid = ExpectedReadiness::new(
        id("fixture"),
        id("run-a"),
        id("testnet"),
        [id("zeta"), id("echo")],
    )
    .unwrap_err();
    assert_eq!(invalid.class(), ReadinessErrorClass::InvalidCapabilities);
}

#[test]
fn stalled_response_cannot_multiply_the_supervisor_deadline() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = DynamicEndpoint::from_reserved_listener(&listener).unwrap();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let server = std::thread::spawn(move || {
        let accept_deadline = Instant::now() + Duration::from_secs(1);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < accept_deadline =>
                {
                    std::thread::yield_now()
                }
                Err(error) => panic!("bounded stall accept failed: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut byte = [0_u8; 1];
        while stream.read_exact(&mut byte).is_ok() && byte[0] != b'\n' {}
        std::thread::sleep(Duration::from_millis(150));
        done_tx.send(()).unwrap();
    });
    let declared = Duration::from_millis(60);
    let started = Instant::now();
    let error = await_ready_with_receipt(
        endpoint,
        &expected(),
        AwaitBudget::new(4, declared, Duration::from_millis(5)).unwrap(),
    )
    .unwrap_err();
    assert_eq!(error.class(), ReadinessErrorClass::DeadlineExceeded);
    let elapsed = started.elapsed();
    assert!(elapsed >= declared);
    assert!(elapsed <= declared + Duration::from_millis(80));
    done_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("stall completion deadline");
    server.join().unwrap();
}

#[test]
fn source_and_topology_have_no_fixed_loopback_service_port() {
    const MAX_FILES: usize = 128;
    const MAX_FILE_BYTES: u64 = 1024 * 1024;
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut pending = vec![workspace.join("scenarios")];
    pending.extend(
        std::fs::read_dir(workspace.join("crates"))
            .unwrap()
            .map(|entry| entry.unwrap().path().join("src")),
    );
    let mut files = 0;
    while let Some(path) = pending.pop() {
        if !path.exists() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert!(!metadata.file_type().is_symlink(), "guard refuses symlinks");
        if metadata.is_dir() {
            pending.extend(
                std::fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
            assert!(pending.len() <= MAX_FILES);
            continue;
        }
        files += 1;
        assert!(files <= MAX_FILES && metadata.len() <= MAX_FILE_BYTES);
        let source = std::fs::read_to_string(&path).unwrap();
        assert!(
            !has_fixed_loopback(&source),
            "fixed loopback endpoint in {}",
            path.display()
        );
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            let value: toml::Value = toml::from_str(&source).unwrap();
            assert!(
                !has_service_port(&value),
                "service port declaration in {}",
                path.display()
            );
            if path
                .parent()
                .is_some_and(|parent| parent.ends_with("topology"))
            {
                Topology::parse(&source).unwrap();
            }
        }
    }
}

fn has_fixed_loopback(source: &str) -> bool {
    ["127.0.0.1:", "[::1]:"].iter().any(|marker| {
        source.match_indices(marker).any(|(offset, marker)| {
            let suffix = &source[offset + marker.len()..];
            let port = suffix
                .bytes()
                .take_while(u8::is_ascii_digit)
                .collect::<Vec<_>>();
            !port.is_empty() && port != b"0"
        })
    })
}

fn has_service_port(value: &toml::Value) -> bool {
    match value {
        toml::Value::Table(table) => table.iter().any(|(key, value)| {
            (key == "port" || key.ends_with("_port"))
                && value.as_integer().is_some_and(|port| port != 0)
                || has_service_port(value)
        }),
        toml::Value::Array(values) => values.iter().any(has_service_port),
        _ => false,
    }
}
