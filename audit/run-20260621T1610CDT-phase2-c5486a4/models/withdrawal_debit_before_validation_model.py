#!/usr/bin/env python3
"""Model P1-004: withdrawal account debit before output validation."""

from common import CallError, Store, emit, source_order_deliver_tx, transactional_deliver_tx


SIGNER = "nomic1alice"


def withdraw(state, amount, script_valid):
    balance = state["accounts"].get(SIGNER, 0)
    if balance < amount:
        raise CallError("insufficient funds")

    state["accounts"][SIGNER] = balance - amount
    state["events"].append(f"debited:{amount}")

    if not script_valid:
        raise CallError("Script exceeds maximum length")

    state["checkpoint_outputs"].append({"signer": SIGNER, "amount": amount})
    state["events"].append("checkpoint-output-added")


initial = {
    "accounts": {SIGNER: 100},
    "checkpoint_outputs": [],
    "events": [],
}

source_store = Store(initial)
source_result = source_order_deliver_tx(
    source_store,
    lambda state: withdraw(state, amount=40, script_valid=False),
)

control_store = Store({"accounts": {SIGNER: 100}, "checkpoint_outputs": [], "events": []})
control_result = transactional_deliver_tx(
    control_store,
    lambda state: withdraw(state, amount=40, script_valid=False),
)

assert source_result["code"] == 1
assert source_store.state["accounts"][SIGNER] == 60
assert source_store.state["checkpoint_outputs"] == []
assert control_result["code"] == 1
assert control_store.state["accounts"][SIGNER] == 100

emit(
    "P1-004 withdrawal debit-before-validation model",
    [
        "src/bitcoin/mod.rs:740-751",
        "src/bitcoin/mod.rs:762-813",
    ],
    {
        "source_order_result": source_result,
        "source_order_committed_state": source_store.state,
        "transactional_control_result": control_result,
        "transactional_control_state": control_store.state,
    },
    [
        "failed invalid withdrawal persisted account debit",
        "transactional control preserved account balance",
    ],
)
