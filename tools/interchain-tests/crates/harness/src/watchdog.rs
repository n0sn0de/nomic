//! An outer deadline that can terminate an owned process independently of I/O.

use crate::driver::process::{ProcessError, ProcessTerminator};
use std::fmt;
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_DEADLINE: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchdogOutcome {
    Cancelled,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WatchdogReceipt {
    outcome: WatchdogOutcome,
    elapsed: Duration,
}

impl WatchdogReceipt {
    pub fn outcome(&self) -> WatchdogOutcome {
        self.outcome
    }
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
    pub fn elapsed_millis(&self) -> u128 {
        self.elapsed.as_millis()
    }
}

#[derive(Debug)]
pub enum WatchdogError {
    InvalidDeadline,
    Termination(ProcessError),
    WorkerPanicked,
}

impl fmt::Display for WatchdogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "watchdog error: {self:?}")
    }
}
impl std::error::Error for WatchdogError {}

pub struct WatchdogCancellation(SyncSender<()>);
impl WatchdogCancellation {
    pub fn cancel(self) -> Result<(), WatchdogError> {
        self.0.send(()).map_err(|_| WatchdogError::WorkerPanicked)
    }
}

pub struct ArmedWatchdog {
    cancellation: Option<WatchdogCancellation>,
    worker: Option<JoinHandle<Result<WatchdogReceipt, WatchdogError>>>,
}

impl ArmedWatchdog {
    pub fn arm(deadline: Duration, terminator: ProcessTerminator) -> Result<Self, WatchdogError> {
        if deadline.is_zero() || deadline > MAX_DEADLINE {
            return Err(WatchdogError::InvalidDeadline);
        }
        let started = Instant::now();
        let expires = started
            .checked_add(deadline)
            .ok_or(WatchdogError::InvalidDeadline)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || loop {
            let now = Instant::now();
            if now >= expires {
                terminator.terminate().map_err(WatchdogError::Termination)?;
                return Ok(WatchdogReceipt {
                    outcome: WatchdogOutcome::TimedOut,
                    elapsed: started.elapsed(),
                });
            }
            match receiver.recv_timeout(expires.duration_since(now)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if Instant::now() >= expires {
                        continue;
                    }
                    return Ok(WatchdogReceipt {
                        outcome: WatchdogOutcome::Cancelled,
                        elapsed: started.elapsed(),
                    });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        });
        Ok(Self {
            cancellation: Some(WatchdogCancellation(sender)),
            worker: Some(worker),
        })
    }

    pub fn cancel(mut self) -> Result<WatchdogReceipt, WatchdogError> {
        if let Some(cancellation) = self.cancellation.take() {
            let _ = cancellation.cancel();
        }
        self.join_worker()
    }

    pub fn wait(mut self) -> Result<WatchdogReceipt, WatchdogError> {
        let result = self.join_worker();
        self.cancellation.take();
        result
    }

    fn join_worker(&mut self) -> Result<WatchdogReceipt, WatchdogError> {
        self.worker
            .take()
            .expect("armed watchdog worker")
            .join()
            .map_err(|_| WatchdogError::WorkerPanicked)?
    }
}

impl Drop for ArmedWatchdog {
    fn drop(&mut self) {
        if let Some(cancellation) = self.cancellation.take() {
            let _ = cancellation.cancel();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
