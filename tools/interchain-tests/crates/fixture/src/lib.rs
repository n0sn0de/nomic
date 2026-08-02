//! Bounded synthetic lifecycle target used by the process harness.

pub use nomic_harness_protocol::{
    CanonicalId, FailureClass, ReadinessIdentity, ReadinessState, Request, Response, StartupEvent,
    MAX_STARTUP_EVENT_BYTES,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

pub const MAX_CANARY_BYTES: usize = 64;
pub const MAX_CONFIG_BYTES: usize = 1024;
pub const MAX_REQUEST_BYTES: usize = 256;
pub const MAX_EVENT_BYTES: usize = 512;
pub const MAX_RECORDED_EVENTS: usize = 64;
pub const MAX_DELAY_MS: u64 = 250;
pub const MAX_FAILURE_COUNT: u16 = 8;
pub const CRASH_EXIT_CODE: i32 = 70;
pub const EVENT_WRITER_QUEUE_CAPACITY: usize = 8;
pub const EVENT_WRITE_DEADLINE: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Healthy,
    DelayedReadiness { delay_ms: u64 },
    TransientFailure { failures: u16 },
    PermanentFailure,
    PanicAfterReadiness,
    CrashAfterReadiness,
    Hang,
    HangWithDescendant,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StdinConfig {
    pub config_canary: String,
    pub component_id: CanonicalId,
    pub run_id: CanonicalId,
    pub network_id: CanonicalId,
    pub capabilities: Vec<CanonicalId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanaryInputs {
    pub argv: String,
    pub named_env: String,
    pub config: String,
    pub intentional_log: String,
}

#[derive(Clone, Debug)]
pub struct FixtureConfig {
    pub listen_addr: SocketAddr,
    pub mode: Mode,
    pub canaries: CanaryInputs,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub readiness: ReadinessIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkDisposition {
    Reply(Response),
    Panic,
    Crash,
    Hang,
    HangWithDescendant,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventChannel {
    Lifecycle,
    ArgvCanary,
    NamedEnvCanary,
    ConfigCanary,
    RequestCanary,
    IntentionalLog,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    schema: String,
    channel: EventChannel,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
}

impl Event {
    pub fn channel(&self) -> EventChannel {
        self.channel
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    pub fn to_json(&self) -> Result<String, FixtureError> {
        let encoded = serde_json::to_string(self).map_err(FixtureError::Json)?;
        if encoded.len() > MAX_EVENT_BYTES {
            return Err(FixtureError::OversizedEvent);
        }
        Ok(encoded)
    }
}

#[derive(Debug)]
pub enum FixtureError {
    NonLoopbackListener,
    NonDynamicPort,
    InvalidMode(&'static str),
    OversizedCanary(&'static str),
    OversizedConfig,
    OversizedRequest,
    OversizedEvent,
    InvalidDeadline,
    EventWriterBackpressure,
    EventWriterDeadline,
    EventWriterStopped,
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidReadiness,
}

impl fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonLoopbackListener => formatter.write_str("listener must be loopback"),
            Self::NonDynamicPort => formatter.write_str("listener port must be zero"),
            Self::InvalidMode(reason) => write!(formatter, "invalid mode: {reason}"),
            Self::OversizedCanary(channel) => write!(formatter, "{channel} canary exceeds bound"),
            Self::OversizedConfig => formatter.write_str("stdin config exceeds bound"),
            Self::OversizedRequest => formatter.write_str("request exceeds bound"),
            Self::OversizedEvent => formatter.write_str("event exceeds bound"),
            Self::InvalidDeadline => formatter.write_str("read/write deadlines must be bounded"),
            Self::EventWriterBackpressure => formatter.write_str("event writer queue is full"),
            Self::EventWriterDeadline => formatter.write_str("event writer missed its deadline"),
            Self::EventWriterStopped => formatter.write_str("event writer stopped"),
            Self::Io(error) => write!(formatter, "fixture I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "fixture JSON failed: {error}"),
            Self::InvalidReadiness => formatter.write_str("invalid readiness identity"),
        }
    }
}

impl std::error::Error for FixtureError {}

impl From<std::io::Error> for FixtureError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn read_stdin_config(reader: impl Read) -> Result<StdinConfig, FixtureError> {
    let mut encoded = Vec::new();
    reader
        .take((MAX_CONFIG_BYTES + 1) as u64)
        .read_to_end(&mut encoded)?;
    if encoded.len() > MAX_CONFIG_BYTES {
        return Err(FixtureError::OversizedConfig);
    }
    serde_json::from_slice(&encoded).map_err(FixtureError::Json)
}

pub struct Fixture {
    listener: TcpListener,
    mode: Mode,
    started: Instant,
    remaining_failures: u16,
    readiness_observed: bool,
    read_timeout: Duration,
    write_timeout: Duration,
    readiness: ReadinessIdentity,
    events: Vec<Event>,
    dropped_event_count: usize,
    event_writer: Option<EventWriter>,
}

struct WriteRequest {
    encoded: String,
    acknowledgement: mpsc::SyncSender<Result<(), std::io::Error>>,
}

struct EventWriter {
    requests: SyncSender<WriteRequest>,
}

impl EventWriter {
    fn spawn(mut writer: Box<dyn Write + Send>) -> Self {
        let (requests, receiver) = mpsc::sync_channel::<WriteRequest>(EVENT_WRITER_QUEUE_CAPACITY);
        thread::spawn(move || {
            while let Ok(request) = receiver.recv() {
                let result = writer
                    .write_all(request.encoded.as_bytes())
                    .and_then(|()| writer.write_all(b"\n"))
                    .and_then(|()| writer.flush());
                let failed = result.is_err();
                let _ = request.acknowledgement.send(result);
                if failed {
                    break;
                }
            }
        });
        Self { requests }
    }

    fn write(&self, encoded: String) -> Result<(), FixtureError> {
        let (acknowledgement, receiver) = mpsc::sync_channel(1);
        self.requests
            .try_send(WriteRequest {
                encoded,
                acknowledgement,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => FixtureError::EventWriterBackpressure,
                mpsc::TrySendError::Disconnected(_) => FixtureError::EventWriterStopped,
            })?;
        match receiver.recv_timeout(EVENT_WRITE_DEADLINE) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(FixtureError::Io(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(FixtureError::EventWriterDeadline),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(FixtureError::EventWriterStopped),
        }
    }
}

impl Fixture {
    pub fn bind(config: FixtureConfig) -> Result<Self, FixtureError> {
        Self::bind_at(config, Instant::now())
    }

    pub fn bind_with_event_writer(
        config: FixtureConfig,
        writer: impl Write + Send + 'static,
    ) -> Result<Self, FixtureError> {
        Self::bind_with_event_writer_at(config, writer, Instant::now())
    }

    #[doc(hidden)]
    pub fn bind_at(config: FixtureConfig, now: Instant) -> Result<Self, FixtureError> {
        Self::bind_inner(config, None, now)
    }

    #[doc(hidden)]
    pub fn bind_with_event_writer_at(
        config: FixtureConfig,
        writer: impl Write + Send + 'static,
        now: Instant,
    ) -> Result<Self, FixtureError> {
        Self::bind_inner(config, Some(EventWriter::spawn(Box::new(writer))), now)
    }

    fn bind_inner(
        config: FixtureConfig,
        event_writer: Option<EventWriter>,
        now: Instant,
    ) -> Result<Self, FixtureError> {
        validate_config(&config)?;
        let listener = TcpListener::bind(config.listen_addr)?;
        let remaining_failures = match config.mode {
            Mode::TransientFailure { failures } => failures,
            _ => 0,
        };
        let mut fixture = Self {
            listener,
            mode: config.mode,
            started: now,
            remaining_failures,
            readiness_observed: false,
            read_timeout: config.read_timeout,
            write_timeout: config.write_timeout,
            readiness: config.readiness,
            events: Vec::with_capacity(8),
            dropped_event_count: 0,
            event_writer,
        };
        let startup = StartupEvent::listening(
            fixture.local_addr().to_string(),
            fixture.readiness.component_id.clone(),
            fixture.readiness.run_id.clone(),
            fixture.readiness.network_id.clone(),
        )
        .map_err(|_| FixtureError::InvalidReadiness)?;
        let encoded = serde_json::to_string(&startup).map_err(FixtureError::Json)?;
        if encoded.len() > MAX_STARTUP_EVENT_BYTES {
            return Err(FixtureError::OversizedEvent);
        }
        if let Some(writer) = &fixture.event_writer {
            writer.write(encoded)?;
        }
        fixture.record(EventChannel::ArgvCanary, "observed", None)?;
        fixture.record(EventChannel::NamedEnvCanary, "observed", None)?;
        fixture.record(EventChannel::ConfigCanary, "observed", None)?;
        fixture.record(
            EventChannel::IntentionalLog,
            "emitted",
            Some(config.canaries.intentional_log),
        )?;
        Ok(fixture)
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.listener
            .local_addr()
            .expect("a bound fixture listener has a local address")
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn dropped_event_count(&self) -> usize {
        self.dropped_event_count
    }

    pub fn try_handle(&mut self, request: Request) -> Result<WorkDisposition, FixtureError> {
        self.try_handle_at(request, Instant::now())
    }

    #[doc(hidden)]
    pub fn try_handle_at(
        &mut self,
        request: Request,
        now: Instant,
    ) -> Result<WorkDisposition, FixtureError> {
        let request_kind = match &request {
            Request::Readiness => "readiness",
            Request::Echo { .. } => "echo",
            Request::Stop => "stop",
        };
        self.record(EventChannel::Lifecycle, request_kind, None)?;
        if let Request::Echo { payload } = &request {
            if payload.len() > MAX_CANARY_BYTES {
                return Err(FixtureError::OversizedRequest);
            }
            self.record(EventChannel::RequestCanary, "observed", None)?;
        }
        if matches!(request, Request::Readiness) {
            let ready = !matches!(self.mode, Mode::DelayedReadiness { delay_ms } if now.saturating_duration_since(self.started) < Duration::from_millis(delay_ms));
            self.readiness_observed |= ready;
            self.record(
                EventChannel::Lifecycle,
                if ready { "ready" } else { "not_ready" },
                None,
            )?;
            let mut report = self.readiness.clone();
            report.state = if ready {
                ReadinessState::Ready
            } else {
                ReadinessState::NotReady
            };
            return Ok(WorkDisposition::Reply(Response::Readiness { report }));
        }
        if matches!(request, Request::Stop) {
            self.record(EventChannel::Lifecycle, "stopped", None)?;
            return Ok(WorkDisposition::Reply(Response::Stopped));
        }
        if self.readiness_observed {
            let disposition = match self.mode {
                Mode::PanicAfterReadiness => Some(WorkDisposition::Panic),
                Mode::CrashAfterReadiness => Some(WorkDisposition::Crash),
                Mode::Hang => Some(WorkDisposition::Hang),
                Mode::HangWithDescendant => Some(WorkDisposition::HangWithDescendant),
                _ => None,
            };
            if let Some(disposition) = disposition {
                let action = match disposition {
                    WorkDisposition::Panic => "panic",
                    WorkDisposition::Crash => "crash",
                    WorkDisposition::Hang => "hang",
                    WorkDisposition::HangWithDescendant => "hang-with-descendant",
                    WorkDisposition::Reply(_) => unreachable!(),
                };
                self.record(EventChannel::Lifecycle, action, None)?;
                return Ok(disposition);
            }
        }
        let response = match self.mode {
            Mode::TransientFailure { .. } if self.remaining_failures > 0 => {
                self.remaining_failures -= 1;
                Response::Failure {
                    class: FailureClass::Transient,
                    remaining: self.remaining_failures,
                }
            }
            Mode::PermanentFailure => Response::Failure {
                class: FailureClass::Permanent,
                remaining: 0,
            },
            _ => match request {
                Request::Echo { payload } => Response::Echo { payload },
                Request::Readiness | Request::Stop => unreachable!("handled above"),
            },
        };
        if matches!(response, Response::Failure { .. }) {
            self.record(EventChannel::Lifecycle, "failure", None)?;
        }
        Ok(WorkDisposition::Reply(response))
    }

    pub fn handle(&mut self, request: Request) -> WorkDisposition {
        self.try_handle(request)
            .expect("validated in-memory fixture request")
    }

    pub fn serve(self) -> Result<(), FixtureError> {
        self.serve_with(false)
    }

    pub fn serve_process(self) -> Result<(), FixtureError> {
        self.serve_with(true)
    }

    fn serve_with(mut self, abrupt_crash: bool) -> Result<(), FixtureError> {
        loop {
            let (stream, peer) = self.listener.accept()?;
            if !peer.ip().is_loopback() {
                return Err(FixtureError::NonLoopbackListener);
            }
            if self.serve_connection(stream, abrupt_crash)? {
                return Ok(());
            }
        }
    }

    fn serve_connection(
        &mut self,
        mut stream: TcpStream,
        abrupt_crash: bool,
    ) -> Result<bool, FixtureError> {
        stream.set_read_timeout(Some(self.read_timeout))?;
        stream.set_write_timeout(Some(self.write_timeout))?;
        let mut encoded = Vec::new();
        if let Err(error) = BufReader::new(stream.try_clone()?)
            .take((MAX_REQUEST_BYTES + 1) as u64)
            .read_until(b'\n', &mut encoded)
        {
            self.record(EventChannel::Lifecycle, "request_failed", None)?;
            return Err(FixtureError::Io(error));
        }
        if encoded.len() > MAX_REQUEST_BYTES || !encoded.ends_with(b"\n") {
            self.record(EventChannel::Lifecycle, "request_rejected", None)?;
            return Err(FixtureError::OversizedRequest);
        }
        let request: Request = match serde_json::from_slice(&encoded) {
            Ok(request) => request,
            Err(error) => {
                self.record(EventChannel::Lifecycle, "request_rejected", None)?;
                return Err(FixtureError::Json(error));
            }
        };
        let stop = matches!(request, Request::Stop);
        match self.try_handle(request)? {
            WorkDisposition::Reply(response) => {
                serde_json::to_writer(&mut stream, &response).map_err(FixtureError::Json)?;
                stream.write_all(b"\n")?;
                stream.flush()?;
                Ok(stop)
            }
            WorkDisposition::Panic => panic!("panic-after-readiness"),
            WorkDisposition::Crash if abrupt_crash => std::process::exit(CRASH_EXIT_CODE),
            WorkDisposition::Crash => panic!("crash disposition"),
            WorkDisposition::Hang => loop {
                thread::park();
            },
            WorkDisposition::HangWithDescendant => {
                let child =
                    std::process::Command::new(std::env::current_exe().map_err(FixtureError::Io)?)
                        .args(["--internal-child", "hang"])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                        .map_err(FixtureError::Io)?;
                self.record(
                    EventChannel::Lifecycle,
                    "descendant_spawned",
                    Some(child.id().to_string()),
                )?;
                loop {
                    thread::park();
                }
            }
        }
    }

    fn record(
        &mut self,
        channel: EventChannel,
        action: &str,
        value: Option<String>,
    ) -> Result<(), FixtureError> {
        let event = Event {
            schema: "fixture-event-v1".into(),
            channel,
            action: action.into(),
            value,
        };
        let encoded = event.to_json()?;
        if let Some(writer) = &self.event_writer {
            writer.write(encoded)?;
        }
        if self.events.len() == MAX_RECORDED_EVENTS {
            self.events.remove(0);
            self.dropped_event_count += 1;
        }
        self.events.push(event);
        Ok(())
    }
}

fn validate_config(config: &FixtureConfig) -> Result<(), FixtureError> {
    if !config.listen_addr.ip().is_loopback() {
        return Err(FixtureError::NonLoopbackListener);
    }
    if config.listen_addr.port() != 0 {
        return Err(FixtureError::NonDynamicPort);
    }
    config
        .readiness
        .validate()
        .map_err(|_| FixtureError::InvalidReadiness)?;
    match config.mode {
        Mode::DelayedReadiness { delay_ms } if delay_ms > MAX_DELAY_MS => {
            return Err(FixtureError::InvalidMode("delay exceeds bound"));
        }
        Mode::TransientFailure { failures } if failures > MAX_FAILURE_COUNT => {
            return Err(FixtureError::InvalidMode("failure count exceeds bound"));
        }
        _ => {}
    }
    for (name, value) in [
        ("argv", &config.canaries.argv),
        ("named_env", &config.canaries.named_env),
        ("config", &config.canaries.config),
        ("intentional_log", &config.canaries.intentional_log),
    ] {
        if value.len() > MAX_CANARY_BYTES {
            return Err(FixtureError::OversizedCanary(name));
        }
    }
    let max_deadline = Duration::from_secs(2);
    if config.read_timeout.is_zero()
        || config.write_timeout.is_zero()
        || config.read_timeout > max_deadline
        || config.write_timeout > max_deadline
    {
        return Err(FixtureError::InvalidDeadline);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        EventWriter, FixtureError, WriteRequest, EVENT_WRITER_QUEUE_CAPACITY, EVENT_WRITE_DEADLINE,
    };
    use std::io::{self, Write};
    use std::sync::mpsc;

    struct BlockingWriter {
        started: mpsc::SyncSender<()>,
        release: mpsc::Receiver<()>,
    }

    impl Write for BlockingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            let _ = self.started.try_send(());
            let _ = self.release.recv();
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn blocking_writer() -> (EventWriter, mpsc::Receiver<()>, mpsc::SyncSender<()>) {
        let (started, started_receiver) = mpsc::sync_channel(1);
        let (release, release_receiver) = mpsc::sync_channel(1);
        (
            EventWriter::spawn(Box::new(BlockingWriter {
                started,
                release: release_receiver,
            })),
            started_receiver,
            release,
        )
    }

    #[test]
    fn blocked_event_write_has_a_stable_deadline_error() {
        let (writer, started, release) = blocking_writer();
        let result = writer.write("event".into());
        assert!(matches!(result, Err(FixtureError::EventWriterDeadline)));
        started.recv_timeout(EVENT_WRITE_DEADLINE).unwrap();
        release.send(()).unwrap();
    }

    #[test]
    fn full_event_queue_has_a_stable_backpressure_error() {
        let (writer, started, release) = blocking_writer();
        let (acknowledgement, _) = mpsc::sync_channel(1);
        writer
            .requests
            .try_send(WriteRequest {
                encoded: "blocking".into(),
                acknowledgement,
            })
            .unwrap();
        started.recv_timeout(EVENT_WRITE_DEADLINE).unwrap();

        for _ in 0..EVENT_WRITER_QUEUE_CAPACITY {
            let (acknowledgement, _) = mpsc::sync_channel(1);
            writer
                .requests
                .try_send(WriteRequest {
                    encoded: "queued".into(),
                    acknowledgement,
                })
                .unwrap();
        }
        assert!(matches!(
            writer.write("overflow".into()),
            Err(FixtureError::EventWriterBackpressure)
        ));
        release.send(()).unwrap();
    }
}
