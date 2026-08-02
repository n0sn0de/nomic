use nomic_bridge_harness::contract::ScenarioOutcome;
use nomic_bridge_harness::identity::{
    NormalizedSemanticReceipt, ResourceLabel, RunIdentity, RunNonce, SemanticSeed,
    SemanticSeedInput,
};

const SOURCE_LOCK_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const SOURCE_LOCK_B: &str = "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn input() -> SemanticSeedInput<'static> {
    SemanticSeedInput {
        source_lock_digest: SOURCE_LOCK_A,
        scenario_id: "deposit-finality",
        case_id: "happy-path",
        spec_version: "zcash-h0-v1",
        topology_id: "bitcoin-nomic-regtest",
        corpus_index: 7,
    }
}

#[test]
fn same_semantic_inputs_produce_the_same_seed() {
    assert_eq!(
        SemanticSeed::derive(input()).unwrap(),
        SemanticSeed::derive(input()).unwrap()
    );
}

#[test]
fn every_semantic_field_changes_the_seed() {
    let baseline = SemanticSeed::derive(input()).unwrap();
    let variants = [
        SemanticSeedInput {
            source_lock_digest: SOURCE_LOCK_B,
            ..input()
        },
        SemanticSeedInput {
            scenario_id: "withdrawal-finality",
            ..input()
        },
        SemanticSeedInput {
            case_id: "reorg",
            ..input()
        },
        SemanticSeedInput {
            spec_version: "zcash-h0-v2",
            ..input()
        },
        SemanticSeedInput {
            topology_id: "bitcoin-nomic-two-relayers",
            ..input()
        },
        SemanticSeedInput {
            corpus_index: 8,
            ..input()
        },
    ];

    for variant in variants {
        assert_ne!(baseline, SemanticSeed::derive(variant).unwrap());
    }
}

#[test]
fn nonce_changes_resource_identity_but_not_semantic_identity() {
    let seed = SemanticSeed::derive(input()).unwrap();
    let first = RunIdentity::new(seed, RunNonce::from_fixed_bytes_for_test([1; 16]));
    let second = RunIdentity::new(seed, RunNonce::from_fixed_bytes_for_test([2; 16]));

    assert_eq!(first.semantic_seed(), second.semantic_seed());
    assert_ne!(
        first.resource_label("zcash-node").unwrap(),
        second.resource_label("zcash-node").unwrap()
    );
}

#[test]
fn normalized_receipts_ignore_run_diagnostics() {
    let seed = SemanticSeed::derive(input()).unwrap();
    let first_receipt = NormalizedSemanticReceipt::new(
        seed,
        "deposit-finality",
        "happy-path",
        ScenarioOutcome::Pass,
    )
    .unwrap();
    let second_receipt = NormalizedSemanticReceipt::new(
        seed,
        "deposit-finality",
        "happy-path",
        ScenarioOutcome::Pass,
    )
    .unwrap();
    let first_diagnostics = (
        RunNonce::from_fixed_bytes_for_test([1; 16]),
        "host-path-a",
        "dynamic-endpoint-a",
        "container-a",
    );
    let second_diagnostics = (
        RunNonce::from_fixed_bytes_for_test([2; 16]),
        "host-path-b",
        "dynamic-endpoint-b",
        "container-b",
    );

    let first = serde_json::to_string(&first_receipt).unwrap();
    let second = serde_json::to_string(&second_receipt).unwrap();
    assert_ne!(first_diagnostics, second_diagnostics);
    assert_eq!(first, second);
    for marker in [
        "host-path-a",
        "host-path-b",
        "dynamic-endpoint-a",
        "dynamic-endpoint-b",
        "container-a",
        "container-b",
    ] {
        assert!(!first.contains(marker), "receipt leaked {marker:?}");
    }

    let with_diagnostics = format!(
        r#"{{"semantic_seed":"{}","scenario_id":"deposit-finality","case_id":"happy-path","outcome":"Pass","diagnostics":{{"path":"host-path-a"}}}}"#,
        seed.to_hex()
    );
    assert!(serde_json::from_str::<NormalizedSemanticReceipt>(&with_diagnostics).is_err());
}

#[test]
fn semantic_seed_json_is_canonical_lowercase_hex_and_round_trips() {
    let seed = SemanticSeed::derive(input()).unwrap();
    let encoded = serde_json::to_string(&seed).unwrap();

    assert_eq!(encoded, format!("\"{}\"", seed.to_hex()));
    assert_eq!(
        serde_json::from_str::<SemanticSeed>(&encoded).unwrap(),
        seed
    );
}

#[test]
fn semantic_seed_json_rejects_uppercase_and_malformed_encodings() {
    let canonical = SemanticSeed::derive(input()).unwrap().to_hex();
    for encoded in [
        canonical.to_uppercase(),
        "0".repeat(63),
        "0".repeat(65),
        format!("{}g", &canonical[..63]),
    ] {
        let json = serde_json::to_string(&encoded).unwrap();
        assert!(
            serde_json::from_str::<SemanticSeed>(&json).is_err(),
            "accepted {encoded:?}"
        );
    }
    assert!(serde_json::from_str::<SemanticSeed>("[0,1,2]").is_err());
}

#[test]
fn malformed_digests_and_blank_or_oversized_semantic_fields_fail() {
    for digest in ["abc", &"g".repeat(64), &"0".repeat(66)] {
        assert!(SemanticSeed::derive(SemanticSeedInput {
            source_lock_digest: digest,
            ..input()
        })
        .is_err());
    }
    assert!(SemanticSeed::derive(SemanticSeedInput {
        scenario_id: " ",
        ..input()
    })
    .is_err());
    let oversized = "x".repeat(257);
    assert!(SemanticSeed::derive(SemanticSeedInput {
        topology_id: &oversized,
        ..input()
    })
    .is_err());
}

#[test]
fn resource_labels_are_safe_bounded_and_keep_collision_resistant_suffixes() {
    for unsafe_label in [
        "",
        " ",
        "UPPER",
        "has space",
        "path/name",
        "dot.name",
        "-leading",
        "trailing-",
    ] {
        assert!(
            ResourceLabel::parse(unsafe_label).is_err(),
            "accepted {unsafe_label:?}"
        );
    }
    assert!(ResourceLabel::parse(&"a".repeat(ResourceLabel::MAX_LEN + 1)).is_err());

    let seed = SemanticSeed::derive(input()).unwrap();
    let identity = RunIdentity::new(seed, RunNonce::from_fixed_bytes_for_test([9; 16]));
    let long_a = format!("service-{}-a", "x".repeat(240));
    let long_b = format!("service-{}-b", "x".repeat(240));
    let oversized = "x".repeat(ResourceLabel::MAX_INPUT_LEN + 1);
    assert!(identity.resource_label(&oversized).is_err());
    let label_a = identity.resource_label(&long_a).unwrap();
    let label_b = identity.resource_label(&long_b).unwrap();

    assert!(label_a.as_str().len() <= ResourceLabel::MAX_LEN);
    assert!(label_b.as_str().len() <= ResourceLabel::MAX_LEN);
    assert_ne!(label_a, label_b);
}
