#!/usr/bin/env python3
"""Model P1-001: failed ABCI calls persist partial mutations."""

from common import CallError, Store, emit, source_order_deliver_tx, transactional_deliver_tx


def mutate_then_error(state):
    state["counter"] += 1
    state["events"].append("mutated-before-error")
    raise CallError("inner call returned Err after mutation")


initial = {"counter": 0, "events": []}

source_store = Store(initial.copy())
source_result = source_order_deliver_tx(source_store, mutate_then_error)

control_store = Store(initial.copy())
control_result = transactional_deliver_tx(control_store, mutate_then_error)

assert source_result["code"] == 1
assert source_store.state["counter"] == 1
assert control_result["code"] == 1
assert control_store.state["counter"] == 0

emit(
    "P1-001 failed-call persistence model",
    [
        "orga/src/abci/node.rs:482-486",
        "orga/src/abci/node.rs:558-590",
    ],
    {
        "initial": initial,
        "source_order_result": source_result,
        "source_order_committed_state": source_store.state,
        "transactional_control_result": control_result,
        "transactional_control_state": control_store.state,
    },
    [
        "failed source-order DeliverTx committed counter = 1",
        "transactional control rolled counter back to 0",
    ],
)
