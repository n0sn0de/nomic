use nomic_harness_fixture::{
    CanaryInputs, CanonicalId, EventChannel, FailureClass, Fixture, FixtureConfig, FixtureError,
    Mode, ReadinessIdentity, Request, Response, WorkDisposition, MAX_CANARY_BYTES, MAX_DELAY_MS,
    MAX_FAILURE_COUNT,
};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

const ARGV_CANARY: &str = "synthetic-argv-canary";
const ENV_CANARY: &str = "synthetic-env-canary";
const CONFIG_CANARY: &str = "synthetic-config-canary";
const REQUEST_CANARY: &str = "synthetic-request-canary";
const LOG_CANARY: &str = "synthetic-log-canary";

fn config(mode: Mode) -> FixtureConfig {
    FixtureConfig {
        listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        mode,
        canaries: CanaryInputs {
            argv: ARGV_CANARY.into(),
            named_env: ENV_CANARY.into(),
            config: CONFIG_CANARY.into(),
            intentional_log: LOG_CANARY.into(),
        },
        read_timeout: Duration::from_millis(100),
        write_timeout: Duration::from_millis(100),
        readiness: ReadinessIdentity::ready(
            CanonicalId::new("fixture").unwrap(),
            CanonicalId::new("run-a").unwrap(),
            CanonicalId::new("testnet").unwrap(),
            [CanonicalId::new("echo").unwrap()],
        )
        .unwrap(),
    }
}

#[test]
fn healthy_readiness_and_echo_are_deterministic_and_logs_are_channel_safe() {
    let mut fixture = Fixture::bind(config(Mode::Healthy)).unwrap();
    assert!(fixture.local_addr().ip().is_loopback());
    assert_ne!(fixture.local_addr().port(), 0);
    assert_eq!(
        fixture.handle(Request::Readiness),
        WorkDisposition::Reply(Response::Readiness {
            report: config(Mode::Healthy).readiness
        })
    );
    assert_eq!(
        fixture.handle(Request::Echo {
            payload: REQUEST_CANARY.into()
        }),
        WorkDisposition::Reply(Response::Echo {
            payload: REQUEST_CANARY.into()
        })
    );

    let events = fixture.events();
    let encoded = events
        .iter()
        .map(|event| event.to_json().unwrap())
        .collect::<Vec<_>>();
    assert!(encoded.iter().all(|line| line.len() <= 512));
    for secret_channel in [ARGV_CANARY, ENV_CANARY, CONFIG_CANARY, REQUEST_CANARY] {
        assert!(encoded.iter().all(|line| !line.contains(secret_channel)));
    }
    assert!(events
        .iter()
        .any(|event| event.channel() == EventChannel::IntentionalLog));
    assert!(encoded.iter().any(|line| line.contains(LOG_CANARY)));
}

#[test]
fn delayed_and_failure_modes_are_typed_and_bounded() {
    let origin = Instant::now();
    let delay = Duration::from_millis(200);
    let mut delayed =
        Fixture::bind_at(config(Mode::DelayedReadiness { delay_ms: 200 }), origin).unwrap();
    assert_eq!(
        delayed.try_handle_at(Request::Readiness, origin).unwrap(),
        WorkDisposition::Reply(Response::Readiness {
            report: ReadinessIdentity::not_ready(
                CanonicalId::new("fixture").unwrap(),
                CanonicalId::new("run-a").unwrap(),
                CanonicalId::new("testnet").unwrap(),
                [CanonicalId::new("echo").unwrap()]
            )
            .unwrap()
        })
    );
    assert_eq!(
        delayed
            .try_handle_at(Request::Readiness, origin + delay)
            .unwrap(),
        WorkDisposition::Reply(Response::Readiness {
            report: config(Mode::Healthy).readiness
        })
    );

    let mut transient = Fixture::bind(config(Mode::TransientFailure { failures: 2 })).unwrap();
    for remaining in [1, 0] {
        assert_eq!(
            transient.handle(Request::Echo {
                payload: "bounded".into()
            }),
            WorkDisposition::Reply(Response::Failure {
                class: FailureClass::Transient,
                remaining,
            })
        );
    }
    assert_eq!(
        transient.handle(Request::Echo {
            payload: "bounded".into()
        }),
        WorkDisposition::Reply(Response::Echo {
            payload: "bounded".into()
        })
    );

    let mut permanent = Fixture::bind(config(Mode::PermanentFailure)).unwrap();
    assert_eq!(
        permanent.handle(Request::Echo {
            payload: "bounded".into()
        }),
        WorkDisposition::Reply(Response::Failure {
            class: FailureClass::Permanent,
            remaining: 0
        })
    );
}

#[test]
fn line_protocol_round_trips_typed_requests_over_a_dynamic_loopback_socket() {
    let fixture = Fixture::bind(config(Mode::Healthy)).unwrap();
    let address = fixture.local_addr();
    let server = std::thread::spawn(|| fixture.serve());

    for (request, expected) in [
        (
            Request::Readiness,
            Response::Readiness {
                report: config(Mode::Healthy).readiness,
            },
        ),
        (
            Request::Echo {
                payload: "echo".into(),
            },
            Response::Echo {
                payload: "echo".into(),
            },
        ),
        (Request::Stop, Response::Stopped),
    ] {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        serde_json::to_writer(&mut stream, &request).unwrap();
        stream.write_all(b"\n").unwrap();
        let mut response = String::new();
        BufReader::new(stream).read_line(&mut response).unwrap();
        assert_eq!(
            serde_json::from_str::<Response>(&response).unwrap(),
            expected
        );
    }
    server.join().unwrap().unwrap();
}

#[test]
fn crash_and_hang_modes_activate_only_after_readiness() {
    let mut panic_fixture = Fixture::bind(config(Mode::PanicAfterReadiness)).unwrap();
    assert_eq!(
        panic_fixture.handle(Request::Readiness),
        WorkDisposition::Reply(Response::Readiness {
            report: config(Mode::Healthy).readiness
        })
    );
    assert_eq!(
        panic_fixture.handle(Request::Echo {
            payload: "x".into()
        }),
        WorkDisposition::Panic
    );

    let mut crash = Fixture::bind(config(Mode::CrashAfterReadiness)).unwrap();
    assert_eq!(
        crash.handle(Request::Readiness),
        WorkDisposition::Reply(Response::Readiness {
            report: config(Mode::Healthy).readiness
        })
    );
    assert_eq!(
        crash.handle(Request::Echo {
            payload: "x".into()
        }),
        WorkDisposition::Crash
    );

    let mut hang = Fixture::bind(config(Mode::Hang)).unwrap();
    assert_eq!(
        hang.handle(Request::Readiness),
        WorkDisposition::Reply(Response::Readiness {
            report: config(Mode::Healthy).readiness
        })
    );
    assert_eq!(
        hang.handle(Request::Echo {
            payload: "x".into()
        }),
        WorkDisposition::Hang
    );
}

#[test]
fn rejects_unsafe_or_unbounded_configuration_and_inputs() {
    let mut non_loopback = config(Mode::Healthy);
    non_loopback.listen_addr = "192.0.2.1:0".parse().unwrap();
    assert!(matches!(
        Fixture::bind(non_loopback),
        Err(FixtureError::NonLoopbackListener)
    ));

    let mut fixed_port = config(Mode::Healthy);
    fixed_port
        .listen_addr
        .set_port(std::num::NonZeroU16::MIN.get());
    assert!(matches!(
        Fixture::bind(fixed_port),
        Err(FixtureError::NonDynamicPort)
    ));

    for mode in [
        Mode::DelayedReadiness {
            delay_ms: MAX_DELAY_MS + 1,
        },
        Mode::TransientFailure {
            failures: MAX_FAILURE_COUNT + 1,
        },
    ] {
        assert!(matches!(
            Fixture::bind(config(mode)),
            Err(FixtureError::InvalidMode(_))
        ));
    }

    for channel in ["argv", "named_env", "config", "intentional_log"] {
        let mut oversized = config(Mode::Healthy);
        let value = "x".repeat(MAX_CANARY_BYTES + 1);
        match channel {
            "argv" => oversized.canaries.argv = value,
            "named_env" => oversized.canaries.named_env = value,
            "config" => oversized.canaries.config = value,
            "intentional_log" => oversized.canaries.intentional_log = value,
            _ => unreachable!(),
        }
        assert!(matches!(
            Fixture::bind(oversized),
            Err(FixtureError::OversizedCanary(found)) if found == channel
        ));
    }

    let mut fixture = Fixture::bind(config(Mode::Healthy)).unwrap();
    assert!(matches!(
        fixture.try_handle(Request::Echo {
            payload: "x".repeat(MAX_CANARY_BYTES + 1)
        }),
        Err(FixtureError::OversizedRequest)
    ));

    for deadline in ["read", "write"] {
        for timeout in [Duration::ZERO, Duration::from_secs(3)] {
            let mut invalid_deadline = config(Mode::Healthy);
            match deadline {
                "read" => invalid_deadline.read_timeout = timeout,
                "write" => invalid_deadline.write_timeout = timeout,
                _ => unreachable!(),
            }
            assert!(matches!(
                Fixture::bind(invalid_deadline),
                Err(FixtureError::InvalidDeadline)
            ));
        }
    }
}
