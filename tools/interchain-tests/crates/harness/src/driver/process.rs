//! Shell-free, direct-child process supervision.

use super::{DynamicEndpoint, EndpointError};
use crate::readiness::{await_ready, AwaitBudget, ExpectedReadiness, ReadinessError};
use nomic_harness_protocol::{
    CanonicalId, ReadinessIdentity, StartupEvent, MAX_STARTUP_EVENT_BYTES,
};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_PROGRAM_BYTES: usize = 4096;
const MAX_ARGS: usize = 64;
const MAX_ARG_BYTES: usize = 4096;
const MAX_TOTAL_ARG_BYTES: usize = 32 * 1024;
const MAX_ENV: usize = 64;
const MAX_ENV_BYTES: usize = 8192;
const MAX_TOTAL_ENV_BYTES: usize = 64 * 1024;
const MAX_STDIN_BYTES: usize = 8192;
const MAX_LATER_EVENT_BYTES: usize = 2048;
const EVENT_QUEUE_CAPACITY: usize = 64;

pub struct ProcessSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub stdin: Vec<u8>,
    pub component_id: CanonicalId,
    pub run_id: CanonicalId,
    pub network_id: CanonicalId,
    pub startup_deadline: Duration,
}

pub struct ProcessDriver {
    child: Option<Child>,
    endpoint: DynamicEndpoint,
    events: Receiver<Result<Vec<u8>, ProcessError>>,
    dropped_event_count: Arc<AtomicUsize>,
}

impl ProcessDriver {
    pub fn spawn(spec: ProcessSpec) -> Result<Self, ProcessError> {
        validate_spec(&spec)?;
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .env_clear()
            .envs(spec.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut owned = OwnedChild::new(command.spawn().map_err(ProcessError::Io)?);
        let startup_deadline = Instant::now()
            .checked_add(spec.startup_deadline)
            .ok_or(ProcessError::InvalidSpec)?;

        let stdout = owned
            .child_mut()
            .stdout
            .take()
            .ok_or(ProcessError::MissingPipe)?;
        let stderr = owned
            .child_mut()
            .stderr
            .take()
            .ok_or(ProcessError::MissingPipe)?;
        drain_stderr(stderr);
        write_stdin(
            owned
                .child_mut()
                .stdin
                .take()
                .ok_or(ProcessError::MissingPipe)?,
            spec.stdin,
            startup_remaining(startup_deadline)?,
        )?;

        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let (event_tx, event_rx) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let dropped_event_count = Arc::new(AtomicUsize::new(0));
        let reader_dropped_event_count = Arc::clone(&dropped_event_count);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let startup = read_bounded_line(&mut reader, MAX_STARTUP_EVENT_BYTES);
            if startup_tx.send(startup).is_err() {
                return;
            }
            loop {
                match read_bounded_line(&mut reader, MAX_LATER_EVENT_BYTES) {
                    Ok(line) if line.is_empty() => return,
                    result => {
                        let failed = result.is_err();
                        match event_tx.try_send(result) {
                            Ok(()) if failed => return,
                            Ok(()) => {}
                            Err(TrySendError::Full(_)) => {
                                reader_dropped_event_count.fetch_add(1, Ordering::Relaxed);
                                if failed {
                                    return;
                                }
                            }
                            Err(TrySendError::Disconnected(_)) => return,
                        }
                    }
                }
            }
        });
        let startup = startup_rx
            .recv_timeout(startup_remaining(startup_deadline)?)
            .map_err(|_| ProcessError::StartupDeadline)??;
        let event: StartupEvent = serde_json::from_slice(&startup).map_err(ProcessError::Json)?;
        let endpoint = DynamicEndpoint::from_startup_event(
            &event,
            &spec.component_id,
            &spec.run_id,
            &spec.network_id,
        )
        .map_err(ProcessError::Endpoint)?;
        Ok(Self {
            child: Some(owned.disarm()),
            endpoint,
            events: event_rx,
            dropped_event_count,
        })
    }

    pub fn endpoint(&self) -> DynamicEndpoint {
        self.endpoint
    }

    pub fn await_ready(
        &self,
        expected: &ExpectedReadiness,
        budget: AwaitBudget,
    ) -> Result<ReadinessIdentity, ReadinessError> {
        await_ready(self.endpoint, expected, budget)
    }

    pub fn next_event(&self, timeout: Duration) -> Result<Vec<u8>, ProcessError> {
        self.events
            .recv_timeout(timeout)
            .map_err(|_| ProcessError::EventDeadline)?
    }

    pub fn dropped_event_count(&self) -> usize {
        self.dropped_event_count.load(Ordering::Relaxed)
    }

    pub fn id(&self) -> u32 {
        self.child.as_ref().expect("driver owns child").id()
    }

    pub fn terminate(mut self) -> Result<ExitStatus, ProcessError> {
        terminate(self.child.take().expect("driver owns child")).map_err(ProcessError::Io)
    }
}

impl Drop for ProcessDriver {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            let _ = terminate(child);
        }
    }
}

struct OwnedChild(Option<Child>);
impl OwnedChild {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }
    fn child_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("owned child")
    }
    fn disarm(mut self) -> Child {
        self.0.take().expect("owned child")
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.take() {
            let _ = terminate(child);
        }
    }
}

fn terminate(mut child: Child) -> std::io::Result<ExitStatus> {
    match child.try_wait()? {
        Some(status) => Ok(status),
        None => {
            let _ = child.kill();
            child.wait()
        }
    }
}

fn validate_spec(spec: &ProcessSpec) -> Result<(), ProcessError> {
    let mut env_names = BTreeSet::new();
    let invalid_env = spec.env.iter().any(|(key, _)| {
        let Some(key) = key.to_str() else {
            return true;
        };
        !valid_env_name(key) || !env_names.insert(key)
    });
    if spec.program.is_empty()
        || spec.program.as_encoded_bytes().len() > MAX_PROGRAM_BYTES
        || spec.args.len() > MAX_ARGS
        || spec
            .args
            .iter()
            .any(|v| v.as_encoded_bytes().len() > MAX_ARG_BYTES)
        || spec
            .args
            .iter()
            .map(|v| v.as_encoded_bytes().len())
            .sum::<usize>()
            > MAX_TOTAL_ARG_BYTES
        || spec.env.len() > MAX_ENV
        || invalid_env
        || spec
            .env
            .iter()
            .any(|(k, v)| k.as_encoded_bytes().len() + v.as_encoded_bytes().len() > MAX_ENV_BYTES)
        || spec
            .env
            .iter()
            .map(|(k, v)| k.as_encoded_bytes().len() + v.as_encoded_bytes().len())
            .sum::<usize>()
            > MAX_TOTAL_ENV_BYTES
        || spec.stdin.len() > MAX_STDIN_BYTES
        || spec.startup_deadline.is_zero()
        || spec.startup_deadline > Duration::from_secs(10)
    {
        return Err(ProcessError::InvalidSpec);
    }
    Ok(())
}

fn valid_env_name(key: &str) -> bool {
    let mut bytes = key.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_uppercase() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn write_stdin(
    mut stdin: impl Write + Send + 'static,
    bytes: Vec<u8>,
    deadline: Duration,
) -> Result<(), ProcessError> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = tx.send(stdin.write_all(&bytes).and_then(|()| stdin.flush()));
    });
    rx.recv_timeout(deadline)
        .map_err(|_| ProcessError::StartupDeadline)?
        .map_err(ProcessError::Io)
}

fn startup_remaining(deadline: Instant) -> Result<Duration, ProcessError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(ProcessError::StartupDeadline)
}

fn drain_stderr(mut stderr: impl Read + Send + 'static) {
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
    });
}

fn read_bounded_line(reader: &mut impl BufRead, max: usize) -> Result<Vec<u8>, ProcessError> {
    let mut bytes = Vec::new();
    reader
        .take((max + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(ProcessError::Io)?;
    if bytes.len() > max {
        return Err(ProcessError::OversizedEvent);
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(ProcessError::MalformedEvent);
    }
    Ok(bytes)
}

#[derive(Debug)]
pub enum ProcessError {
    InvalidSpec,
    MissingPipe,
    StartupDeadline,
    EventDeadline,
    OversizedEvent,
    MalformedEvent,
    Io(std::io::Error),
    Json(serde_json::Error),
    Endpoint(EndpointError),
}
impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "process driver error: {self:?}")
    }
}
impl std::error::Error for ProcessError {}
