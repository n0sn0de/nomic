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
    deadline: Instant,
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
        let deadline = Instant::now()
            .checked_add(deadline)
            .ok_or_else(|| ReadinessError::permanent(ReadinessErrorClass::InvalidBudget, 0))?;
        Self::until(max_attempts, deadline, backoff)
    }

    pub fn until(
        max_attempts: u16,
        deadline: Instant,
        backoff: Duration,
    ) -> Result<Self, ReadinessError> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if max_attempts == 0
            || max_attempts > MAX_ATTEMPTS
            || remaining.is_zero()
            || remaining > MAX_DEADLINE
            || backoff.is_zero()
            || backoff >= remaining
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
    let retry_budget = RetryBudget::until(budget.max_attempts, budget.deadline, budget.backoff)
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
    let response = protocol_request(endpoint, &Request::Readiness, deadline).map_err(|error| {
        let class = match error {
            ProtocolRequestError::Deadline | ProtocolRequestError::Transport => {
                ReadinessErrorClass::TransportUnavailable
            }
            ProtocolRequestError::Oversized => ReadinessErrorClass::OversizedResponse,
            ProtocolRequestError::Malformed => ReadinessErrorClass::MalformedResponse,
        };
        ReadinessError {
            class,
            attempts: attempt,
            permanent: matches!(
                error,
                ProtocolRequestError::Oversized | ProtocolRequestError::Malformed
            ),
            receipt: None,
        }
    })?;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolRequestError {
    Deadline,
    Transport,
    Oversized,
    Malformed,
}

/// Sends one bounded request using the fixture's closed line protocol.
pub fn protocol_request(
    endpoint: DynamicEndpoint,
    request: &Request,
    deadline: Instant,
) -> Result<Response, ProtocolRequestError> {
    let remaining = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|value| !value.is_zero())
            .ok_or(ProtocolRequestError::Deadline)
    };
    let mut stream = TcpStream::connect_timeout(&endpoint.socket_addr(), remaining()?)
        .map_err(|_| ProtocolRequestError::Transport)?;
    stream
        .set_write_timeout(Some(remaining()?))
        .map_err(|_| ProtocolRequestError::Transport)?;
    let mut encoded = serde_json::to_vec(request).map_err(|_| ProtocolRequestError::Malformed)?;
    encoded.push(b'\n');
    stream
        .write_all(&encoded)
        .map_err(|_| ProtocolRequestError::Transport)?;
    stream
        .set_read_timeout(Some(remaining()?))
        .map_err(|_| ProtocolRequestError::Transport)?;
    let mut response = Vec::new();
    BufReader::new(stream)
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_until(b'\n', &mut response)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                ProtocolRequestError::Deadline
            }
            _ => ProtocolRequestError::Transport,
        })?;
    if response.len() > MAX_RESPONSE_BYTES {
        return Err(ProtocolRequestError::Oversized);
    }
    if !response.ends_with(b"\n") {
        return Err(ProtocolRequestError::Malformed);
    }
    serde_json::from_slice(&response).map_err(|_| ProtocolRequestError::Malformed)
}
