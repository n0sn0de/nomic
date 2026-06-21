#!/usr/bin/env python3
"""Model P1-002/P2: header reorg mutation before validation.

This mirrors the relevant source order:
1. If the first replacement height is at or below the current height, pop the
   current chain suffix with pop_back_to(first.height).
2. Verify and append replacement headers.
3. Only after appending, reject if added_work <= removed_work.

It also preserves the source behavior that pop_back_to returns removed work but
does not decrement current_work before replacement headers are appended.
"""

from __future__ import annotations

from dataclasses import dataclass

from common import CallError, Store, emit, source_order_deliver_tx, transactional_deliver_tx


@dataclass
class Header:
    height: int
    block_hash: str
    prev_hash: str
    work: int
    valid: bool = True


class HeaderQueue:
    def __init__(self, headers):
        self.headers = list(headers)
        self.current_work = sum(header.work for header in self.headers)

    @classmethod
    def canonical(cls):
        headers = []
        prev = "none"
        for height in range(6):
            block_hash = f"h{height}"
            headers.append(Header(height, block_hash, prev, 10))
            prev = block_hash
        return cls(headers)

    def snapshot(self):
        actual_work = sum(header.work for header in self.headers)
        return {
            "heights": [header.height for header in self.headers],
            "hashes": [header.block_hash for header in self.headers],
            "current_work": self.current_work,
            "actual_work_from_queue": actual_work,
            "tip": self.headers[-1].block_hash,
        }

    def height(self):
        return self.headers[-1].height

    def get_by_height(self, height):
        for header in self.headers:
            if header.height == height:
                return header
        return None

    def pop_back_to(self, height):
        removed_work = 0
        while self.height() >= height:
            if not self.headers:
                raise CallError("Removed all headers")
            removed = self.headers.pop()
            removed_work += removed.work
        return removed_work

    def verify_and_add_headers(self, headers):
        first_height = headers[0].height
        if first_height == 0:
            raise CallError("Headers must start after height 0")

        prev_header = self.get_by_height(first_height - 1)
        if prev_header is None:
            raise CallError("Headers not connect to chain")

        work = 0
        prev = prev_header
        for header in headers:
            if header.height != prev.height + 1:
                raise CallError("Non-consecutive headers passed")
            if header.prev_hash != prev.block_hash:
                raise CallError("Passed header references incorrect previous block hash")
            if not header.valid:
                raise CallError("Header failed proof-of-work or target validation")

            work += header.work
            chain_work = self.current_work + header.work
            self.headers.append(header)
            self.current_work = chain_work
            prev = header

        return work

    def add_into_iter(self, headers):
        headers = list(headers)
        current_height = self.height()
        first = headers[0]
        removed_work = 0

        if first.height <= current_height:
            first_replaced = self.get_by_height(first.height)
            if first_replaced is None:
                raise CallError("Header not found")
            if first_replaced.block_hash == first.block_hash:
                raise CallError("Provided redundant header")
            removed_work = self.pop_back_to(first.height)

        added_work = self.verify_and_add_headers(headers)
        if added_work <= removed_work:
            raise CallError("New best chain must include more work than old best chain")


replacement = [
    Header(4, "alt4", "h3", 1),
    Header(5, "alt5", "alt4", 1),
]

initial_queue = HeaderQueue.canonical()
initial_snapshot = initial_queue.snapshot()

source_store = Store({"queue": initial_queue})
source_result = source_order_deliver_tx(
    source_store,
    lambda state: state["queue"].add_into_iter(replacement),
)
source_snapshot = source_store.state["queue"].snapshot()

control_store = Store({"queue": HeaderQueue.canonical()})
control_result = transactional_deliver_tx(
    control_store,
    lambda state: state["queue"].add_into_iter(replacement),
)
control_snapshot = control_store.state["queue"].snapshot()

assert source_result["code"] == 1
assert source_snapshot["tip"] == "alt5"
assert source_snapshot["current_work"] == 62
assert source_snapshot["actual_work_from_queue"] == 42
assert control_result["code"] == 1
assert control_snapshot == initial_snapshot

emit(
    "P1-002 header reorg mutation-before-validation model",
    [
        "src/bitcoin/header_queue.rs:455",
        "src/bitcoin/header_queue.rs:458-463",
        "src/bitcoin/header_queue.rs:522-525",
        "src/bitcoin/header_queue.rs:620-632",
    ],
    {
        "initial_queue": initial_snapshot,
        "replacement_headers": [header.__dict__ for header in replacement],
        "source_order_result": source_result,
        "source_order_committed_queue": source_snapshot,
        "transactional_control_result": control_result,
        "transactional_control_queue": control_snapshot,
    },
    [
        "failed source-order reorg persisted alt tip",
        "current_work remained inflated relative to actual queued work",
        "transactional control preserved original queue",
    ],
)
