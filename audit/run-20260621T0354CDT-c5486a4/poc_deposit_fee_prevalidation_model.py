#!/usr/bin/env python3

"""
Local model for the deposit pre-validation accounting mismatch.

The source-level issue is in src/bitcoin/mod.rs:

- amount_after_deposit_fee checks that the miner-fee subtraction would not
  underflow, but does not assign the result back to amount.
- InnerApp::relay_deposit validates the destination using that overstated
  amount.
- Bitcoin::relay_deposit later subtracts the miner fee before storing the
  pending destination.

For amount-sensitive destinations such as Dest::Bitcoin, there is therefore a
non-empty boundary range where pre-validation accepts a deposit, but the later
actual destination credit fails after the deposit has already become a pending
checkpoint transfer.
"""

UNITS_PER_SAT = 1_000_000
FEE_RATE = 10
USER_FEE_FACTOR = 21_000
WITHDRAW_SCRIPT_LEN = 22
MIN_WITHDRAWAL_SATS = 600


def bridge_fee(usats: int) -> int:
    return usats // 100


def withdrawal_fee_usats(script_len: int = WITHDRAW_SCRIPT_LEN) -> int:
    return (9 + script_len) * FEE_RATE * USER_FEE_FACTOR // 10_000 * UNITS_PER_SAT


def withdrawal_accepts(usats: int) -> bool:
    fee = withdrawal_fee_usats()
    if fee > usats:
        return False
    value_sats = (usats - fee) // UNITS_PER_SAT
    return value_sats >= MIN_WITHDRAWAL_SATS


def prevalidated_amount_usats(output_sats: int, deposit_input_vbytes: int) -> int | None:
    amount = output_sats * UNITS_PER_SAT
    miner_fee = deposit_input_vbytes * FEE_RATE * USER_FEE_FACTOR // 10_000 * UNITS_PER_SAT
    if amount < miner_fee:
        return None

    # Current bug: checked_sub(miner_fee) is not assigned.
    return amount - bridge_fee(amount)


def actual_pending_amount_usats(output_sats: int, deposit_input_vbytes: int) -> int | None:
    amount = output_sats * UNITS_PER_SAT
    miner_fee = deposit_input_vbytes * FEE_RATE * USER_FEE_FACTOR // 10_000 * UNITS_PER_SAT
    if amount < miner_fee:
        return None
    amount -= miner_fee
    amount -= bridge_fee(amount)
    return amount


def main() -> None:
    print("withdrawal_fee_usats", withdrawal_fee_usats())
    print("withdrawal_min_total_usats", withdrawal_fee_usats() + MIN_WITHDRAWAL_SATS * UNITS_PER_SAT)

    # Even a small single-input deposit miner fee creates a reachable window.
    # Real signatory scripts can be significantly larger than this model.
    for deposit_input_vbytes in (50, 100, 200, 500):
        examples = []
        for output_sats in range(600, 10_000):
            pre = prevalidated_amount_usats(output_sats, deposit_input_vbytes)
            actual = actual_pending_amount_usats(output_sats, deposit_input_vbytes)
            if pre is None or actual is None:
                continue
            if withdrawal_accepts(pre) and not withdrawal_accepts(actual):
                examples.append((output_sats, pre, actual))
                if len(examples) == 3:
                    break
        print("deposit_input_vbytes", deposit_input_vbytes, "examples", examples)


if __name__ == "__main__":
    main()
