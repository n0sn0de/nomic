//! Shell-free, direct-child process supervision.

use super::{DynamicEndpoint, EndpointError};
use crate::readiness::protocol_request;
use crate::readiness::{await_ready, AwaitBudget, ExpectedReadiness, ReadinessError};
use nomic_harness_protocol::{
    CanonicalId, ReadinessIdentity, Request, Response, StartupEvent, MAX_STARTUP_EVENT_BYTES,
};
use std::collections::{BTreeSet, VecDeque};
use std::ffi::OsString;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
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
const REAP_DEADLINE: Duration = Duration::from_secs(1);
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(1);
const CLEANUP_SUPERVISOR_CAPACITY: usize = 8;

#[cfg(target_os = "linux")]
static SUBREAPER: OnceLock<Result<(), i32>> = OnceLock::new();
static CLEANUP_SUPERVISOR: OnceLock<CleanupSupervisor> = OnceLock::new();

pub struct ProcessSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub stdin: Vec<u8>,
    pub component_id: CanonicalId,
    pub run_id: CanonicalId,
    pub network_id: CanonicalId,
    pub startup_deadline: Duration,
    /// Absolute startup deadline. When present this is authoritative; the
    /// duration field remains only as a compatibility wrapper.
    pub startup_until: Option<Instant>,
    /// Required H0 containment contract. The launched program must not use
    /// daemonization, `setsid`, `setpgid`, PID namespaces, or otherwise create
    /// unreported descendants outside its inherited process group. Programs
    /// needing those features require a future stronger isolation backend.
    pub containment: ProcessContainment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessContainment {
    /// H0 fixture contract: every descendant remains reported and in the
    /// process group established by `ProcessDriver`.
    NoProcessGroupOrSessionEscape,
    /// Marker for workloads that may escape process-group containment. H0
    /// rejects this before reserving or spawning; a stronger backend is needed.
    StrongerIsolationRequired,
}

pub struct ProcessDriver {
    cleanup: Arc<SharedCleanup>,
    child_id: u32,
    endpoint: DynamicEndpoint,
    events: Receiver<Result<Vec<u8>, ProcessError>>,
    reader: Option<ReaderHandle>,
    dropped_event_count: Arc<AtomicUsize>,
}

impl ProcessDriver {
    pub fn spawn(spec: ProcessSpec) -> Result<Self, ProcessError> {
        validate_spec(&spec)?;
        let permit = cleanup_supervisor().reserve()?;
        enable_descendant_ownership()?;
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .env_clear()
            .envs(spec.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return Err(ProcessError::Io(error)),
        };
        let mut owned = OwnedChild::new(child, permit);
        let startup_deadline = match spec.startup_until {
            Some(deadline) => deadline,
            None => Instant::now()
                .checked_add(spec.startup_deadline)
                .ok_or(ProcessError::InvalidSpec)?,
        };

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
        let reader = ReaderHandle::spawn(stdout, startup_tx, event_tx, reader_dropped_event_count)?;
        let startup_result = (|| {
            let startup = startup_rx
                .recv_timeout(startup_remaining(startup_deadline)?)
                .map_err(|_| ProcessError::StartupDeadline)??;
            let event: StartupEvent =
                serde_json::from_slice(&startup).map_err(ProcessError::Json)?;
            DynamicEndpoint::from_startup_event(
                &event,
                &spec.component_id,
                &spec.run_id,
                &spec.network_id,
            )
            .map_err(ProcessError::Endpoint)
        })();
        let endpoint = match startup_result {
            Ok(endpoint) => endpoint,
            Err(original) => {
                let _ = cleanup_once(owned.cleanup.as_mut().expect("owned child cleanup state"));
                let mut reader = reader;
                if let Ok(deadline) = bounded_reap_deadline() {
                    let _ = reader.finish(deadline);
                }
                return Err(original);
            }
        };
        let (cleanup, permit) = owned.disarm();
        let child_id = cleanup.child_id().expect("new child cleanup state");
        Ok(Self {
            cleanup: Arc::new(SharedCleanup::new(cleanup, permit)),
            child_id,
            endpoint,
            events: event_rx,
            reader: Some(reader),
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

    /// Drains an event that is already buffered without extending a deadline.
    pub fn try_next_event(&self) -> Result<Option<Vec<u8>>, ProcessError> {
        match self.events.try_recv() {
            Ok(event) => event.map(Some),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Ok(None),
        }
    }

    pub fn dropped_event_count(&self) -> usize {
        self.dropped_event_count.load(Ordering::Relaxed)
    }

    pub fn id(&self) -> u32 {
        self.child_id
    }

    pub fn termination_handle(&self) -> ProcessTerminator {
        ProcessTerminator {
            cleanup: Arc::clone(&self.cleanup),
            child_id: self.child_id,
        }
    }

    /// Reports whether direct-child or exact-group cleanup remains pending.
    pub fn has_pending_cleanup(&self) -> Result<bool, ProcessError> {
        let guard = lock_recover(&self.cleanup.state);
        Ok(guard.has_pending_cleanup())
    }

    pub fn terminate(self) -> Result<ExitStatus, ProcessError> {
        let mut this = self;
        let status = terminate_shared(&this.cleanup)?.ok_or(ProcessError::AlreadyReaped)?;
        let deadline = bounded_reap_deadline()?;
        this.finish_reader_and_drain(deadline)?;
        Ok(status)
    }

    /// Requests typed graceful shutdown and waits for a clean exit using one deadline.
    pub fn stop_and_wait(self, timeout: Duration) -> Result<ExitStatus, ProcessError> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(ProcessError::GracefulDeadline)?;
        self.stop_and_wait_until(deadline)
    }

    pub fn stop_and_wait_until(self, deadline: Instant) -> Result<ExitStatus, ProcessError> {
        let mut this = self;
        match protocol_request(this.endpoint, &Request::Stop, deadline) {
            Ok(Response::Stopped) => {}
            Ok(_) => return Err(ProcessError::UnexpectedStopResponse),
            Err(_) => return Err(ProcessError::GracefulDeadline),
        }
        let mut guard = lock_recover(&this.cleanup.state);
        loop {
            let child = guard
                .direct_child
                .as_mut()
                .ok_or(ProcessError::AlreadyReaped)?;
            if let Some(status) = child.try_wait().map_err(ProcessError::Io)? {
                guard.direct_child = None;
                guard.direct_status = Some(status);
                break;
            }
            if Instant::now() >= deadline {
                return Err(ProcessError::GracefulDeadline);
            }
            std::thread::sleep(REAP_POLL_INTERVAL);
        }
        #[cfg(target_os = "linux")]
        if let Some(group) = guard.pending_process_group {
            reap_descendants_until(group, deadline)?;
            guard.pending_process_group = None;
        }
        let status = guard
            .direct_status
            .take()
            .ok_or(ProcessError::AlreadyReaped)?;
        lock_recover(&this.cleanup.permit).take();
        drop(guard);
        this.finish_reader_and_drain(deadline)?;
        if !status.success() {
            return Err(ProcessError::NonzeroExit);
        }
        Ok(status)
    }

    /// After the process has been reaped, waits for stdout EOF, joins the
    /// retained reader, and returns every event it published.
    pub fn finish_reader_and_drain(
        &mut self,
        deadline: Instant,
    ) -> Result<Vec<Vec<u8>>, ProcessError> {
        let mut reader = self.reader.take().ok_or(ProcessError::ReaderDisconnected)?;
        if let Err(error) = reader.finish(deadline) {
            self.reader = Some(reader);
            return Err(error);
        }
        let mut events = Vec::with_capacity(EVENT_QUEUE_CAPACITY);
        let mut first_error = None;
        for _ in 0..EVENT_QUEUE_CAPACITY {
            match self.events.try_recv() {
                Ok(Ok(event)) => events.push(event),
                Ok(Err(error)) => {
                    first_error.get_or_insert(error);
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
        }
        first_error.map_or(Ok(events), Err)
    }
}

impl Drop for ProcessDriver {
    fn drop(&mut self) {
        let _ = terminate_shared(&self.cleanup);
        if let Ok(deadline) = bounded_reap_deadline() {
            let _ = self.finish_reader_and_drain(deadline);
        }
    }
}

struct ReaderHandle {
    join: Option<JoinHandle<()>>,
    completed: Receiver<()>,
}

impl ReaderHandle {
    fn spawn(
        stdout: impl Read + Send + 'static,
        startup_tx: SyncSender<Result<Vec<u8>, ProcessError>>,
        event_tx: SyncSender<Result<Vec<u8>, ProcessError>>,
        dropped_event_count: Arc<AtomicUsize>,
    ) -> Result<Self, ProcessError> {
        Self::spawn_inner(stdout, startup_tx, event_tx, dropped_event_count, || {})
    }

    fn spawn_inner<B: FnMut() + Send + 'static>(
        stdout: impl Read + Send + 'static,
        startup_tx: SyncSender<Result<Vec<u8>, ProcessError>>,
        event_tx: SyncSender<Result<Vec<u8>, ProcessError>>,
        dropped_event_count: Arc<AtomicUsize>,
        mut before_event_handoff: B,
    ) -> Result<Self, ProcessError> {
        let (completed_tx, completed) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("process-stdout-reader".into())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut reader = BufReader::new(stdout);
                    let startup = read_bounded_line(&mut reader, MAX_STARTUP_EVENT_BYTES);
                    if startup_tx.send(startup).is_ok() {
                        loop {
                            match read_bounded_line(&mut reader, MAX_LATER_EVENT_BYTES) {
                                Ok(line) if line.is_empty() => break,
                                result => {
                                    before_event_handoff();
                                    let failed = result.is_err();
                                    match event_tx.try_send(result) {
                                        Ok(()) if failed => break,
                                        Ok(()) => {}
                                        Err(TrySendError::Full(_)) => {
                                            dropped_event_count.fetch_add(1, Ordering::Relaxed);
                                            if failed {
                                                break;
                                            }
                                        }
                                        Err(TrySendError::Disconnected(_)) => break,
                                    }
                                }
                            }
                        }
                    }
                }));
                let _ = completed_tx.send(());
                if let Err(payload) = outcome {
                    std::panic::resume_unwind(payload);
                }
            })
            .map_err(ProcessError::Io)?;
        Ok(Self {
            join: Some(join),
            completed,
        })
    }

    fn finish(&mut self, deadline: Instant) -> Result<(), ProcessError> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(ProcessError::ReaderDeadline)?;
        match self.completed.recv_timeout(remaining) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => return Err(ProcessError::ReaderDeadline),
            Err(RecvTimeoutError::Disconnected) => return Err(ProcessError::ReaderDisconnected),
        }
        self.join
            .take()
            .ok_or(ProcessError::ReaderDisconnected)?
            .join()
            .map_err(|_| ProcessError::ReaderPanic)
    }
}

fn bounded_reap_deadline() -> Result<Instant, ProcessError> {
    Instant::now()
        .checked_add(REAP_DEADLINE)
        .ok_or(ProcessError::ReaderDeadline)
}

#[derive(Clone)]
pub struct ProcessTerminator {
    cleanup: Arc<SharedCleanup>,
    child_id: u32,
}

impl ProcessTerminator {
    pub fn child_id(&self) -> u32 {
        self.child_id
    }

    pub fn terminate(&self) -> Result<Option<ExitStatus>, ProcessError> {
        terminate_shared(&self.cleanup)
    }
}

struct OwnedChild {
    cleanup: Option<CleanupState>,
    permit: Option<CleanupPermit>,
}
impl OwnedChild {
    fn new(child: Child, permit: CleanupPermit) -> Self {
        Self {
            cleanup: Some(CleanupState::new(child)),
            permit: Some(permit),
        }
    }
    fn child_mut(&mut self) -> &mut Child {
        self.cleanup
            .as_mut()
            .and_then(|state| state.direct_child.as_mut())
            .expect("owned child")
    }
    fn disarm(mut self) -> (CleanupState, CleanupPermit) {
        (
            self.cleanup.take().expect("owned child cleanup state"),
            self.permit.take().expect("owned cleanup permit"),
        )
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let Some(mut cleanup) = self.cleanup.take() else {
            return;
        };
        let permit = self.permit.take().expect("owned cleanup permit");
        if cleanup_once(&mut cleanup).is_err() && cleanup.has_pending_cleanup() {
            cleanup_supervisor().handoff(CleanupJob {
                cleanup,
                _permit: permit,
            });
        }
    }
}

struct SharedCleanup {
    state: Mutex<CleanupState>,
    permit: Mutex<Option<CleanupPermit>>,
}

impl SharedCleanup {
    fn new(state: CleanupState, permit: CleanupPermit) -> Self {
        Self {
            state: Mutex::new(state),
            permit: Mutex::new(Some(permit)),
        }
    }
}

impl Drop for SharedCleanup {
    fn drop(&mut self) {
        let mut state = lock_recover(&self.state);
        if !state.has_pending_cleanup() {
            return;
        }
        let cleanup = std::mem::replace(&mut *state, CleanupState::empty());
        drop(state);
        let permit = lock_recover(&self.permit)
            .take()
            .expect("pending cleanup owns permit");
        cleanup_supervisor().handoff(CleanupJob {
            cleanup,
            _permit: permit,
        });
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn terminate_shared(cleanup: &Arc<SharedCleanup>) -> Result<Option<ExitStatus>, ProcessError> {
    let mut guard = lock_recover(&cleanup.state);
    let result = cleanup_once(&mut guard);
    if result.is_ok() && !guard.has_pending_cleanup() {
        lock_recover(&cleanup.permit).take();
    }
    result
}

struct CleanupState {
    direct_child: Option<Child>,
    #[cfg(target_os = "linux")]
    pending_process_group: Option<i32>,
    direct_status: Option<ExitStatus>,
    group_kill_sent: bool,
}

impl CleanupState {
    fn empty() -> Self {
        Self {
            direct_child: None,
            #[cfg(target_os = "linux")]
            pending_process_group: None,
            direct_status: None,
            group_kill_sent: true,
        }
    }
    fn new(child: Child) -> Self {
        #[cfg(target_os = "linux")]
        let pending_process_group = i32::try_from(child.id()).ok();
        Self {
            direct_child: Some(child),
            #[cfg(target_os = "linux")]
            pending_process_group,
            direct_status: None,
            group_kill_sent: false,
        }
    }

    fn child_id(&self) -> Option<u32> {
        self.direct_child.as_ref().map(Child::id)
    }

    fn has_pending_cleanup(&self) -> bool {
        self.direct_child.is_some() || self.direct_status.is_some() || {
            #[cfg(target_os = "linux")]
            {
                self.pending_process_group.is_some()
            }
            #[cfg(not(target_os = "linux"))]
            {
                false
            }
        }
    }
}

fn cleanup_once(cleanup: &mut CleanupState) -> Result<Option<ExitStatus>, ProcessError> {
    if cleanup.direct_child.is_some() {
        send_kill_once(cleanup)?;
        reap_direct_child(cleanup)?;
    }
    #[cfg(target_os = "linux")]
    if let Some(process_group) = cleanup.pending_process_group {
        reap_descendants(process_group)?;
        cleanup.pending_process_group = None;
    }
    if cleanup.direct_child.is_none() {
        return Ok(cleanup.direct_status.take());
    }
    Ok(None)
}

fn send_kill_once(cleanup: &mut CleanupState) -> Result<(), ProcessError> {
    if cleanup.group_kill_sent {
        return Ok(());
    }
    #[cfg(unix)]
    {
        let process_group = process_group(
            cleanup
                .direct_child
                .as_ref()
                .expect("direct child checked by caller"),
        )?;
        kill_tree(process_group).map_err(ProcessError::Io)?;
    }
    #[cfg(not(unix))]
    {
        let child = cleanup
            .direct_child
            .as_mut()
            .expect("direct child checked by caller");
        if child.try_wait().map_err(ProcessError::Io)?.is_none() {
            child.kill().map_err(ProcessError::Io)?;
        }
    }
    cleanup.group_kill_sent = true;
    Ok(())
}

fn reap_direct_child(cleanup: &mut CleanupState) -> Result<(), ProcessError> {
    let deadline = Instant::now()
        .checked_add(REAP_DEADLINE)
        .ok_or(ProcessError::ReapDeadline)?;
    loop {
        let child = cleanup
            .direct_child
            .as_mut()
            .expect("direct child checked by caller");
        if let Some(status) = child.try_wait().map_err(ProcessError::Io)? {
            cleanup.direct_child = None;
            cleanup.direct_status = Some(status);
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ProcessError::ReapDeadline);
        }
        std::thread::sleep(REAP_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn process_group(child: &Child) -> Result<i32, ProcessError> {
    i32::try_from(child.id()).map_err(|_| ProcessError::InvalidChildId)
}

#[cfg(unix)]
fn kill_tree(process_group: i32) -> std::io::Result<()> {
    let result = unsafe { libc::kill(-process_group, libc::SIGKILL) };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn enable_descendant_ownership() -> Result<(), ProcessError> {
    let result = SUBREAPER.get_or_init(|| {
        if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } == -1 {
            Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(0))
        } else {
            Ok(())
        }
    });
    result.map_err(|errno| ProcessError::Subreaper(std::io::Error::from_raw_os_error(errno)))
}

#[cfg(not(target_os = "linux"))]
fn enable_descendant_ownership() -> Result<(), ProcessError> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn reap_descendants(process_group: i32) -> Result<(), ProcessError> {
    let deadline = Instant::now()
        .checked_add(REAP_DEADLINE)
        .ok_or(ProcessError::ReapDeadline)?;
    loop {
        let result = unsafe { libc::waitpid(-process_group, std::ptr::null_mut(), libc::WNOHANG) };
        if result > 0 {
            continue;
        }
        if result == -1 {
            let error = std::io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::ECHILD) => return Ok(()),
                Some(libc::EINTR) => continue,
                _ => return Err(ProcessError::UnresolvedWaitpid(error)),
            }
        }
        if Instant::now() >= deadline {
            return Err(ProcessError::ReapDeadline);
        }
        std::thread::sleep(REAP_POLL_INTERVAL);
    }
}

#[cfg(target_os = "linux")]
fn reap_descendants_until(process_group: i32, deadline: Instant) -> Result<(), ProcessError> {
    loop {
        let result = unsafe { libc::waitpid(-process_group, std::ptr::null_mut(), libc::WNOHANG) };
        if result > 0 {
            continue;
        }
        if result == -1 {
            let error = std::io::Error::last_os_error();
            return match error.raw_os_error() {
                Some(libc::ECHILD) => Ok(()),
                Some(libc::EINTR) => continue,
                _ => Err(ProcessError::UnresolvedWaitpid(error)),
            };
        }
        if Instant::now() >= deadline {
            return Err(ProcessError::GracefulDeadline);
        }
        std::thread::sleep(REAP_POLL_INTERVAL);
    }
}

struct CleanupPermit {
    active: Arc<AtomicUsize>,
}

impl Drop for CleanupPermit {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

struct CleanupJob {
    cleanup: CleanupState,
    _permit: CleanupPermit,
}

struct CleanupSupervisor {
    sender: SyncSender<CleanupJob>,
    active: Arc<AtomicUsize>,
    pending: Arc<AtomicUsize>,
    rejected: AtomicUsize,
    emergency: Arc<Mutex<VecDeque<CleanupJob>>>,
    unresolved: Arc<AtomicUsize>,
    _worker: JoinHandle<()>,
}

impl CleanupSupervisor {
    fn start() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<CleanupJob>(CLEANUP_SUPERVISOR_CAPACITY);
        let active = Arc::new(AtomicUsize::new(0));
        let pending = Arc::new(AtomicUsize::new(0));
        let emergency = Arc::new(Mutex::new(VecDeque::with_capacity(
            CLEANUP_SUPERVISOR_CAPACITY,
        )));
        let unresolved = Arc::new(AtomicUsize::new(0));
        let worker_pending = Arc::clone(&pending);
        let worker = std::thread::Builder::new()
            .name("process-cleanup-supervisor".into())
            .spawn(move || {
                let mut jobs = VecDeque::with_capacity(CLEANUP_SUPERVISOR_CAPACITY);
                loop {
                    if jobs.is_empty() {
                        match receiver.recv() {
                            Ok(job) => jobs.push_back(job),
                            Err(_) => return,
                        }
                    }
                    while let Ok(job) = receiver.try_recv() {
                        jobs.push_back(job);
                    }
                    let mut job = jobs.pop_front().expect("nonempty cleanup queue");
                    let _ = cleanup_once(&mut job.cleanup);
                    if job.cleanup.has_pending_cleanup() {
                        jobs.push_back(job);
                        std::thread::yield_now();
                    } else {
                        drop(job);
                        // Publish completion only after the permit has released its
                        // active slot; observing pending == 0 then guarantees the
                        // corresponding active-slot decrement is visible.
                        worker_pending.fetch_sub(1, Ordering::Release);
                    }
                }
            })
            .expect("process cleanup supervisor thread");
        Self {
            sender,
            active,
            pending,
            rejected: AtomicUsize::new(0),
            emergency,
            unresolved,
            _worker: worker,
        }
    }

    fn reserve(&self) -> Result<CleanupPermit, ProcessError> {
        let reserved = self
            .active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                (pending < CLEANUP_SUPERVISOR_CAPACITY).then_some(pending + 1)
            });
        if reserved.is_err() {
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(ProcessError::CleanupCapacity);
        }
        Ok(CleanupPermit {
            active: Arc::clone(&self.active),
        })
    }

    fn handoff(&self, job: CleanupJob) {
        self.pending.fetch_add(1, Ordering::AcqRel);
        match self.sender.try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Disconnected(job)) | Err(TrySendError::Full(job)) => {
                // A reserved permit proves a bounded slot exists. Preserve the
                // complete job even if the worker/channel invariant is broken.
                let mut emergency = lock_recover(&self.emergency);
                // Its reserved permit makes this store bounded to eight jobs.
                emergency.push_back(job);
                self.unresolved.fetch_add(1, Ordering::AcqRel);
            }
        }
    }
}

fn cleanup_supervisor() -> &'static CleanupSupervisor {
    CLEANUP_SUPERVISOR.get_or_init(CleanupSupervisor::start)
}

/// Stable number of startup-cleanup states owned by the process-lifetime supervisor.
pub fn pending_cleanup_count() -> usize {
    CLEANUP_SUPERVISOR
        .get()
        .map_or(0, |supervisor| supervisor.pending.load(Ordering::Acquire))
}

/// Stable number of jobs currently owned by the cleanup worker or its queue.
pub fn supervised_pending_cleanup_count() -> usize {
    pending_cleanup_count()
}

/// Number of reserved process-lifetime cleanup slots.
pub fn active_cleanup_slot_count() -> usize {
    CLEANUP_SUPERVISOR
        .get()
        .map_or(0, |s| s.active.load(Ordering::Acquire))
}

/// Jobs retained after an unexpected supervisor channel failure.
pub fn emergency_cleanup_count() -> usize {
    CLEANUP_SUPERVISOR
        .get()
        .map_or(0, |s| lock_recover(&s.emergency).len())
}

/// Typed unresolved cleanup incidents retained in the emergency store.
pub fn unresolved_cleanup_count() -> usize {
    CLEANUP_SUPERVISOR
        .get()
        .map_or(0, |s| s.unresolved.load(Ordering::Acquire))
}

/// Cumulative admissions refused because the fixed supervisor bound was unavailable.
pub fn rejected_cleanup_admission_count() -> usize {
    CLEANUP_SUPERVISOR
        .get()
        .map_or(0, |supervisor| supervisor.rejected.load(Ordering::Relaxed))
}

/// Cumulative pre-spawn cleanup-permit reservations rejected at capacity.
pub fn rejected_cleanup_reservation_count() -> usize {
    rejected_cleanup_admission_count()
}

#[cfg(test)]
mod reader_handle_tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn finish_join_then_drain_observes_delayed_event_handoff() {
        let input = Cursor::new(b"startup\nevent\n".to_vec());
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let (event_tx, event_rx) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let (arrived_tx, arrived_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let mut arrived_tx = Some(arrived_tx);
        let mut release_rx = Some(release_rx);
        let mut reader = ReaderHandle::spawn_inner(
            input,
            startup_tx,
            event_tx,
            Arc::new(AtomicUsize::new(0)),
            move || {
                if let Some(tx) = arrived_tx.take() {
                    tx.send(()).unwrap();
                    release_rx.take().unwrap().recv().unwrap();
                }
            },
        )
        .unwrap();

        assert_eq!(startup_rx.recv().unwrap().unwrap(), b"startup\n");
        arrived_rx.recv().unwrap();
        assert!(matches!(event_rx.try_recv(), Err(TryRecvError::Empty)));
        release_tx.send(()).unwrap();
        reader
            .finish(Instant::now() + Duration::from_secs(1))
            .unwrap();
        assert_eq!(event_rx.try_recv().unwrap().unwrap(), b"event\n");
        assert!(matches!(
            event_rx.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn reader_deadline_is_typed_and_handle_remains_joinable() {
        let (completed_tx, completed) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let join = std::thread::spawn(move || {
            release_rx.recv().unwrap();
            completed_tx.send(()).unwrap();
        });
        let mut reader = ReaderHandle {
            join: Some(join),
            completed,
        };
        assert!(matches!(
            reader.finish(Instant::now() + Duration::from_millis(1)),
            Err(ProcessError::ReaderDeadline)
        ));
        release_tx.send(()).unwrap();
        reader
            .finish(Instant::now() + Duration::from_secs(1))
            .unwrap();
    }

    #[test]
    fn reader_panic_is_typed() {
        let (completed_tx, completed) = mpsc::sync_channel(1);
        let join = std::thread::spawn(move || {
            completed_tx.send(()).unwrap();
            panic!("synthetic reader panic");
        });
        let mut reader = ReaderHandle {
            join: Some(join),
            completed,
        };
        assert!(matches!(
            reader.finish(Instant::now() + Duration::from_secs(1)),
            Err(ProcessError::ReaderPanic)
        ));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod cleanup_state_tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn live_group() -> i32 {
        let mut ready = [0; 2];
        assert_eq!(unsafe { libc::pipe(ready.as_mut_ptr()) }, 0);
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            unsafe {
                libc::close(ready[0]);
                libc::setpgid(0, 0);
                let byte = [1_u8];
                libc::write(ready[1], byte.as_ptr().cast(), byte.len());
                libc::close(ready[1]);
                loop {
                    libc::pause();
                }
            }
        }
        unsafe {
            libc::close(ready[1]);
            let mut byte = [0_u8];
            assert_eq!(
                libc::read(ready[0], byte.as_mut_ptr().cast(), byte.len()),
                1
            );
            libc::close(ready[0]);
        }
        pid
    }

    fn group_only_cleanup(process_group: i32, code: i32) -> CleanupState {
        CleanupState {
            direct_child: None,
            pending_process_group: Some(process_group),
            direct_status: Some(ExitStatus::from_raw(code << 8)),
            group_kill_sent: true,
        }
    }

    #[test]
    fn group_wait_retains_exact_pgid_and_saved_status_until_echild() {
        let _serial = lock_recover(&TEST_LOCK);
        enable_descendant_ownership().unwrap();
        let sentinel_group = live_group();
        let mut cleanup = group_only_cleanup(sentinel_group, 23);

        assert!(matches!(
            cleanup_once(&mut cleanup),
            Err(ProcessError::ReapDeadline)
        ));
        assert!(cleanup.direct_child.is_none());
        assert!(cleanup.direct_status.is_some());
        assert_eq!(cleanup.pending_process_group, Some(sentinel_group));

        kill_tree(sentinel_group).unwrap();
        let status = cleanup_once(&mut cleanup).unwrap().unwrap();
        assert_eq!(status.code(), Some(23));
        assert!(!cleanup.has_pending_cleanup());
    }

    #[test]
    fn completed_state_never_reuses_saved_process_group() {
        let _serial = lock_recover(&TEST_LOCK);
        let mut cleanup = group_only_cleanup(i32::MAX, 0);
        assert!(cleanup_once(&mut cleanup).unwrap().unwrap().success());
        assert!(cleanup_once(&mut cleanup).unwrap().is_none());
        assert!(!cleanup.has_pending_cleanup());
    }

    #[test]
    fn reservations_are_preallocated_and_fair_supervision_has_zero_leaks() {
        let _serial = lock_recover(&TEST_LOCK);
        enable_descendant_ownership().unwrap();
        let supervisor = cleanup_supervisor();
        let rejected_before = rejected_cleanup_admission_count();
        let permits = (0..CLEANUP_SUPERVISOR_CAPACITY)
            .map(|_| supervisor.reserve().unwrap())
            .collect::<Vec<_>>();
        let rejected_spec = ProcessSpec {
            program: OsString::from("program-must-never-be-spawned"),
            args: Vec::new(),
            env: Vec::new(),
            stdin: Vec::new(),
            component_id: CanonicalId::new("fixture").unwrap(),
            run_id: CanonicalId::new("run-a").unwrap(),
            network_id: CanonicalId::new("testnet").unwrap(),
            startup_deadline: Duration::from_secs(1),
            startup_until: None,
            containment: ProcessContainment::NoProcessGroupOrSessionEscape,
        };
        assert!(matches!(
            ProcessDriver::spawn(rejected_spec),
            Err(ProcessError::CleanupCapacity)
        ));
        assert_eq!(active_cleanup_slot_count(), CLEANUP_SUPERVISOR_CAPACITY);
        assert_eq!(rejected_cleanup_admission_count(), rejected_before + 1);
        drop(permits);

        let stuck_group = live_group();
        let stuck_permit = supervisor.reserve().unwrap();
        let clean_permit = supervisor.reserve().unwrap();
        supervisor.handoff(CleanupJob {
            cleanup: group_only_cleanup(stuck_group, 0),
            _permit: stuck_permit,
        });
        supervisor.handoff(CleanupJob {
            cleanup: group_only_cleanup(i32::MAX, 0),
            _permit: clean_permit,
        });
        let fair_deadline = Instant::now() + Duration::from_secs(3);
        while pending_cleanup_count() > 1 && Instant::now() < fair_deadline {
            std::thread::yield_now();
        }
        assert_eq!(
            pending_cleanup_count(),
            1,
            "stuck job starved cleanable job"
        );
        kill_tree(stuck_group).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while pending_cleanup_count() != 0 && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(
            pending_cleanup_count(),
            0,
            "supervisor leaked cleanup state"
        );
        assert_eq!(active_cleanup_slot_count(), 0, "cleanup permit leaked");
        assert_eq!(emergency_cleanup_count(), 0);
        assert_eq!(unresolved_cleanup_count(), 0);
    }

    #[test]
    fn final_shared_owner_hands_pending_state_and_reserved_permit_to_supervisor() {
        let _serial = lock_recover(&TEST_LOCK);
        enable_descendant_ownership().unwrap();
        let supervisor = cleanup_supervisor();
        let group = live_group();
        let shared = Arc::new(SharedCleanup::new(
            group_only_cleanup(group, 0),
            supervisor.reserve().unwrap(),
        ));
        let final_owner = Arc::clone(&shared);
        drop(shared);
        assert_eq!(pending_cleanup_count(), 0);
        drop(final_owner);
        let handoff_deadline = Instant::now() + Duration::from_secs(1);
        while pending_cleanup_count() == 0 && Instant::now() < handoff_deadline {
            std::thread::yield_now();
        }
        assert_eq!(pending_cleanup_count(), 1);
        kill_tree(group).unwrap();
        let cleanup_deadline = Instant::now() + Duration::from_secs(3);
        while pending_cleanup_count() != 0 && Instant::now() < cleanup_deadline {
            std::thread::yield_now();
        }
        assert_eq!(pending_cleanup_count(), 0);
        assert_eq!(active_cleanup_slot_count(), 0);
    }
}

fn validate_spec(spec: &ProcessSpec) -> Result<(), ProcessError> {
    if spec.containment != ProcessContainment::NoProcessGroupOrSessionEscape {
        return Err(ProcessError::UnsupportedContainment);
    }
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
        || (spec.startup_until.is_none()
            && (spec.startup_deadline.is_zero() || spec.startup_deadline > Duration::from_secs(10)))
        || spec.startup_until.is_some_and(|deadline| {
            let remaining = deadline.saturating_duration_since(Instant::now());
            remaining.is_zero() || remaining > Duration::from_secs(10)
        })
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
    ReaderDeadline,
    ReaderDisconnected,
    ReaderPanic,
    OversizedEvent,
    MalformedEvent,
    Io(std::io::Error),
    Json(serde_json::Error),
    Endpoint(EndpointError),
    SupervisorState,
    AlreadyReaped,
    InvalidChildId,
    ReapDeadline,
    Subreaper(std::io::Error),
    CleanupCapacity,
    UnsupportedContainment,
    UnresolvedWaitpid(std::io::Error),
    GracefulDeadline,
    UnexpectedStopResponse,
    NonzeroExit,
}
impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "process driver error: {self:?}")
    }
}
impl std::error::Error for ProcessError {}
