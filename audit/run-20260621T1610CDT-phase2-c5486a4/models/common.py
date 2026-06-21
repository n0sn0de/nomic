#!/usr/bin/env python3
"""Shared helpers for audit-local source-order models.

These helpers model the state-persistence distinction observed in pinned Orga:
an inner call can mutate state, return an error, and still have the mutated
state flushed by the ABCI wrapper. They intentionally avoid importing Nomic
runtime code so that the models stay portable and local-only.
"""

from __future__ import annotations

import copy
import json
from dataclasses import dataclass
from typing import Any, Callable, Dict


class CallError(Exception):
    """Expected model error representing a failed app call."""


@dataclass
class Store:
    state: Dict[str, Any]


def source_order_deliver_tx(store: Store, call: Callable[[Dict[str, Any]], None]) -> Dict[str, Any]:
    """Model pinned Orga DeliverTx source order.

    The call mutates a loaded state object. Whether the call succeeds or fails,
    the loaded state is committed back to the store.
    """

    working_state = copy.deepcopy(store.state)
    try:
        call(working_state)
        result = {"code": 0, "log": "ok"}
    except CallError as err:
        result = {"code": 1, "log": str(err)}

    store.state = working_state
    return result


def transactional_deliver_tx(store: Store, call: Callable[[Dict[str, Any]], None]) -> Dict[str, Any]:
    """Control model for expected rollback-on-error semantics."""

    working_state = copy.deepcopy(store.state)
    try:
        call(working_state)
    except CallError as err:
        return {"code": 1, "log": str(err)}

    store.state = working_state
    return {"code": 0, "log": "ok"}


def emit(title: str, source_refs: list[str], trace: Dict[str, Any], assertions: list[str]) -> None:
    print(
        json.dumps(
            {
                "title": title,
                "source_refs": source_refs,
                "trace": trace,
                "assertions": assertions,
            },
            indent=2,
            sort_keys=True,
        )
    )
