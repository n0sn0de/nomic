//! Classified, bounded retries driven by one monotonic deadline.

use std::fmt;
use std::time::{Duration, Instant};

const MAX_ATTEMPTS: u16 = 1024;
const MAX_DEADLINE: Duration = Duration::from_secs(60 * 60);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug)]
pub struct RetryBudget {
    max_attempts: u16,
    deadline: Duration,
    backoff: Duration,
}

impl RetryBudget {
    pub fn new(
        max_attempts: u16,
        deadline: Duration,
        backoff: Duration,
    ) -> Result<Self, InvalidRetryBudget> {
        if max_attempts == 0
            || max_attempts > MAX_ATTEMPTS
            || deadline.is_zero()
            || deadline > MAX_DEADLINE
            || backoff.is_zero()
            || backoff > MAX_BACKOFF
            || backoff >= deadline
        {
            return Err(InvalidRetryBudget);
        }
        Ok(Self {
            max_attempts,
            deadline,
            backoff,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidRetryBudget;

impl fmt::Display for InvalidRetryBudget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("retry budget must be nonzero and bounded")
    }
}

impl std::error::Error for InvalidRetryBudget {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetryClass<E> {
    Transient(E),
    Permanent(E),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryOutcome {
    Succeeded,
    Permanent,
    DeadlineExceeded,
    AttemptsExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryReceipt {
    outcome: RetryOutcome,
    attempts: u16,
    elapsed: Duration,
}

impl RetryReceipt {
    pub fn outcome(&self) -> RetryOutcome {
        self.outcome
    }
    pub fn attempts(&self) -> u16 {
        self.attempts
    }
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
    pub fn elapsed_millis(&self) -> u128 {
        self.elapsed.as_millis()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RetryAttempt {
    number: u16,
    deadline: Instant,
}

impl RetryAttempt {
    pub fn number(self) -> u16 {
        self.number
    }
    pub fn remaining(self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    pub fn deadline(self) -> Instant {
        self.deadline
    }
}

#[derive(Debug)]
pub struct RetryReport<T, E> {
    result: Result<T, RetryFailure<E>>,
    receipt: RetryReceipt,
}

impl<T, E> RetryReport<T, E> {
    pub fn receipt(&self) -> &RetryReceipt {
        &self.receipt
    }
    pub fn into_result(self) -> Result<T, RetryFailure<E>> {
        self.result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetryFailure<E> {
    Permanent(E),
    DeadlineExceeded(Option<E>),
    AttemptsExhausted(E),
}

pub fn run<T, E>(
    budget: RetryBudget,
    mut operation: impl FnMut(RetryAttempt) -> Result<T, RetryClass<E>>,
) -> RetryReport<T, E> {
    let started = Instant::now();
    let deadline = started
        .checked_add(budget.deadline)
        .expect("bounded retry deadline");
    let mut attempts = 0;
    loop {
        if Instant::now() >= deadline {
            return report(
                Err(RetryFailure::DeadlineExceeded(None)),
                RetryOutcome::DeadlineExceeded,
                attempts,
                started,
            );
        }
        attempts += 1;
        let result = operation(RetryAttempt {
            number: attempts,
            deadline,
        });
        if Instant::now() >= deadline {
            let error = match result {
                Err(RetryClass::Transient(error) | RetryClass::Permanent(error)) => Some(error),
                Ok(_) => None,
            };
            return report(
                Err(RetryFailure::DeadlineExceeded(error)),
                RetryOutcome::DeadlineExceeded,
                attempts,
                started,
            );
        }
        match result {
            Ok(value) => {
                return report(Ok(value), RetryOutcome::Succeeded, attempts, started);
            }
            Err(RetryClass::Permanent(error)) => {
                return report(
                    Err(RetryFailure::Permanent(error)),
                    RetryOutcome::Permanent,
                    attempts,
                    started,
                );
            }
            Err(RetryClass::Transient(error)) if attempts == budget.max_attempts => {
                return report(
                    Err(RetryFailure::AttemptsExhausted(error)),
                    RetryOutcome::AttemptsExhausted,
                    attempts,
                    started,
                );
            }
            Err(RetryClass::Transient(_)) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                std::thread::sleep(budget.backoff.min(remaining));
            }
        }
    }
}

fn report<T, E>(
    result: Result<T, RetryFailure<E>>,
    outcome: RetryOutcome,
    attempts: u16,
    started: Instant,
) -> RetryReport<T, E> {
    RetryReport {
        result,
        receipt: RetryReceipt {
            outcome,
            attempts,
            elapsed: started.elapsed(),
        },
    }
}
