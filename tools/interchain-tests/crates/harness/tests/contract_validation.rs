use nomic_bridge_harness::contract::{ExecutionResult, ScenarioOutcome, ScenarioStatus};
use nomic_bridge_harness::manifest::{parse_manifest, InventoryCounts};
use nomic_harness_oracle::{
    verify, BucketMembership, MerklePath, OracleReason, OutPoint, ProtocolEvent, ProtocolReceipt,
    Transition,
};

fn valid_contract(id: &str, status: &str) -> String {
    format!(
        r#"
schema_version = "1"

[[scenarios]]
id = "{id}"
case_id = "case-{id}"
spec_version = "1.0.0"
status = "{status}"
release_requirement = "H0 harness contract"
phase = "h0"
topology = "topology/smoke.toml"
backend = "process"
fidelity = "synthetic"
semantic_seed_domain = "nomic-zcash-h0"
capabilities = ["fixture-process"]
fixtures = ["synthetic-daemon"]
steps = ["start", "assert", "stop"]
barriers = ["ready"]
terminal_predicates = ["clean-exit"]
unchanged_projections = ["semantic-receipt"]
oracles = ["independent-oracle"]
invariants = ["bounded-execution"]
evidence = ["receipt.json"]
operation_deadline_ms = 1000
overall_deadline_ms = 5000
memory_budget_bytes = 67108864
artifact_budget_bytes = 1048576
ci_lane = "h0-process"
executable = "h0::{id}"
"#
    )
}

#[test]
fn rejects_duplicate_ids() {
    let input = format!(
        "{}\n{}",
        valid_contract("HAR-001", "enabled"),
        valid_contract("HAR-001", "enabled").replace("schema_version = \"1\"", "")
    );
    assert!(parse_manifest(&input)
        .unwrap_err()
        .to_string()
        .contains("duplicate scenario id HAR-001"));
}

#[test]
fn rejects_unknown_fields_and_statuses() {
    let unknown = valid_contract("HAR-001", "enabled")
        .replace("executable =", "surprise = true\nexecutable =");
    assert!(parse_manifest(&unknown)
        .unwrap_err()
        .to_string()
        .contains("unknown field"));
    let status = valid_contract("HAR-001", "paused");
    assert!(parse_manifest(&status)
        .unwrap_err()
        .to_string()
        .contains("unknown variant `paused`"));
}

#[test]
fn rejects_enabled_without_executable() {
    let input = valid_contract("HAR-001", "enabled").replace("executable = \"h0::HAR-001\"", "");
    assert!(parse_manifest(&input)
        .unwrap_err()
        .to_string()
        .contains("HAR-001: enabled scenario requires executable"));
}

#[test]
fn rejects_missing_semantic_requirements() {
    for (line, expected) in [
        ("oracles = [\"independent-oracle\"]", "oracles"),
        ("evidence = [\"receipt.json\"]", "evidence"),
        ("operation_deadline_ms = 1000", "operation_deadline_ms"),
        ("memory_budget_bytes = 67108864", "memory_budget_bytes"),
    ] {
        let input = valid_contract("HAR-001", "enabled").replace(line, "");
        assert!(parse_manifest(&input)
            .unwrap_err()
            .to_string()
            .contains(expected));
    }
}

#[test]
fn rejects_empty_required_values_and_oversized_inputs() {
    let empty = valid_contract("HAR-001", "enabled")
        .replace("oracles = [\"independent-oracle\"]", "oracles = []");
    assert!(parse_manifest(&empty)
        .unwrap_err()
        .to_string()
        .contains("oracles must not be empty"));
    let whitespace = valid_contract("HAR-001", "enabled").replace(
        "release_requirement = \"H0 harness contract\"",
        "release_requirement = \"   \"",
    );
    assert!(parse_manifest(&whitespace)
        .unwrap_err()
        .to_string()
        .contains("release_requirement must not be empty"));
    let whitespace_list_item = valid_contract("HAR-001", "enabled")
        .replace("oracles = [\"independent-oracle\"]", "oracles = [\"   \"]");
    assert!(parse_manifest(&whitespace_list_item)
        .unwrap_err()
        .to_string()
        .contains("oracles must not be empty"));
    let whitespace_executable = valid_contract("HAR-001", "enabled")
        .replace("executable = \"h0::HAR-001\"", "executable = \"   \"");
    assert!(parse_manifest(&whitespace_executable)
        .unwrap_err()
        .to_string()
        .contains("enabled scenario requires executable"));
    let huge =
        valid_contract("HAR-001", "enabled").replace("H0 harness contract", &"x".repeat(5000));
    assert!(parse_manifest(&huge)
        .unwrap_err()
        .to_string()
        .contains("release_requirement exceeds"));

    let oversized_list = valid_contract("HAR-001", "enabled").replace(
        "capabilities = [\"fixture-process\"]",
        &format!(
            "capabilities = [{}]",
            std::iter::repeat_n("\"capability\"", 33)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    assert!(parse_manifest(&oversized_list)
        .unwrap_err()
        .to_string()
        .contains("capabilities exceeds 32 items"));
}

#[test]
fn accepts_only_canonical_generic_scenario_ids() {
    for id in ["HAR-001", "HAR-011", "HAR-999"] {
        parse_manifest(&valid_contract(id, "enabled")).unwrap();
    }
    for id in [
        "HAR-000", "HAR-01", "HAR-0001", "HAR-+01", "HAR-1A1", "har-001",
    ] {
        assert!(
            parse_manifest(&valid_contract(id, "enabled"))
                .unwrap_err()
                .to_string()
                .contains(&format!("invalid scenario id {id}")),
            "accepted invalid id {id}"
        );
    }
}

#[test]
fn har_011_parses_generically_but_is_rejected_by_h0_inventory() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios/manifest.toml");
    let mut input = std::fs::read_to_string(path).unwrap();
    input.push_str(&valid_contract("HAR-011", "enabled").replace("schema_version = \"1\"", ""));
    let manifest = parse_manifest(&input).unwrap();
    assert!(manifest
        .validate_h0_inventory()
        .unwrap_err()
        .to_string()
        .contains("H0 inventory must contain HAR-001 through HAR-010 exactly once"));
}

#[test]
fn result_accounting_is_honest() {
    assert_eq!(
        ScenarioOutcome::from_result(ScenarioStatus::Blocked, ExecutionResult::Pass),
        ScenarioOutcome::Blocked
    );
    assert_eq!(
        ScenarioOutcome::from_result(ScenarioStatus::Enabled, ExecutionResult::MissingCapability),
        ScenarioOutcome::Fail
    );
    let counts = InventoryCounts::from_outcomes([
        ScenarioOutcome::Blocked,
        ScenarioOutcome::Pass,
        ScenarioOutcome::Fail,
    ]);
    assert_eq!((counts.pass, counts.fail, counts.blocked), (1, 1, 1));
}

#[test]
fn checked_in_inventory_is_complete_and_legally_blocked() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios/manifest.toml");
    let manifest = nomic_bridge_harness::manifest::load_manifest(path).unwrap();
    manifest.validate_h0_inventory().unwrap();
    assert_eq!(manifest.scenarios.len(), 10);
    for scenario in &manifest.scenarios {
        let expected = if matches!(scenario.id.as_str(), "HAR-003" | "HAR-004") {
            ScenarioStatus::Blocked
        } else {
            ScenarioStatus::Enabled
        };
        assert_eq!(
            scenario.status, expected,
            "{} has wrong status",
            scenario.id
        );
    }
}

#[test]
fn illegal_blocked_inventory_fails_closed() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios/manifest.toml");
    let input = std::fs::read_to_string(path).unwrap();
    let input = input.replacen("status = \"enabled\"", "status = \"blocked\"", 1);
    let manifest = parse_manifest(&input).unwrap();
    assert!(manifest
        .validate_h0_inventory()
        .unwrap_err()
        .to_string()
        .contains("illegal H0 status for HAR-001"));
}

#[test]
fn cli_listing_reports_inventory_but_no_execution_passes() {
    let binary = env!("CARGO_BIN_EXE_nomic-bridge-harness");
    let output = std::process::Command::new(binary)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "scenario",
            "list",
            "--manifest",
            "scenarios/manifest.toml",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema_version"], "1");
    assert_eq!(json["enabled"], 8);
    assert_eq!(json["blocked"], 2);
    assert_eq!(json["pass"], 0);
    assert_eq!(json["fail"], 0);
}

fn control_receipt() -> ProtocolReceipt {
    ProtocolReceipt {
        amount: 50_000,
        events: vec![
            ProtocolEvent {
                kind: "shielded-lock".into(),
                amount: 50_000,
            },
            ProtocolEvent {
                kind: "ibc-mint".into(),
                amount: 49_000,
            },
        ],
        asset_tag: "synthetic-zec".into(),
        transitions: vec![
            Transition {
                id: "lock".into(),
                state: "observed".into(),
            },
            Transition {
                id: "mint".into(),
                state: "final".into(),
            },
        ],
        bucket_membership: vec![
            BucketMembership {
                transition_id: "lock".into(),
                bucket: "pending".into(),
            },
            BucketMembership {
                transition_id: "mint".into(),
                bucket: "finalized".into(),
            },
        ],
        outpoint: OutPoint {
            txid: [0x11; 32],
            vout: 2,
        },
        merkle_path: MerklePath {
            siblings: vec![[0x22; 32], [0x33; 32]],
            index: 1,
        },
        branch_id: [0xaa, 0xbb, 0xcc, 0xdd],
        synthetic_signature: vec![0x44; 64],
        fee: 1_000,
        expiry: 2_000_000,
        ibc_denom_trace: "transfer/channel-0/uzec".into(),
    }
}

#[test]
fn public_receipt_mutation_ledger_is_exact() {
    type Mutation = fn(&mut ProtocolReceipt);
    let cases: [(&str, OracleReason, Mutation); 13] = [
        ("amount", OracleReason::AmountMismatch, |r| r.amount += 1),
        ("omitted event", OracleReason::EventMismatch, |r| {
            r.events.pop();
        }),
        ("asset tag", OracleReason::AssetTagMismatch, |r| {
            r.asset_tag = "synthetic-other".into();
        }),
        (
            "transition order",
            OracleReason::TransitionOrderMismatch,
            |r| r.transitions.swap(0, 1),
        ),
        (
            "duplicate bucket membership",
            OracleReason::DuplicateBucketMembership,
            |r| {
                r.bucket_membership[1].transition_id = "lock".into();
            },
        ),
        ("outpoint", OracleReason::OutpointMismatch, |r| {
            r.outpoint.vout += 1;
        }),
        ("Merkle sibling", OracleReason::MerkleSiblingMismatch, |r| {
            r.merkle_path.siblings[0][0] ^= 1
        }),
        ("Merkle index", OracleReason::MerkleIndexMismatch, |r| {
            r.merkle_path.index ^= 1;
        }),
        ("branch ID", OracleReason::BranchIdMismatch, |r| {
            r.branch_id[0] ^= 1;
        }),
        (
            "synthetic signature bytes",
            OracleReason::SyntheticSignatureMismatch,
            |r| r.synthetic_signature[0] ^= 1,
        ),
        ("fee", OracleReason::FeeMismatch, |r| r.fee += 1),
        ("expiry", OracleReason::ExpiryMismatch, |r| r.expiry += 1),
        (
            "IBC denom trace",
            OracleReason::IbcDenomTraceMismatch,
            |r| r.ibc_denom_trace = "transfer/channel-9/uzec".into(),
        ),
    ];

    let expected = control_receipt();
    for (field, reason, mutate) in cases {
        let control = expected.clone();
        assert_eq!(verify(&expected, &control), Ok(()), "control for {field}");
        let mut changed = control;
        mutate(&mut changed);
        assert_eq!(
            verify(&expected, &changed),
            Err(reason),
            "mutation for {field}"
        );
    }
}

#[test]
fn public_receipt_oracle_rejects_malformed_or_oversized_inputs_first() {
    let expected = control_receipt();
    let mut malformed = expected.clone();
    malformed.asset_tag.clear();
    malformed.amount += 1;
    assert_eq!(
        verify(&expected, &malformed),
        Err(OracleReason::MalformedReceipt)
    );

    let mut oversized = expected.clone();
    oversized.events = std::iter::repeat_n(oversized.events[0].clone(), 65).collect();
    assert_eq!(
        verify(&expected, &oversized),
        Err(OracleReason::ReceiptTooLarge)
    );

    let mut oversized_string = expected.clone();
    oversized_string.ibc_denom_trace = "x".repeat(257);
    assert_eq!(
        verify(&expected, &oversized_string),
        Err(OracleReason::ReceiptTooLarge)
    );
}

#[test]
fn oracle_dependency_closure_excludes_harness_and_production_packages() {
    let workspace = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let output = std::process::Command::new(env!("CARGO"))
        .current_dir(workspace)
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let oracle = packages
        .iter()
        .find(|package| package["name"] == "nomic-harness-oracle")
        .unwrap();
    let oracle_id = oracle["id"].as_str().unwrap();
    let nodes = metadata["resolve"]["nodes"].as_array().unwrap();
    let mut pending = vec![oracle_id];
    let mut closure = std::collections::BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !closure.insert(id) {
            continue;
        }
        let node = nodes.iter().find(|node| node["id"] == id).unwrap();
        pending.extend(
            node["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .map(|dependency| dependency.as_str().unwrap()),
        );
    }
    let prohibited = ["nomic", "nomic-bridge-harness"];
    let found: Vec<_> = packages
        .iter()
        .filter(|package| closure.contains(package["id"].as_str().unwrap()))
        .filter_map(|package| {
            let name = package["name"].as_str().unwrap();
            prohibited.contains(&name).then_some(name)
        })
        .collect();
    assert!(
        found.is_empty(),
        "prohibited oracle dependencies: {found:?}"
    );
}
