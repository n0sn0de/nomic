//! Deterministic semantic identities and per-execution resource identities.

use std::{error::Error, fmt};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::contract::ScenarioOutcome;

const SEMANTIC_DOMAIN: &[u8] = b"nomic-bridge-harness/semantic-seed/v1";
const RESOURCE_DOMAIN: &[u8] = b"nomic-bridge-harness/resource-label/v1";
const MAX_SEMANTIC_FIELD_LEN: usize = 256;

/// Inputs which define a test case independently of where or when it runs.
#[derive(Clone, Copy, Debug)]
pub struct SemanticSeedInput<'a> {
    pub source_lock_digest: &'a str,
    pub scenario_id: &'a str,
    pub case_id: &'a str,
    pub spec_version: &'a str,
    pub topology_id: &'a str,
    pub corpus_index: u64,
}

/// A stable digest of the semantic inputs to a harness case.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SemanticSeed([u8; 32]);

impl SemanticSeed {
    pub fn derive(input: SemanticSeedInput<'_>) -> Result<Self, IdentityError> {
        let source_lock = decode_source_lock_digest(input.source_lock_digest)?;
        validate_semantic_field("scenario_id", input.scenario_id)?;
        validate_semantic_field("case_id", input.case_id)?;
        validate_semantic_field("spec_version", input.spec_version)?;
        validate_semantic_field("topology_id", input.topology_id)?;

        let mut hasher = Sha256::new();
        encode_part(&mut hasher, b"domain", SEMANTIC_DOMAIN);
        encode_part(&mut hasher, b"source-lock-sha256", &source_lock);
        encode_part(&mut hasher, b"scenario-id", input.scenario_id.as_bytes());
        encode_part(&mut hasher, b"case-id", input.case_id.as_bytes());
        encode_part(&mut hasher, b"spec-version", input.spec_version.as_bytes());
        encode_part(&mut hasher, b"topology-id", input.topology_id.as_bytes());
        encode_part(
            &mut hasher,
            b"corpus-index-u64be",
            &input.corpus_index.to_be_bytes(),
        );
        Ok(Self(hasher.finalize().into()))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        hex(&self.0)
    }
}

impl Serialize for SemanticSeed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for SemanticSeed {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = <&str>::deserialize(deserializer)?;
        decode_canonical_digest(encoded).map(Self).map_err(|()| {
            de::Error::custom("semantic seed must be 64 lowercase hexadecimal characters")
        })
    }
}

/// Execution-only entropy. It must never influence semantic test data.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RunNonce([u8; 16]);

impl RunNonce {
    /// Obtains a fresh nonce directly from the operating system random source.
    pub fn from_os_random() -> Result<Self, IdentityError> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|error| IdentityError::Randomness(error.to_string()))?;
        Ok(Self(bytes))
    }

    /// Supplies a reproducible nonce for deterministic identity tests only.
    #[doc(hidden)]
    pub const fn from_fixed_bytes_for_test(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

/// The semantic identity paired with execution-only naming entropy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunIdentity {
    semantic_seed: SemanticSeed,
    run_nonce: RunNonce,
}

impl RunIdentity {
    pub const fn new(semantic_seed: SemanticSeed, run_nonce: RunNonce) -> Self {
        Self {
            semantic_seed,
            run_nonce,
        }
    }

    pub const fn semantic_seed(&self) -> SemanticSeed {
        self.semantic_seed
    }

    pub fn resource_label(&self, base: &str) -> Result<ResourceLabel, IdentityError> {
        ResourceLabel::for_run(base, self.semantic_seed, self.run_nonce)
    }
}

/// A container-orchestrator-safe resource label.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ResourceLabel(String);

impl ResourceLabel {
    pub const MAX_LEN: usize = 63;
    pub const MAX_INPUT_LEN: usize = 256;

    pub fn parse(value: &str) -> Result<Self, IdentityError> {
        validate_label(value, Self::MAX_LEN)?;
        Ok(Self(value.to_owned()))
    }

    fn for_run(base: &str, seed: SemanticSeed, nonce: RunNonce) -> Result<Self, IdentityError> {
        validate_label(base, Self::MAX_INPUT_LEN)?;
        let mut hasher = Sha256::new();
        encode_part(&mut hasher, b"domain", RESOURCE_DOMAIN);
        encode_part(&mut hasher, b"base", base.as_bytes());
        encode_part(&mut hasher, b"semantic-seed", &seed.0);
        encode_part(&mut hasher, b"run-nonce", &nonce.0);
        let digest: [u8; 32] = hasher.finalize().into();
        let suffix = &hex(&digest)[..16];
        let prefix_len = Self::MAX_LEN - 1 - suffix.len();
        let prefix = &base[..base.len().min(prefix_len)];
        Ok(Self(format!("{prefix}-{suffix}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Comparable semantic output. Runtime diagnostics deliberately live elsewhere.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedSemanticReceipt {
    semantic_seed: SemanticSeed,
    scenario_id: String,
    case_id: String,
    outcome: ScenarioOutcome,
}

impl NormalizedSemanticReceipt {
    pub fn new(
        semantic_seed: SemanticSeed,
        scenario_id: &str,
        case_id: &str,
        outcome: ScenarioOutcome,
    ) -> Result<Self, IdentityError> {
        validate_semantic_field("scenario_id", scenario_id)?;
        validate_semantic_field("case_id", case_id)?;
        Ok(Self {
            semantic_seed,
            scenario_id: scenario_id.to_owned(),
            case_id: case_id.to_owned(),
            outcome,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityError {
    InvalidSourceLockDigest,
    BlankField(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    InvalidResourceLabel,
    Randomness(String),
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceLockDigest => write!(
                formatter,
                "source-lock digest must be 64 lowercase hexadecimal characters"
            ),
            Self::BlankField(field) => write!(formatter, "{field} must not be blank"),
            Self::FieldTooLong { field, max } => write!(formatter, "{field} exceeds {max} bytes"),
            Self::InvalidResourceLabel => write!(
                formatter,
                "resource label must use lowercase ASCII letters, digits, or interior hyphens"
            ),
            Self::Randomness(error) => {
                write!(formatter, "operating-system randomness failed: {error}")
            }
        }
    }
}

impl Error for IdentityError {}

fn encode_part(hasher: &mut Sha256, name: &[u8], value: &[u8]) {
    hasher.update((name.len() as u32).to_be_bytes());
    hasher.update(name);
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn validate_semantic_field(field: &'static str, value: &str) -> Result<(), IdentityError> {
    if value.trim().is_empty() {
        return Err(IdentityError::BlankField(field));
    }
    if value.len() > MAX_SEMANTIC_FIELD_LEN {
        return Err(IdentityError::FieldTooLong {
            field,
            max: MAX_SEMANTIC_FIELD_LEN,
        });
    }
    Ok(())
}

fn validate_label(value: &str, max: usize) -> Result<(), IdentityError> {
    let valid_edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if value.is_empty()
        || value.len() > max
        || !value.is_ascii()
        || !value.bytes().all(|byte| valid_edge(byte) || byte == b'-')
        || !valid_edge(value.as_bytes()[0])
        || !valid_edge(value.as_bytes()[value.len() - 1])
    {
        return Err(if value.len() > max {
            IdentityError::FieldTooLong {
                field: "resource_label",
                max,
            }
        } else {
            IdentityError::InvalidResourceLabel
        });
    }
    Ok(())
}

fn decode_source_lock_digest(value: &str) -> Result<[u8; 32], IdentityError> {
    decode_canonical_digest(value).map_err(|()| IdentityError::InvalidSourceLockDigest)
}

fn decode_canonical_digest(value: &str) -> Result<[u8; 32], ()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(());
    }
    let mut decoded = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        decoded[index] = (hex_value(pair[0]) << 4) | hex_value(pair[1]);
    }
    Ok(decoded)
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => unreachable!("digest validation precedes decoding"),
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[usize::from(byte >> 4)] as char);
        output.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    output
}
