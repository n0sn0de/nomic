//! Process-backed H0 infrastructure scenarios.

use crate::contract::{Backend, Fidelity, ScenarioContract, ScenarioOutcome, ScenarioStatus};
use crate::driver::process::{ProcessContainment, ProcessDriver, ProcessError, ProcessSpec};
use crate::identity::{
    IdentityError, NormalizedSemanticReceipt, ResourceLabel, RunIdentity, RunNonce, SemanticSeed,
    SemanticSeedInput,
};
use crate::readiness::{protocol_request, AwaitBudget, ExpectedReadiness, ProtocolRequestError};
use crate::retry::{run, InvalidRetryBudget, RetryBudget, RetryClass, RetryOutcome};
use crate::topology::{Topology, TopologyError};
use crate::watchdog::{ArmedWatchdog, WatchdogError, WatchdogOutcome};
use nomic_harness_protocol::{CanonicalId, FailureClass, Request, Response};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt;
use std::time::{Duration, Instant};

const FIXTURE_CANARY_ENV: &str = "NOMIC_FIXTURE_SYNTHETIC_CANARY";
const MAX_CONTROL_DURATION: Duration = Duration::from_secs(10);

pub struct H0RunnerRequest<'a> {
    pub contract: &'a ScenarioContract,
    pub topology: &'a Topology,
    pub fixture_executable: OsString,
    pub source_lock_digest: &'a str,
    pub run_nonces: [RunNonce; 2],
    pub network_id: CanonicalId,
    pub corpus_index: u64,
    pub controls: H0Controls,
}

#[derive(Clone, Copy, Debug)]
pub struct H0Controls {
    pub readiness_attempts: u16,
    pub retry_attempts: u16,
    pub retry_backoff: Duration,
    pub forced_hang_timeout: Duration,
    pub transient_failures: u16,
    /// Deterministic fail-closed startup injection used by lifecycle tests.
    pub startup_failure_node: Option<usize>,
}

impl H0Controls {
    pub fn bounded_test_defaults() -> Self {
        Self {
            readiness_attempts: 16,
            retry_attempts: 4,
            retry_backoff: Duration::from_millis(5),
            forced_hang_timeout: Duration::from_millis(80),
            transient_failures: 2,
            startup_failure_node: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum H0Receipt {
    Har001(BootstrapReceipt),
    Har005(ReplayReceipt),
    Har006(SupervisionOutput),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BootstrapReceipt {
    pub scenario_id: String,
    pub startup_order: Vec<String>,
    pub shutdown_order: Vec<String>,
    pub outcome: ScenarioOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReplayReceipt {
    pub first: NormalizedSemanticReceipt,
    pub second: NormalizedSemanticReceipt,
    pub resource_labels_differ: bool,
    pub equal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SupervisionReceipt {
    pub retry_attempts: u16,
    pub retry_outcome: RetryOutcome,
    pub watchdog_outcome: WatchdogOutcome,
    pub cleanup_restored: bool,
    pub outcome: ScenarioOutcome,
}

#[derive(Clone, Debug, Serialize)]
pub struct SupervisionOutput {
    pub receipt: SupervisionReceipt,
    #[serde(skip)]
    pub diagnostics: SupervisionDiagnostics,
}

impl PartialEq for SupervisionOutput {
    fn eq(&self, other: &Self) -> bool {
        self.receipt == other.receipt
    }
}
impl Eq for SupervisionOutput {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupervisionDiagnostics {
    pub retry_elapsed: Duration,
    pub watchdog_elapsed: Duration,
}

#[derive(Debug)]
pub enum H0Error {
    UnsupportedScenario,
    DisabledScenario,
    WrongBinding,
    WrongBackend,
    WrongFidelity,
    TopologyMismatch,
    InvalidTopology(TopologyError),
    InvalidControl,
    SameRunNonce,
    Identity(IdentityError),
    InvalidProtocolId,
    FixtureConfig,
    Process(ProcessError),
    Readiness,
    Protocol(ProtocolRequestError),
    UnexpectedResponse,
    RetryBudget(InvalidRetryBudget),
    RetryFailed,
    ReplayMismatch,
    Watchdog(WatchdogError),
    CleanupLeak,
    OverallDeadline,
}

impl fmt::Display for H0Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "H0 scenario error: {self:?}")
    }
}
impl std::error::Error for H0Error {}

pub fn run_h0(request: H0RunnerRequest<'_>) -> Result<H0Receipt, H0Error> {
    validate(&request)?;
    match request.contract.id.as_str() {
        "HAR-001" => run_har_001(&request).map(H0Receipt::Har001),
        "HAR-005" => run_har_005(&request).map(H0Receipt::Har005),
        "HAR-006" => run_har_006(&request).map(H0Receipt::Har006),
        _ => Err(H0Error::UnsupportedScenario),
    }
}

fn validate(request: &H0RunnerRequest<'_>) -> Result<(), H0Error> {
    let expected_binding = match request.contract.id.as_str() {
        "HAR-001" => "h0/v1::har_001",
        "HAR-005" => "h0/v1::har_005",
        "HAR-006" => "h0/v1::har_006",
        _ => return Err(H0Error::UnsupportedScenario),
    };
    if request.contract.status != ScenarioStatus::Enabled {
        return Err(H0Error::DisabledScenario);
    }
    if request
        .contract
        .executable
        .as_ref()
        .map(|value| value.as_str())
        != Some(expected_binding)
    {
        return Err(H0Error::WrongBinding);
    }
    if request.contract.backend != Backend::Process {
        return Err(H0Error::WrongBackend);
    }
    if request.contract.fidelity != Fidelity::Synthetic {
        return Err(H0Error::WrongFidelity);
    }
    if request.contract.topology != format!("topology/{}.toml", request.topology.id().as_str()) {
        return Err(H0Error::TopologyMismatch);
    }
    request
        .topology
        .startup_order()
        .map_err(H0Error::InvalidTopology)?;
    if request.run_nonces[0] == request.run_nonces[1] {
        return Err(H0Error::SameRunNonce);
    }
    if request.controls.readiness_attempts == 0
        || request.controls.retry_attempts == 0
        || request.controls.retry_backoff.is_zero()
        || request.controls.forced_hang_timeout.is_zero()
        || request.controls.retry_backoff > MAX_CONTROL_DURATION
        || request.controls.forced_hang_timeout > MAX_CONTROL_DURATION
        || request.controls.forced_hang_timeout
            > Duration::from_millis(request.contract.operation_deadline_ms)
        || request.controls.forced_hang_timeout
            > Duration::from_millis(request.contract.overall_deadline_ms)
        || request.controls.transient_failures > 8
    {
        return Err(H0Error::InvalidControl);
    }
    Ok(())
}

fn run_har_001(request: &H0RunnerRequest<'_>) -> Result<BootstrapReceipt, H0Error> {
    let deadline = overall_deadline(request)?;
    let seed = semantic_seed(request)?;
    let identity = RunIdentity::new(seed, request.run_nonces[0]);
    let run_id = canonical_label(
        identity
            .resource_label("h0-run")
            .map_err(H0Error::Identity)?,
    )?;
    let capabilities = capabilities(request)?;
    let order = request
        .topology
        .startup_order()
        .map_err(H0Error::InvalidTopology)?;
    let mut owner = ProcessOwner::new(request, deadline);
    let work = (|| {
        for (index, node) in order.iter().enumerate() {
            ensure_before(deadline)?;
            let driver = spawn_fixture(
                request,
                node,
                &run_id,
                &capabilities,
                "healthy",
                deadline,
                request.controls.startup_failure_node == Some(index),
            )?;
            owner.own(driver);
            ready(
                request,
                owner.last(),
                node,
                &run_id,
                &capabilities,
                deadline,
            )?;
        }
        Ok(())
    })();
    let startup_order = order.iter().map(|node| node.as_str().to_owned()).collect();
    let shutdown_order = order
        .iter()
        .rev()
        .map(|node| node.as_str().to_owned())
        .collect();
    owner.finish(work)?;
    Ok(BootstrapReceipt {
        scenario_id: request.contract.id.clone(),
        startup_order,
        shutdown_order,
        outcome: ScenarioOutcome::Pass,
    })
}

fn run_har_005(request: &H0RunnerRequest<'_>) -> Result<ReplayReceipt, H0Error> {
    let deadline = overall_deadline(request)?;
    let seed = semantic_seed(request)?;
    let first_identity = RunIdentity::new(seed, request.run_nonces[0]);
    let second_identity = RunIdentity::new(seed, request.run_nonces[1]);
    let first_resource_label = first_identity
        .resource_label("h0-replay")
        .map_err(H0Error::Identity)?;
    let second_resource_label = second_identity
        .resource_label("h0-replay")
        .map_err(H0Error::Identity)?;
    if first_resource_label == second_resource_label {
        return Err(H0Error::SameRunNonce);
    }
    let first = replay_once(request, seed, &first_resource_label, deadline)?;
    let second = replay_once(request, seed, &second_resource_label, deadline)?;
    let equal = first == second;
    if !equal {
        return Err(H0Error::ReplayMismatch);
    }
    Ok(ReplayReceipt {
        first,
        second,
        resource_labels_differ: true,
        equal,
    })
}

fn replay_once(
    request: &H0RunnerRequest<'_>,
    seed: SemanticSeed,
    label: &ResourceLabel,
    deadline: Instant,
) -> Result<NormalizedSemanticReceipt, H0Error> {
    let node = only_node(request)?;
    let run_id = canonical_label(label.clone())?;
    let capabilities = capabilities(request)?;
    let mut owner = ProcessOwner::new(request, deadline);
    let result = (|| {
        let driver = spawn_fixture(
            request,
            node,
            &run_id,
            &capabilities,
            "healthy",
            deadline,
            false,
        )?;
        owner.own(driver);
        ready(
            request,
            owner.last(),
            node,
            &run_id,
            &capabilities,
            deadline,
        )?;
        let payload = seed.to_hex();
        match protocol_request(
            owner.last().endpoint(),
            &Request::Echo {
                payload: payload.clone(),
            },
            operation_deadline(request, deadline)?,
        )
        .map_err(H0Error::Protocol)?
        {
            Response::Echo { payload: echoed } if echoed == payload => {}
            _ => return Err(H0Error::UnexpectedResponse),
        }
        NormalizedSemanticReceipt::new(
            seed,
            &request.contract.id,
            &request.contract.case_id,
            ScenarioOutcome::Pass,
        )
        .map_err(H0Error::Identity)
    })();
    owner.finish(result)
}

fn run_har_006(request: &H0RunnerRequest<'_>) -> Result<SupervisionOutput, H0Error> {
    let deadline = overall_deadline(request)?;
    let seed = semantic_seed(request)?;
    let capabilities = capabilities(request)?;
    let node = only_node(request)?;
    let retry_label = RunIdentity::new(seed, request.run_nonces[0])
        .resource_label("h0-retry")
        .map_err(H0Error::Identity)?;
    let retry_run = canonical_label(retry_label)?;
    let mode = format!("transient-failure:{}", request.controls.transient_failures);
    let mut owner = ProcessOwner::new(request, deadline);
    let result = (|| {
        let driver = spawn_fixture(
            request,
            node,
            &retry_run,
            &capabilities,
            &mode,
            deadline,
            false,
        )?;
        owner.own(driver);
        ready(
            request,
            owner.last(),
            node,
            &retry_run,
            &capabilities,
            deadline,
        )?;
        let retry_deadline = operation_deadline(request, deadline)?;
        let budget = RetryBudget::until(
            request.controls.retry_attempts,
            retry_deadline,
            request.controls.retry_backoff,
        )
        .map_err(H0Error::RetryBudget)?;
        let report = run(budget, |attempt| {
            match protocol_request(
                owner.last().endpoint(),
                &Request::Echo {
                    payload: "classified".into(),
                },
                attempt.deadline(),
            ) {
                Ok(Response::Echo { payload }) if payload == "classified" => Ok(()),
                Ok(Response::Failure {
                    class: FailureClass::Transient,
                    ..
                }) => Err(RetryClass::Transient(H0Error::RetryFailed)),
                Ok(Response::Failure {
                    class: FailureClass::Permanent,
                    ..
                }) => Err(RetryClass::Permanent(H0Error::RetryFailed)),
                Ok(_) => Err(RetryClass::Permanent(H0Error::UnexpectedResponse)),
                Err(error) => Err(RetryClass::Permanent(H0Error::Protocol(error))),
            }
        });
        let retry_receipt = *report.receipt();
        report.into_result().map_err(|_| H0Error::RetryFailed)?;
        if retry_receipt.attempts() != request.controls.transient_failures + 1
            || retry_receipt.outcome() != RetryOutcome::Succeeded
        {
            return Err(H0Error::RetryFailed);
        }
        owner.stop_last()?;

        let hang_label = RunIdentity::new(seed, request.run_nonces[1])
            .resource_label("h0-hang")
            .map_err(H0Error::Identity)?;
        let hang_run = canonical_label(hang_label)?;
        let hang = spawn_fixture(
            request,
            node,
            &hang_run,
            &capabilities,
            "hang-with-descendant",
            deadline,
            false,
        )?;
        owner.own(hang);
        ready(
            request,
            owner.last(),
            node,
            &hang_run,
            &capabilities,
            deadline,
        )?;
        let hang_started = Instant::now();
        let hang_deadline = hang_started
            .checked_add(request.controls.forced_hang_timeout)
            .ok_or(H0Error::OverallDeadline)?;
        if hang_deadline > operation_deadline_at(request, deadline, hang_started)? {
            return Err(H0Error::InvalidControl);
        }
        let watchdog = ArmedWatchdog::until(hang_deadline, owner.last().termination_handle())
            .map_err(H0Error::Watchdog)?;
        let blocked_result = protocol_request(
            owner.last().endpoint(),
            &Request::Echo {
                payload: "hang".into(),
            },
            hang_deadline,
        );
        let watchdog_receipt = watchdog.wait().map_err(H0Error::Watchdog)?;
        if watchdog_receipt.outcome() != WatchdogOutcome::TimedOut {
            return Err(H0Error::RetryFailed);
        }
        if blocked_result.is_ok() {
            return Err(H0Error::UnexpectedResponse);
        }
        if owner
            .last()
            .has_pending_cleanup()
            .map_err(H0Error::Process)?
        {
            return Err(H0Error::CleanupLeak);
        }
        let collection_deadline = operation_deadline(request, deadline)?;
        let events = owner
            .last_mut()
            .finish_reader_and_drain(collection_deadline)
            .map_err(H0Error::Process)?;
        observe_descendant(&events)?;
        Ok(SupervisionOutput {
            receipt: SupervisionReceipt {
                retry_attempts: retry_receipt.attempts(),
                retry_outcome: retry_receipt.outcome(),
                watchdog_outcome: watchdog_receipt.outcome(),
                cleanup_restored: true,
                outcome: ScenarioOutcome::Pass,
            },
            diagnostics: SupervisionDiagnostics {
                retry_elapsed: retry_receipt.elapsed(),
                watchdog_elapsed: watchdog_receipt.elapsed(),
            },
        })
    })();
    owner.finish(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureDiagnostic {
    schema: String,
    channel: String,
    action: String,
    #[serde(default)]
    value: Option<String>,
}

fn observe_descendant(events: &[Vec<u8>]) -> Result<(), H0Error> {
    for encoded in events {
        let event: FixtureDiagnostic =
            serde_json::from_slice(encoded).map_err(|_| H0Error::UnexpectedResponse)?;
        if event.schema != "fixture-event-v1" || event.channel != "lifecycle" {
            continue;
        }
        if event.action == "descendant_spawned" {
            let value = event.value.ok_or(H0Error::UnexpectedResponse)?;
            value
                .parse::<u32>()
                .map_err(|_| H0Error::UnexpectedResponse)?;
            return Ok(());
        }
    }
    Err(H0Error::UnexpectedResponse)
}

#[derive(Serialize)]
struct FixtureStdin<'a> {
    config_canary: &'a str,
    component_id: &'a CanonicalId,
    run_id: &'a CanonicalId,
    network_id: &'a CanonicalId,
    capabilities: &'a [CanonicalId],
}

fn spawn_fixture(
    request: &H0RunnerRequest<'_>,
    node: &CanonicalId,
    run_id: &CanonicalId,
    capabilities: &[CanonicalId],
    mode: &str,
    deadline: Instant,
    force_startup_failure: bool,
) -> Result<ProcessDriver, H0Error> {
    let stdin = serde_json::to_vec(&FixtureStdin {
        config_canary: "h0-config",
        component_id: node,
        run_id,
        network_id: &request.network_id,
        capabilities,
    })
    .map_err(|_| H0Error::FixtureConfig)?;
    let spec = ProcessSpec {
        program: if force_startup_failure {
            OsString::new()
        } else {
            request.fixture_executable.clone()
        },
        args: [
            "--listen",
            "127.0.0.1:0",
            "--mode",
            mode,
            "--argv-canary",
            "h0-argv",
            "--log-canary",
            "h0-log",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        env: vec![(OsString::from(FIXTURE_CANARY_ENV), OsString::from("h0-env"))],
        stdin,
        component_id: node.clone(),
        run_id: run_id.clone(),
        network_id: request.network_id.clone(),
        startup_deadline: Duration::from_millis(request.contract.operation_deadline_ms),
        startup_until: Some(operation_deadline(request, deadline)?),
        containment: ProcessContainment::NoProcessGroupOrSessionEscape,
    };
    ProcessDriver::spawn(spec).map_err(H0Error::Process)
}

fn ready(
    request: &H0RunnerRequest<'_>,
    driver: &ProcessDriver,
    node: &CanonicalId,
    run_id: &CanonicalId,
    capabilities: &[CanonicalId],
    deadline: Instant,
) -> Result<(), H0Error> {
    let expected = ExpectedReadiness::new(
        node.clone(),
        run_id.clone(),
        request.network_id.clone(),
        capabilities.iter().cloned(),
    )
    .map_err(|_| H0Error::Readiness)?;
    let operation_deadline = operation_deadline(request, deadline)?;
    let remaining = operation_deadline.saturating_duration_since(Instant::now());
    let backoff = request.controls.retry_backoff.min(remaining / 2);
    let budget = AwaitBudget::until(
        request.controls.readiness_attempts,
        operation_deadline,
        backoff,
    )
    .map_err(|_| H0Error::Readiness)?;
    driver
        .await_ready(&expected, budget)
        .map_err(|_| H0Error::Readiness)?;
    Ok(())
}

fn semantic_seed(request: &H0RunnerRequest<'_>) -> Result<SemanticSeed, H0Error> {
    SemanticSeed::derive(SemanticSeedInput {
        source_lock_digest: request.source_lock_digest,
        semantic_seed_domain: &request.contract.semantic_seed_domain,
        scenario_id: &request.contract.id,
        case_id: &request.contract.case_id,
        spec_version: &request.contract.spec_version,
        topology_id: request.topology.id().as_str(),
        corpus_index: request.corpus_index,
    })
    .map_err(H0Error::Identity)
}

fn capabilities(request: &H0RunnerRequest<'_>) -> Result<Vec<CanonicalId>, H0Error> {
    let mut values = request
        .contract
        .capabilities
        .iter()
        .map(|value| CanonicalId::new(value.clone()).map_err(|_| H0Error::InvalidProtocolId))
        .collect::<Result<Vec<_>, _>>()?;
    values.sort();
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(H0Error::InvalidProtocolId);
    }
    Ok(values)
}

fn canonical_label(label: ResourceLabel) -> Result<CanonicalId, H0Error> {
    CanonicalId::new(label.as_str()).map_err(|_| H0Error::InvalidProtocolId)
}
fn only_node<'a>(request: &'a H0RunnerRequest<'_>) -> Result<&'a CanonicalId, H0Error> {
    let order = request
        .topology
        .startup_order()
        .map_err(H0Error::InvalidTopology)?;
    if order.len() != 1 {
        return Err(H0Error::TopologyMismatch);
    }
    Ok(order[0])
}
fn overall_deadline(request: &H0RunnerRequest<'_>) -> Result<Instant, H0Error> {
    Instant::now()
        .checked_add(Duration::from_millis(request.contract.overall_deadline_ms))
        .ok_or(H0Error::OverallDeadline)
}
fn operation_deadline_at(
    request: &H0RunnerRequest<'_>,
    overall: Instant,
    started: Instant,
) -> Result<Instant, H0Error> {
    ensure_before(overall)?;
    let operation = started
        .checked_add(Duration::from_millis(
            request.contract.operation_deadline_ms,
        ))
        .ok_or(H0Error::OverallDeadline)?;
    Ok(operation.min(overall))
}
fn operation_deadline(request: &H0RunnerRequest<'_>, overall: Instant) -> Result<Instant, H0Error> {
    operation_deadline_at(request, overall, Instant::now())
}
fn ensure_before(deadline: Instant) -> Result<(), H0Error> {
    (Instant::now() < deadline)
        .then_some(())
        .ok_or(H0Error::OverallDeadline)
}
struct ProcessOwner<'a> {
    request: &'a H0RunnerRequest<'a>,
    overall_deadline: Instant,
    drivers: Vec<ProcessDriver>,
}

impl<'a> ProcessOwner<'a> {
    fn new(request: &'a H0RunnerRequest<'a>, overall_deadline: Instant) -> Self {
        Self {
            request,
            overall_deadline,
            drivers: Vec::new(),
        }
    }

    fn own(&mut self, driver: ProcessDriver) {
        self.drivers.push(driver);
    }

    fn last(&self) -> &ProcessDriver {
        self.drivers.last().expect("owned process")
    }

    fn last_mut(&mut self) -> &mut ProcessDriver {
        self.drivers.last_mut().expect("owned process")
    }

    fn stop_last(&mut self) -> Result<(), H0Error> {
        let driver = self.drivers.pop().expect("owned process");
        let deadline = operation_deadline(self.request, self.overall_deadline)?;
        driver
            .stop_and_wait_until(deadline)
            .map_err(H0Error::Process)?;
        Ok(())
    }

    fn cleanup(&mut self) -> Result<(), H0Error> {
        let mut first_error = None;
        while let Some(driver) = self.drivers.pop() {
            let pending = driver.has_pending_cleanup().map_err(H0Error::Process);
            match pending {
                Ok(false) => drop(driver),
                Ok(true) => {
                    let deadline = operation_deadline(self.request, self.overall_deadline)
                        .unwrap_or(self.overall_deadline);
                    let result = driver
                        .stop_and_wait_until(deadline)
                        .map_err(H0Error::Process);
                    if first_error.is_none() {
                        first_error = result.err();
                    }
                }
                Err(error) => {
                    drop(driver);
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn finish<T>(&mut self, result: Result<T, H0Error>) -> Result<T, H0Error> {
        let cleanup = self.cleanup();
        match result {
            Err(error) => Err(error),
            Ok(value) => cleanup.map(|()| value),
        }
    }
}

impl Drop for ProcessOwner<'_> {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
