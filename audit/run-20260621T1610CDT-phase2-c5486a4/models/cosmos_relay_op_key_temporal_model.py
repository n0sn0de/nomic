#!/usr/bin/env python3
"""Model P1-006: relay_op_key temporal and type-url validation gap.

This is not an exploit proof. It captures the validation shape observed in
source: proofs are checked at a supplied historical height, while consensus-key
membership is checked against the latest cached header. It also records that the
protobuf Any type_url is not part of the acceptance condition in the source.
"""

from common import CallError, Store, emit, source_order_deliver_tx


def relay_op_key(state, proof_height, cons_key, op_key, any_type_url):
    if proof_height not in state["consensus_roots"]:
        raise CallError("No consensus state for given height")

    membership_height = state["latest_header_height"]
    if cons_key not in state["validator_sets"][membership_height]:
        raise CallError("Consensus key is not in most recent validator set")

    if (proof_height, "staking", cons_key) not in state["proofs"]:
        raise CallError("Invalid staking proof")
    if (proof_height, "acc", cons_key) not in state["proofs"]:
        raise CallError("Invalid account proof")

    state["op_keys_by_cons"][cons_key] = {
        "op_key": op_key,
        "proof_height": proof_height,
        "membership_checked_height": membership_height,
        "accepted_any_type_url": any_type_url,
    }


initial = {
    "consensus_roots": {10: "root10", 20: "root20"},
    "latest_header_height": 20,
    "validator_sets": {
        10: {"cons-key-at-height-10"},
        20: {"cons-key-at-height-20"},
    },
    "proofs": {
        (10, "staking", "cons-key-at-height-20"),
        (10, "acc", "cons-key-at-height-20"),
    },
    "op_keys_by_cons": {},
}

store = Store(initial)
result = source_order_deliver_tx(
    store,
    lambda state: relay_op_key(
        state,
        proof_height=10,
        cons_key="cons-key-at-height-20",
        op_key="secp256k1-op-key-bytes",
        any_type_url="/cosmos.crypto.ed25519.PubKey",
    ),
)

accepted = store.state["op_keys_by_cons"]["cons-key-at-height-20"]

assert result["code"] == 0
assert accepted["proof_height"] == 10
assert accepted["membership_checked_height"] == 20
assert accepted["accepted_any_type_url"] == "/cosmos.crypto.ed25519.PubKey"

emit(
    "P1-006 relay_op_key temporal/type-url validation model",
    [
        "src/cosmos.rs:151-176",
        "src/cosmos.rs:195-203",
    ],
    {
        "result": result,
        "accepted_mapping": accepted,
        "temporal_consistency_invariant": "membership_checked_height == proof_height",
        "type_url_invariant": "Any.type_url must match decoded key type",
    },
    [
        "model accepted proof_height 10 while checking validator membership at height 20",
        "model accepted a non-secp256k1 type_url because type_url is not in the source acceptance condition",
    ],
)
