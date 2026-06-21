#!/usr/bin/env python3
"""Model P1-003: deposit outpoint inserted before deposits_enabled check."""

from common import CallError, Store, emit, source_order_deliver_tx, transactional_deliver_tx


OUTPOINT = "txid:0"


def relay_deposit(state, deposits_enabled):
    if not state["proof_valid"]:
        raise CallError("invalid proof")
    if OUTPOINT in state["processed_outpoints"]:
        raise CallError("Output has already been relayed")

    state["processed_outpoints"].add(OUTPOINT)
    state["events"].append("processed-outpoint-inserted")

    if not deposits_enabled:
        raise CallError("Deposits are disabled for the given checkpoint")

    state["checkpoint_inputs"].append(OUTPOINT)
    state["events"].append("checkpoint-input-added")


initial = {
    "proof_valid": True,
    "processed_outpoints": set(),
    "checkpoint_inputs": [],
    "events": [],
}

source_store = Store(initial)
disabled_result = source_order_deliver_tx(
    source_store,
    lambda state: relay_deposit(state, deposits_enabled=False),
)
retry_result = source_order_deliver_tx(
    source_store,
    lambda state: relay_deposit(state, deposits_enabled=True),
)

control_store = Store(
    {
        "proof_valid": True,
        "processed_outpoints": set(),
        "checkpoint_inputs": [],
        "events": [],
    }
)
control_disabled = transactional_deliver_tx(
    control_store,
    lambda state: relay_deposit(state, deposits_enabled=False),
)
control_retry = transactional_deliver_tx(
    control_store,
    lambda state: relay_deposit(state, deposits_enabled=True),
)

assert disabled_result["code"] == 1
assert retry_result["code"] == 1
assert OUTPOINT in source_store.state["processed_outpoints"]
assert source_store.state["checkpoint_inputs"] == []
assert control_disabled["code"] == 1
assert control_retry["code"] == 0
assert control_store.state["checkpoint_inputs"] == [OUTPOINT]

emit(
    "P1-003 deposit outpoint poisoning model",
    [
        "src/bitcoin/mod.rs:606-613",
        "src/bitcoin/mod.rs:615-619",
    ],
    {
        "source_order_disabled_result": disabled_result,
        "source_order_retry_result": retry_result,
        "source_order_committed_state": {
            "processed_outpoints": sorted(source_store.state["processed_outpoints"]),
            "checkpoint_inputs": source_store.state["checkpoint_inputs"],
            "events": source_store.state["events"],
        },
        "transactional_disabled_result": control_disabled,
        "transactional_retry_result": control_retry,
        "transactional_state": {
            "processed_outpoints": sorted(control_store.state["processed_outpoints"]),
            "checkpoint_inputs": control_store.state["checkpoint_inputs"],
            "events": control_store.state["events"],
        },
    },
    [
        "failed disabled-deposit relay persisted processed outpoint",
        "later enabled retry was blocked as already relayed",
        "transactional control allowed retry to succeed",
    ],
)
