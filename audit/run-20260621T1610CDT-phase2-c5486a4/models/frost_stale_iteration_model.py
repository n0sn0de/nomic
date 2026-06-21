#!/usr/bin/env python3
"""Model P1-005: stale FROST signature shares poison signing state."""

from common import CallError, Store, emit, source_order_deliver_tx, transactional_deliver_tx


class Signing:
    def __init__(self, threshold=2):
        self.threshold = threshold
        self.iteration = 0
        self.commitments = {}
        self.commitments_len = 0
        self.sig_shares = {}
        self.sig_shares_len = 0
        self.signing_package = None
        self.signature = None

    def snapshot(self):
        return {
            "iteration": self.iteration,
            "state": self.state(),
            "commitments": sorted([f"{k[0]}:{k[1]}" for k in self.commitments]),
            "commitments_len": self.commitments_len,
            "sig_shares": sorted([f"{k[0]}:{k[1]}" for k in self.sig_shares]),
            "sig_shares_len": self.sig_shares_len,
            "signing_package": self.signing_package,
            "signature": self.signature,
        }

    def state(self):
        if self.commitments_len < self.threshold:
            return "Round1"
        if self.sig_shares_len < self.threshold:
            return "Round2"
        return "Complete"

    def submit_commitments(self, iteration, participant):
        if self.state() != "Round1":
            raise CallError("Not in round 1")
        if iteration != self.iteration:
            raise CallError("Invalid iteration")
        key = (iteration, participant)
        if key in self.commitments:
            raise CallError("Commitment already submitted")
        self.commitments[key] = "commitment"
        self.commitments_len += 1
        if self.state() == "Round2":
            self.signing_package = f"package-for-iteration-{self.iteration}"

    def next_iteration(self):
        self.iteration += 1
        self.commitments_len = 0
        self.sig_shares_len = 0
        self.signing_package = None

    def submit_sig_share(self, iteration, participant):
        if self.state() != "Round2":
            raise CallError("Not in round 2")
        key = (iteration, participant)
        if key in self.sig_shares:
            raise CallError("Signature share already submitted")
        if key not in self.commitments:
            raise CallError("Participant not included in this round")

        self.sig_shares[key] = "share"
        self.sig_shares_len += 1
        if self.state() == "Complete":
            self.aggregate_signature()

    def aggregate_signature(self):
        current_iteration_shares = [
            key for key in self.sig_shares if key[0] == self.iteration
        ]
        if len(current_iteration_shares) < self.threshold:
            raise CallError("Failed to aggregate signature: insufficient current iteration shares")
        self.signature = "aggregate-signature"


def prepared_signing():
    signing = Signing(threshold=2)
    signing.submit_commitments(0, 1)
    signing.submit_commitments(0, 2)
    signing.next_iteration()
    signing.submit_commitments(1, 3)
    signing.submit_commitments(1, 4)
    return signing


def submit_stale_shares(state):
    signing = state["signing"]
    signing.submit_sig_share(0, 1)
    signing.submit_sig_share(0, 2)


def submit_current_shares(state):
    signing = state["signing"]
    signing.submit_sig_share(1, 3)
    signing.submit_sig_share(1, 4)


initial_signing = prepared_signing()
source_store = Store({"signing": initial_signing})
source_result = source_order_deliver_tx(source_store, submit_stale_shares)
after_stale = source_store.state["signing"].snapshot()
source_retry = source_order_deliver_tx(source_store, submit_current_shares)

control_store = Store({"signing": prepared_signing()})
control_stale = transactional_deliver_tx(control_store, submit_stale_shares)
control_retry = transactional_deliver_tx(control_store, submit_current_shares)

assert source_result["code"] == 1
assert after_stale["state"] == "Complete"
assert after_stale["sig_shares"] == ["0:1", "0:2"]
assert source_retry["code"] == 1
assert "Not in round 2" in source_retry["log"]
assert control_stale["code"] == 1
assert control_retry["code"] == 0
assert control_store.state["signing"].signature == "aggregate-signature"

emit(
    "P1-005 FROST stale-iteration share poisoning model",
    [
        "src/frost/signing.rs:65-70",
        "src/frost/signing.rs:101-123",
        "src/frost/signing.rs:128-151",
        "src/frost/signing.rs:196-205",
    ],
    {
        "prepared_signing": prepared_signing().snapshot(),
        "source_order_stale_result": source_result,
        "source_order_after_stale": after_stale,
        "source_order_current_retry": source_retry,
        "source_order_after_retry": source_store.state["signing"].snapshot(),
        "transactional_stale_result": control_stale,
        "transactional_current_retry": control_retry,
        "transactional_final": control_store.state["signing"].snapshot(),
    },
    [
        "stale shares from iteration 0 reached Complete during iteration 1",
        "aggregate failed because current iteration shares were absent",
        "persisted Complete state blocked later current-iteration shares",
        "transactional control allowed current shares after stale failure rolled back",
    ],
)
