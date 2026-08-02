use nomic_bridge_harness::contract::{ExecutionResult, ScenarioOutcome, ScenarioStatus};
use nomic_bridge_harness::manifest::{parse_manifest, InventoryCounts};

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
