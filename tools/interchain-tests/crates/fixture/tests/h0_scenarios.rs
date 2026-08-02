use nomic_bridge_harness::contract::{
    Backend, ExecutableBinding, Fidelity, ScenarioContract, ScenarioStatus,
};
use nomic_bridge_harness::driver::process::{
    active_cleanup_slot_count, pending_cleanup_count, ProcessContainment, ProcessDriver,
    ProcessSpec,
};
use nomic_bridge_harness::identity::RunNonce;
use nomic_bridge_harness::manifest::load_manifest;
use nomic_bridge_harness::readiness::{protocol_request, AwaitBudget, ExpectedReadiness};
use nomic_bridge_harness::scenario::h0::{run_h0, H0Controls, H0Error, H0Receipt, H0RunnerRequest};
use nomic_bridge_harness::topology::{Topology, TopologyError};
use nomic_harness_fixture::{CanonicalId, Request, Response};
use std::ffi::OsString;
use std::sync::Mutex;
use std::time::Duration;

static SERIAL: Mutex<()> = Mutex::new(());
const SOURCE_LOCK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const SMOKE: &str = r#"
schema_version = "1"
id = "smoke"
nodes = ["fixture"]
links = []
"#;

fn contract(id: &str) -> ScenarioContract {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    load_manifest(root.join("scenarios/manifest.toml"))
        .unwrap()
        .scenarios
        .into_iter()
        .find(|scenario| scenario.id == id)
        .unwrap()
}

fn controls() -> H0Controls {
    H0Controls::bounded_test_defaults()
}

fn execute(
    contract: &ScenarioContract,
    topology: &Topology,
    controls: H0Controls,
    nonces: [RunNonce; 2],
) -> Result<H0Receipt, H0Error> {
    run_h0(H0RunnerRequest {
        contract,
        topology,
        fixture_executable: OsString::from(env!("CARGO_BIN_EXE_nomic-harness-fixture")),
        source_lock_digest: SOURCE_LOCK,
        run_nonces: nonces,
        network_id: CanonicalId::new("h0-testnet").unwrap(),
        corpus_index: 7,
        controls,
    })
}

fn nonces() -> [RunNonce; 2] {
    [RunNonce::from_bytes([1; 16]), RunNonce::from_bytes([2; 16])]
}

fn assert_clean() {
    assert_eq!(pending_cleanup_count(), 0);
    assert_eq!(active_cleanup_slot_count(), 0);
}

fn healthy_peer() -> ProcessDriver {
    let component = CanonicalId::new("peer").unwrap();
    let run = CanonicalId::new("peer-run").unwrap();
    let network = CanonicalId::new("peer-net").unwrap();
    let driver = ProcessDriver::spawn(ProcessSpec {
        program: OsString::from(env!("CARGO_BIN_EXE_nomic-harness-fixture")),
        args: [
            "--listen",
            "127.0.0.1:0",
            "--mode",
            "healthy",
            "--argv-canary",
            "peer-argv",
            "--log-canary",
            "peer-log",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        env: vec![(
            OsString::from("NOMIC_FIXTURE_SYNTHETIC_CANARY"),
            OsString::from("peer-env"),
        )],
        stdin: serde_json::to_vec(&serde_json::json!({
            "config_canary": "peer-config", "component_id": component,
            "run_id": run, "network_id": network, "capabilities": []
        }))
        .unwrap(),
        component_id: component.clone(),
        run_id: run.clone(),
        network_id: network.clone(),
        startup_deadline: Duration::from_secs(1),
        startup_until: None,
        containment: ProcessContainment::NoProcessGroupOrSessionEscape,
    })
    .unwrap();
    driver
        .await_ready(
            &ExpectedReadiness::new(component, run, network, []).unwrap(),
            AwaitBudget::new(8, Duration::from_secs(1), Duration::from_millis(5)).unwrap(),
        )
        .unwrap();
    driver
}

#[test]
fn red_evidence_unimplemented_h0_binding_is_typed_unsupported() {
    let _serial = SERIAL.lock().unwrap();
    let topology = Topology::parse(SMOKE).unwrap();
    assert!(matches!(
        execute(&contract("HAR-002"), &topology, controls(), nonces()),
        Err(H0Error::UnsupportedScenario)
    ));
    assert_clean();
}

#[test]
fn har_001_starts_in_deterministic_topological_order_and_stops_in_reverse() {
    let _serial = SERIAL.lock().unwrap();
    let topology = Topology::parse(
        r#"
schema_version = "1"
id = "dag"
nodes = ["charlie", "alpha", "bravo"]
links = [{ from = "alpha", to = "charlie" }, { from = "bravo", to = "charlie" }]
"#,
    )
    .unwrap();
    let mut contract = contract("HAR-001");
    contract.topology = "topology/dag.toml".into();
    let H0Receipt::Har001(receipt) = execute(&contract, &topology, controls(), nonces()).unwrap()
    else {
        panic!("wrong receipt")
    };
    assert_eq!(receipt.startup_order, ["alpha", "bravo", "charlie"]);
    assert_eq!(receipt.shutdown_order, ["charlie", "bravo", "alpha"]);
    assert_clean();
}

#[test]
fn har_001_cleanup_is_scoped_while_an_unrelated_peer_remains_alive() {
    let _serial = SERIAL.lock().unwrap();
    let peer = healthy_peer();
    let topology = Topology::parse(SMOKE).unwrap();
    execute(&contract("HAR-001"), &topology, controls(), nonces()).unwrap();
    assert!(matches!(
        protocol_request(
            peer.endpoint(),
            &Request::Echo { payload: "still-alive".into() },
            std::time::Instant::now() + Duration::from_secs(1),
        ),
        Ok(Response::Echo { payload }) if payload == "still-alive"
    ));
    peer.stop_and_wait(Duration::from_secs(1)).unwrap();
    assert_clean();
}

#[test]
fn har_005_replays_two_real_runs_with_equal_semantics_and_distinct_resources() {
    let _serial = SERIAL.lock().unwrap();
    let topology = Topology::parse(SMOKE).unwrap();
    let H0Receipt::Har005(receipt) =
        execute(&contract("HAR-005"), &topology, controls(), nonces()).unwrap()
    else {
        panic!("wrong receipt")
    };
    assert!(receipt.equal);
    assert_eq!(receipt.first, receipt.second);
    assert!(receipt.resource_labels_differ);
    assert_clean();
}

#[cfg(target_os = "linux")]
#[test]
fn har_006_retries_real_typed_failures_then_times_out_and_reaps_tree() {
    let _serial = SERIAL.lock().unwrap();
    let topology = Topology::parse(SMOKE).unwrap();
    let H0Receipt::Har006(receipt) =
        execute(&contract("HAR-006"), &topology, controls(), nonces()).unwrap()
    else {
        panic!("wrong receipt")
    };
    assert_eq!(receipt.receipt.retry_attempts, 3);
    assert_eq!(receipt.receipt.permanent_attempts, 1);
    assert_eq!(
        receipt.receipt.permanent_outcome,
        nomic_bridge_harness::retry::RetryOutcome::Permanent
    );
    assert!(receipt.receipt.cleanup_restored);
    let mut changed_diagnostics = receipt.clone();
    changed_diagnostics.diagnostics.retry_elapsed += Duration::from_secs(1);
    changed_diagnostics.diagnostics.permanent_elapsed += Duration::from_secs(1);
    changed_diagnostics.diagnostics.watchdog_elapsed += Duration::from_secs(1);
    assert_eq!(receipt, changed_diagnostics);
    let serialized = serde_json::to_string(&receipt.receipt).unwrap();
    assert!(!serialized.contains("elapsed"));
    assert_clean();
}

#[test]
fn wrong_binding_backend_status_fidelity_topology_and_same_nonce_fail_closed() {
    let _serial = SERIAL.lock().unwrap();
    let topology = Topology::parse(SMOKE).unwrap();

    let mut wrong = contract("HAR-001");
    wrong.executable = Some(ExecutableBinding::parse("h0/v1::har_005").unwrap());
    assert!(matches!(
        execute(&wrong, &topology, controls(), nonces()),
        Err(H0Error::WrongBinding)
    ));
    wrong = contract("HAR-001");
    wrong.backend = Backend::Container;
    assert!(matches!(
        execute(&wrong, &topology, controls(), nonces()),
        Err(H0Error::WrongBackend)
    ));
    wrong = contract("HAR-001");
    wrong.status = ScenarioStatus::Blocked;
    assert!(matches!(
        execute(&wrong, &topology, controls(), nonces()),
        Err(H0Error::DisabledScenario)
    ));
    wrong = contract("HAR-001");
    wrong.fidelity = Fidelity::Integrated;
    assert!(matches!(
        execute(&wrong, &topology, controls(), nonces()),
        Err(H0Error::WrongFidelity)
    ));
    wrong = contract("HAR-001");
    wrong.topology = "topology/other.toml".into();
    assert!(matches!(
        execute(&wrong, &topology, controls(), nonces()),
        Err(H0Error::TopologyMismatch)
    ));
    let nonce = RunNonce::from_bytes([9; 16]);
    assert!(matches!(
        execute(&contract("HAR-005"), &topology, controls(), [nonce, nonce]),
        Err(H0Error::SameRunNonce)
    ));
    assert_clean();
}

#[test]
fn startup_failure_cleans_already_ready_nodes_in_reverse() {
    let _serial = SERIAL.lock().unwrap();
    let topology = Topology::parse(
        r#"
schema_version = "1"
id = "startup-failure"
nodes = ["alpha", "bravo"]
links = [{ from = "alpha", to = "bravo" }]
"#,
    )
    .unwrap();
    let mut contract = contract("HAR-001");
    contract.topology = "topology/startup-failure.toml".into();
    let mut controls = controls();
    controls.startup_failure_node = Some(1);
    assert!(matches!(
        execute(&contract, &topology, controls, nonces()),
        Err(H0Error::Process(_))
    ));
    assert_clean();
}

#[test]
fn exhausted_transient_failure_stays_failed_and_cleans_process() {
    let _serial = SERIAL.lock().unwrap();
    let topology = Topology::parse(SMOKE).unwrap();
    let mut controls = controls();
    controls.retry_attempts = 2;
    controls.transient_failures = 2;
    assert!(matches!(
        execute(&contract("HAR-006"), &topology, controls, nonces()),
        Err(H0Error::RetryFailed)
    ));
    assert_clean();
}

#[test]
fn topology_duplicate_unknown_cycle_reject_and_one_node_smoke_remains_valid() {
    let _serial = SERIAL.lock().unwrap();
    assert_eq!(
        Topology::parse(SMOKE)
            .unwrap()
            .startup_order()
            .unwrap()
            .len(),
        1
    );
    let duplicate = SMOKE.replace("[\"fixture\"]", "[\"fixture\", \"fixture\"]");
    assert!(matches!(
        Topology::parse(&duplicate),
        Err(TopologyError::DuplicateNode)
    ));
    let unknown = SMOKE.replace(
        "links = []",
        "links = [{ from = \"fixture\", to = \"missing\" }]",
    );
    assert!(matches!(
        Topology::parse(&unknown),
        Err(TopologyError::InvalidLink)
    ));
    let cycle = r#"
schema_version = "1"
id = "cycle"
nodes = ["alpha", "bravo"]
links = [{ from = "alpha", to = "bravo" }, { from = "bravo", to = "alpha" }]
"#;
    assert!(matches!(Topology::parse(cycle), Err(TopologyError::Cycle)));
    assert_clean();
}
