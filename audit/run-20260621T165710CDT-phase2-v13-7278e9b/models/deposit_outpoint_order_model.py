#!/usr/bin/env python3
"""Executable model for relay_deposit outpoint marking order.

Mirrors src/bitcoin/mod.rs lines where processed_outpoints.insert() occurs
before checkpoint.deposits_enabled is checked.
"""


class Bridge:
    def __init__(self):
        self.processed = set()
        self.credited = 0

    def relay_deposit_v13(self, outpoint, deposits_enabled=True):
        if outpoint in self.processed:
            raise ValueError("Output has already been relayed")
        self.processed.add(outpoint)

        if not deposits_enabled:
            raise ValueError("Deposits are disabled for the given checkpoint")

        self.credited += 1


def main():
    bridge = Bridge()
    outpoint = ("txid", 0)

    try:
        bridge.relay_deposit_v13(outpoint, deposits_enabled=False)
    except ValueError as err:
        first_error = str(err)
    else:
        raise AssertionError("disabled deposit should be rejected")

    try:
        bridge.relay_deposit_v13(outpoint, deposits_enabled=True)
    except ValueError as err:
        second_error = str(err)
    else:
        raise AssertionError("model expected outpoint poisoning to block retry")

    print("first_error:", first_error)
    print("processed_after_first_error:", sorted(bridge.processed))
    print("second_error:", second_error)
    print("credited:", bridge.credited)

    assert first_error == "Deposits are disabled for the given checkpoint"
    assert second_error == "Output has already been relayed"
    assert bridge.credited == 0


if __name__ == "__main__":
    main()
