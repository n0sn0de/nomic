//! Bounded, identity-validating readiness probes.

use crate::driver::DynamicEndpoint;
use crate::retry::{run, RetryBudget, RetryClass, RetryFailure, RetryReceipt};
use nomic_harness_protocol::{
    CanonicalId, ReadinessIdentity, ReadinessState, Request, Response, READINESS_SCHEMA,
};
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: usize = 2 * 1024;
const MAX_ATTEMPTS: u16 = 64;
const MAX_DEADLINE: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct ExpectedReadiness {
    component_id: CanonicalId,
    run_id: CanonicalId,
    network_id: CanonicalId,
    capabilities: Vec<CanonicalId>,
}
impl ExpectedReadiness {
    pub fn new(
        component_id: CanonicalId,
        run_id: CanonicalId,
        network_id: CanonicalId,
        capabilities: impl IntoIterator<Item = CanonicalId>,
    ) -> Result<Self, ReadinessError> {
        let capabilities = capabilities.into_iter().collect::<Vec<_>>();
        if capabilities.len() > nomic_harness_protocol::MAX_CAPABILITIES
            || capabilities.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(ReadinessError::permanent(
                ReadinessErrorClass::InvalidCapabilities,
                0,
            ));
        }
        Ok(Self {
            component_id,
            run_id,
            network_id,
            capabilities,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AwaitBudget {
    max_attempts: u16,
    deadline: Duration,
    backoff: Duration,
}
impl AwaitBudget {
    pub fn new(
        max_attempts: u16,
        deadline: Duration,
        backoff: Duration,
    ) -> Result<Self, ReadinessError> {
        if max_attempts == 0
            || max_attempts > MAX_ATTEMPTS
            || deadline.is_zero()
            || deadline > MAX_DEADLINE
            || backoff.is_zero()
            || backoff >= deadline
        {
            return Err(ReadinessError::permanent(
                ReadinessErrorClass::InvalidBudget,
                0,
            ));
        }
        Ok(Self {
            max_attempts,
            deadline,
            backoff,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadinessErrorClass {
    DeadlineExceeded,
    AttemptsExhausted,
    NotReady,
    TransportUnavailable,
    WrongSchema,
    IdentityMismatch,
    CapabilityMismatch,
    InvalidCapabilities,
    MalformedResponse,
    OversizedResponse,
    UnexpectedResponse,
    InvalidBudget,
}

#[derive(Debug)]
pub struct ReadinessError {
    class: ReadinessErrorClass,
    attempts: u16,
    permanent: bool,
    receipt: Option<RetryReceipt>,
}
impl ReadinessError {
    fn permanent(class: ReadinessErrorClass, attempts: u16) -> Self {
        Self {
            class,
            attempts,
            permanent: true,
            receipt: None,
        }
    }
    pub fn class(&self) -> ReadinessErrorClass {
        self.class
    }
    pub fn attempts(&self) -> u16 {
        self.attempts
    }
    pub fn is_permanent(&self) -> bool {
        self.permanent
    }
    pub fn receipt(&self) -> Option<&RetryReceipt> {
        self.receipt.as_ref()
    }
}
impl fmt::Display for ReadinessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "readiness {:?} after {} attempt(s)",
            self.class, self.attempts
        )
    }
}
impl std::error::Error for ReadinessError {}

pub fn await_ready(
    endpoint: DynamicEndpoint,
    expected: &ExpectedReadiness,
    budget: AwaitBudget,
) -> Result<ReadinessIdentity, ReadinessError> {
    await_ready_with_receipt(endpoint, expected, budget).map(|success| success.identity)
}

#[derive(Debug)]
pub struct ReadyWithReceipt {
    pub identity: ReadinessIdentity,
    pub receipt: RetryReceipt,
}

pub fn await_ready_with_receipt(
    endpoint: DynamicEndpoint,
    expected: &ExpectedReadiness,
    budget: AwaitBudget,
) -> Result<ReadyWithReceipt, ReadinessError> {
    let retry_budget = RetryBudget::new(budget.max_attempts, budget.deadline, budget.backoff)
        .expect("readiness budget is already validated");
    let report = run(retry_budget, |attempt| {
        let number = attempt.number();
        probe(endpoint, expected, attempt.deadline(), number).map_err(|error| {
            if error.permanent {
                RetryClass::Permanent(error)
            } else {
                RetryClass::Transient(error)
            }
        })
    });
    let receipt = *report.receipt();
    match report.into_result() {
        Ok(identity) => Ok(ReadyWithReceipt { identity, receipt }),
        Err(RetryFailure::Permanent(mut error)) => {
            error.attempts = receipt.attempts();
            error.receipt = Some(receipt);
            Err(error)
        }
        Err(RetryFailure::DeadlineExceeded(_)) => Err(ReadinessError {
            class: ReadinessErrorClass::DeadlineExceeded,
            attempts: receipt.attempts(),
            permanent: false,
            receipt: Some(receipt),
        }),
        Err(RetryFailure::AttemptsExhausted(_)) => Err(ReadinessError {
            class: ReadinessErrorClass::AttemptsExhausted,
            attempts: receipt.attempts(),
            permanent: false,
            receipt: Some(receipt),
        }),
    }
}

fn probe(
    endpoint: DynamicEndpoint,
    expected: &ExpectedReadiness,
    deadline: Instant,
    attempt: u16,
) -> Result<ReadinessIdentity, ReadinessError> {
    let mut stream =
        TcpStream::connect_timeout(&endpoint.socket_addr(), remaining(deadline, attempt)?)
            .map_err(|_| ReadinessError {
                class: ReadinessErrorClass::TransportUnavailable,
                attempts: attempt,
                permanent: false,
                receipt: None,
            })?;
    let write_timeout = remaining(deadline, attempt)?;
    stream
        .set_write_timeout(Some(write_timeout))
        .map_err(|_| ReadinessError {
            class: ReadinessErrorClass::TransportUnavailable,
            attempts: attempt,
            permanent: false,
            receipt: None,
        })?;
    let mut request = serde_json::to_vec(&Request::Readiness)
        .map_err(|_| ReadinessError::permanent(ReadinessErrorClass::MalformedResponse, attempt))?;
    request.push(b'\n');
    stream.write_all(&request).map_err(|_| ReadinessError {
        class: ReadinessErrorClass::TransportUnavailable,
        attempts: attempt,
        permanent: false,
        receipt: None,
    })?;
    let read_timeout = remaining(deadline, attempt)?;
    stream
        .set_read_timeout(Some(read_timeout))
        .map_err(|_| ReadinessError {
            class: ReadinessErrorClass::TransportUnavailable,
            attempts: attempt,
            permanent: false,
            receipt: None,
        })?;
    let mut encoded = Vec::new();
    BufReader::new(stream)
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_until(b'\n', &mut encoded)
        .map_err(|_| ReadinessError {
            class: ReadinessErrorClass::TransportUnavailable,
            attempts: attempt,
            permanent: false,
            receipt: None,
        })?;
    if encoded.len() > MAX_RESPONSE_BYTES {
        return Err(ReadinessError::permanent(
            ReadinessErrorClass::OversizedResponse,
            attempt,
        ));
    }
    if !encoded.ends_with(b"\n") {
        return Err(ReadinessError::permanent(
            ReadinessErrorClass::MalformedResponse,
            attempt,
        ));
    }
    let response: Response = serde_json::from_slice(&encoded)
        .map_err(|_| ReadinessError::permanent(ReadinessErrorClass::MalformedResponse, attempt))?;
    let Response::Readiness { report } = response else {
        return Err(ReadinessError::permanent(
            ReadinessErrorClass::UnexpectedResponse,
            attempt,
        ));
    };
    report
        .validate()
        .map_err(|_| ReadinessError::permanent(ReadinessErrorClass::MalformedResponse, attempt))?;
    if report.schema != READINESS_SCHEMA {
        return Err(ReadinessError::permanent(
            ReadinessErrorClass::WrongSchema,
            attempt,
        ));
    }
    if report.component_id != expected.component_id
        || report.run_id != expected.run_id
        || report.network_id != expected.network_id
    {
        return Err(ReadinessError::permanent(
            ReadinessErrorClass::IdentityMismatch,
            attempt,
        ));
    }
    if report.capabilities != expected.capabilities {
        return Err(ReadinessError::permanent(
            ReadinessErrorClass::CapabilityMismatch,
            attempt,
        ));
    }
    if report.state == ReadinessState::NotReady {
        return Err(ReadinessError {
            class: ReadinessErrorClass::NotReady,
            attempts: attempt,
            permanent: false,
            receipt: None,
        });
    }
    Ok(report)
}

fn remaining(deadline: Instant, attempt: u16) -> Result<Duration, ReadinessError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(ReadinessError {
            class: ReadinessErrorClass::DeadlineExceeded,
            attempts: attempt,
            permanent: false,
            receipt: None,
        })
}
