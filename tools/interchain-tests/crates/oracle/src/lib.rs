//! Independent public-receipt oracle boundary for the interchain test harness.
//!
//! This crate compares bounded, synthetic, public protocol receipts. In H0, the
//! signature bytes, branch ID, and Merkle path are ordinary public receipt
//! fields. Comparing them does **not** claim Zcash cryptographic verification or
//! Zcash consensus validation.

const MAX_ITEMS: usize = 64;
const MAX_STRING_BYTES: usize = 256;
const MAX_SIGNATURE_BYTES: usize = 128;

/// A synthetic protocol event recorded in a public receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolEvent {
    pub kind: String,
    pub amount: u64,
}

/// An ordered synthetic state transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    pub id: String,
    pub state: String,
}

/// Assignment of a transition to exactly one synthetic accounting bucket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BucketMembership {
    pub transition_id: String,
    pub bucket: String,
}

/// A public synthetic transaction outpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutPoint {
    pub txid: [u8; 32],
    pub vout: u32,
}

/// A public synthetic Merkle path; no hashes are computed by this oracle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerklePath {
    pub siblings: Vec<[u8; 32]>,
    pub index: u64,
}

/// The complete public synthetic receipt accepted by the H0 oracle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolReceipt {
    pub amount: u64,
    pub events: Vec<ProtocolEvent>,
    pub asset_tag: String,
    pub transitions: Vec<Transition>,
    pub bucket_membership: Vec<BucketMembership>,
    pub outpoint: OutPoint,
    pub merkle_path: MerklePath,
    pub branch_id: [u8; 4],
    pub synthetic_signature: Vec<u8>,
    pub fee: u64,
    pub expiry: u64,
    pub ibc_denom_trace: String,
}

/// Stable, typed rejection categories returned by [`verify`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OracleReason {
    MalformedReceipt,
    ReceiptTooLarge,
    DuplicateBucketMembership,
    AmountMismatch,
    EventMismatch,
    AssetTagMismatch,
    TransitionOrderMismatch,
    BucketMembershipMismatch,
    OutpointMismatch,
    MerkleSiblingMismatch,
    MerkleIndexMismatch,
    BranchIdMismatch,
    SyntheticSignatureMismatch,
    FeeMismatch,
    ExpiryMismatch,
    IbcDenomTraceMismatch,
}

/// Compares an expected receipt with an observed receipt deterministically.
///
/// Both operands are bounds-checked before any comparison. This makes malformed
/// input fail closed rather than accidentally reporting a semantic match.
pub fn verify(expected: &ProtocolReceipt, observed: &ProtocolReceipt) -> Result<(), OracleReason> {
    validate(expected)?;
    validate(observed)?;

    if expected.amount != observed.amount {
        return Err(OracleReason::AmountMismatch);
    }
    if expected.events != observed.events {
        return Err(OracleReason::EventMismatch);
    }
    if expected.asset_tag != observed.asset_tag {
        return Err(OracleReason::AssetTagMismatch);
    }
    if expected.transitions != observed.transitions {
        return Err(OracleReason::TransitionOrderMismatch);
    }
    if expected.bucket_membership != observed.bucket_membership {
        return Err(OracleReason::BucketMembershipMismatch);
    }
    if expected.outpoint != observed.outpoint {
        return Err(OracleReason::OutpointMismatch);
    }
    if expected.merkle_path.siblings != observed.merkle_path.siblings {
        return Err(OracleReason::MerkleSiblingMismatch);
    }
    if expected.merkle_path.index != observed.merkle_path.index {
        return Err(OracleReason::MerkleIndexMismatch);
    }
    if expected.branch_id != observed.branch_id {
        return Err(OracleReason::BranchIdMismatch);
    }
    if expected.synthetic_signature != observed.synthetic_signature {
        return Err(OracleReason::SyntheticSignatureMismatch);
    }
    if expected.fee != observed.fee {
        return Err(OracleReason::FeeMismatch);
    }
    if expected.expiry != observed.expiry {
        return Err(OracleReason::ExpiryMismatch);
    }
    if expected.ibc_denom_trace != observed.ibc_denom_trace {
        return Err(OracleReason::IbcDenomTraceMismatch);
    }
    Ok(())
}

fn validate(receipt: &ProtocolReceipt) -> Result<(), OracleReason> {
    if receipt.events.len() > MAX_ITEMS
        || receipt.transitions.len() > MAX_ITEMS
        || receipt.bucket_membership.len() > MAX_ITEMS
        || receipt.merkle_path.siblings.len() > MAX_ITEMS
        || receipt.synthetic_signature.len() > MAX_SIGNATURE_BYTES
        || strings(receipt).any(|value| value.len() > MAX_STRING_BYTES)
    {
        return Err(OracleReason::ReceiptTooLarge);
    }
    if receipt.events.is_empty()
        || receipt.transitions.is_empty()
        || receipt.bucket_membership.is_empty()
        || receipt.merkle_path.siblings.is_empty()
        || receipt.synthetic_signature.is_empty()
        || strings(receipt).any(|value| value.is_empty() || value.trim() != value)
    {
        return Err(OracleReason::MalformedReceipt);
    }
    for (index, membership) in receipt.bucket_membership.iter().enumerate() {
        if receipt.bucket_membership[..index]
            .iter()
            .any(|prior| prior.transition_id == membership.transition_id)
        {
            return Err(OracleReason::DuplicateBucketMembership);
        }
    }
    Ok(())
}

fn strings(receipt: &ProtocolReceipt) -> impl Iterator<Item = &str> {
    std::iter::once(receipt.asset_tag.as_str())
        .chain(std::iter::once(receipt.ibc_denom_trace.as_str()))
        .chain(receipt.events.iter().map(|event| event.kind.as_str()))
        .chain(
            receipt
                .transitions
                .iter()
                .flat_map(|transition| [transition.id.as_str(), transition.state.as_str()]),
        )
        .chain(receipt.bucket_membership.iter().flat_map(|membership| {
            [
                membership.transition_id.as_str(),
                membership.bucket.as_str(),
            ]
        }))
}
