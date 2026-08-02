use nomic_bridge_harness::contract::{
    h0_binding_registry, BindingRegistry, ContractProfile, ExecutableBinding, ExecutionResult,
    ScenarioOutcome, ScenarioStatus,
};
use nomic_bridge_harness::manifest::{load_manifest, parse_manifest, InventoryCounts};
use nomic_harness_oracle::{
    verify, BucketMembership, MerklePath, OracleReason, OutPoint, ProtocolEvent, ProtocolReceipt,
    Transition,
};

fn valid_contract(id: &str, status: &str) -> String {
    format!(
        r#"
schema_version = "1"
profile = "h0"

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
executable = "h0/v1::har_001"
"#
    )
}

#[test]
fn rejects_duplicate_ids() {
    let input = format!(
        "{}\n{}",
        valid_contract("HAR-001", "enabled"),
        valid_contract("HAR-001", "enabled")
            .replace("schema_version = \"1\"", "")
            .replace("profile = \"h0\"", "")
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
    let input = valid_contract("HAR-001", "enabled").replace("executable = \"h0/v1::har_001\"", "");
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
        .replace("executable = \"h0/v1::har_001\"", "executable = \"   \"");
    assert!(parse_manifest(&whitespace_executable)
        .unwrap_err()
        .to_string()
        .contains("executable binding must use canonical"));
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
fn executable_bindings_are_typed_versioned_and_canonical() {
    for invalid in [
        "h0::har_001",
        "h0/v0::har_001",
        "h0/v01::har_001",
        "H0/v1::har_001",
        "h0/v1::HAR_001",
        "h0/v1::har-001",
        " h0/v1::har_001",
    ] {
        let input = valid_contract("HAR-001", "enabled").replace("h0/v1::har_001", invalid);
        assert!(
            parse_manifest(&input).is_err(),
            "accepted non-canonical binding {invalid:?}"
        );
    }
    assert_eq!(
        ExecutableBinding::parse("h0/v1::har_001").unwrap().as_str(),
        "h0/v1::har_001"
    );
}

#[test]
fn blocked_contracts_must_not_have_executable_bindings() {
    let input = valid_contract("HAR-003", "blocked");
    assert!(parse_manifest(&input)
        .unwrap_err()
        .to_string()
        .contains("HAR-003: blocked scenario must not have executable"));
}

#[test]
fn declared_profile_rejects_unknown_or_misspelled_bindings() {
    let generic = parse_manifest(
        &valid_contract("HAR-001", "enabled").replace("h0/v1::har_001", "h0/v1::har_001_typo"),
    )
    .unwrap();
    let registry = h0_binding_registry();
    assert!(generic
        .validate_declared_profile(&registry)
        .unwrap_err()
        .to_string()
        .contains("does not match registered binding h0/v1::har_001"));
}

#[test]
fn declared_profile_rejects_swapped_bindings() {
    let input = valid_contract("HAR-001", "enabled").replace("h0/v1::har_001", "h0/v1::har_002");
    let manifest = parse_manifest(&input).unwrap();
    assert!(manifest
        .validate_declared_profile(&h0_binding_registry())
        .unwrap_err()
        .to_string()
        .contains("HAR-001: executable binding h0/v1::har_002 does not match registered binding h0/v1::har_001"));

    let absent = parse_manifest(&valid_contract("HAR-011", "enabled")).unwrap();
    assert!(absent
        .validate_declared_profile(&h0_binding_registry())
        .unwrap_err()
        .to_string()
        .contains("HAR-011: no executable binding registered"));
}

#[test]
fn binding_registries_are_explicit_and_h0_has_exactly_eight() {
    let registry = h0_binding_registry();
    assert_eq!(registry.profile(), &ContractProfile::parse("h0").unwrap());
    assert_eq!(registry.len(), 8);
    let expected = [1, 2, 5, 6, 7, 8, 9, 10]
        .map(|number| ExecutableBinding::parse(&format!("h0/v1::har_{number:03}")).unwrap());
    assert_eq!(
        registry
            .bindings()
            .map(|(id, binding)| (id.to_owned(), binding.clone()))
            .collect::<Vec<_>>(),
        [1, 2, 5, 6, 7, 8, 9, 10]
            .into_iter()
            .zip(expected)
            .map(|(number, binding)| (format!("HAR-{number:03}"), binding))
            .collect::<Vec<_>>()
    );

    let future = BindingRegistry::new(
        ContractProfile::parse("future").unwrap(),
        [(
            "HAR-011".into(),
            ExecutableBinding::parse("future/v1::har_011").unwrap(),
        )],
    )
    .unwrap();
    assert_eq!(future.len(), 1);
}

#[test]
fn binding_registry_rejects_duplicate_entries() {
    let profile = ContractProfile::parse("h0").unwrap();
    let har_001 = ExecutableBinding::parse("h0/v1::har_001").unwrap();
    let har_002 = ExecutableBinding::parse("h0/v1::har_002").unwrap();

    let duplicate_id = BindingRegistry::new(
        profile.clone(),
        [
            ("HAR-001".into(), har_001.clone()),
            ("HAR-001".into(), har_002),
        ],
    )
    .unwrap_err();
    assert!(duplicate_id
        .to_string()
        .contains("duplicate registry scenario ID HAR-001"));

    let duplicate_binding = BindingRegistry::new(
        profile,
        [
            ("HAR-001".into(), har_001.clone()),
            ("HAR-002".into(), har_001),
        ],
    )
    .unwrap_err();
    assert!(duplicate_binding
        .to_string()
        .contains("duplicate registry executable binding h0/v1::har_001"));

    let blank_id = BindingRegistry::new(
        ContractProfile::parse("h0").unwrap(),
        [(
            " ".into(),
            ExecutableBinding::parse("h0/v1::har_001").unwrap(),
        )],
    )
    .unwrap_err();
    assert_eq!(
        blank_id.to_string(),
        "registry scenario ID must not be blank"
    );

    let wrong_profile = BindingRegistry::new(
        ContractProfile::parse("h0").unwrap(),
        [(
            "HAR-001".into(),
            ExecutableBinding::parse("h1/v1::har_001").unwrap(),
        )],
    )
    .unwrap_err();
    assert!(wrong_profile
        .to_string()
        .contains("binding h1/v1::har_001 does not belong to profile h0"));
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
    input.push_str(
        &valid_contract("HAR-011", "enabled")
            .replace("schema_version = \"1\"", "")
            .replace("profile = \"h0\"", ""),
    );
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
    manifest
        .validate_declared_profile(&h0_binding_registry())
        .unwrap();
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
fn checked_in_manifest_declares_h0_profile() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios/manifest.toml");
    let manifest = load_manifest(path).unwrap();
    assert_eq!(manifest.profile.as_str(), "h0");
}

#[test]
fn file_loader_rejects_an_oversized_stream_before_parsing() {
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "nomic-harness-oversized-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&vec![b'x'; 1_048_577]).unwrap();
    drop(file);

    let error = load_manifest(&path).unwrap_err().to_string();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(error, "manifest exceeds 1048576 bytes");
}

#[test]
fn illegal_blocked_inventory_fails_closed() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios/manifest.toml");
    let input = std::fs::read_to_string(path).unwrap();
    let input = input.replacen("status = \"enabled\"", "status = \"blocked\"", 1);
    assert!(parse_manifest(&input)
        .unwrap_err()
        .to_string()
        .contains("HAR-001: blocked scenario must not have executable"));
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
fn oracle_dependency_closure_is_fail_closed() {
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
    let found: std::collections::BTreeSet<_> = packages
        .iter()
        .filter(|package| closure.contains(package["id"].as_str().unwrap()))
        .map(|package| package["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        found,
        std::collections::BTreeSet::from(["nomic-harness-oracle"]),
        "oracle dependency closure must match the fail-closed allowlist"
    );
}
