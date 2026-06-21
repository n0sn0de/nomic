#!/usr/bin/env python3

"""
Local model for the header retarget timestamp underflow.

Nomic's calculate_next_target computes:

    let mut timespan = header.time() - prev_retarget;

where `header` is the previous block at a retarget boundary and
`prev_retarget` is the first block in that 2016-block period. Bitcoin's MTP
rule does not require timestamps to be monotonic across the whole period.

This script constructs a sequence that satisfies the local MTP rule used by
src/bitcoin/header_queue.rs while ending the period below the first timestamp.
With Rust overflow checks enabled, the subtraction panics; without them it wraps
to a huge u32 before clamping.
"""

RETARGET_INTERVAL = 2016


def mtp_ok(times: list[int], candidate: int) -> bool:
    if len(times) < 11:
        return True
    prev = sorted(times[-11:])
    return candidate > prev[5]


def main() -> None:
    # Eleven low pre-period timestamps followed by a high retarget-start
    # timestamp. The high timestamp passes MTP because the previous median is
    # low.
    history = [1_000] * 11
    period = []
    first_retarget_time = 10_000
    assert mtp_ok(history, first_retarget_time)
    history.append(first_retarget_time)
    period.append(first_retarget_time)

    # Build the remaining period with the smallest timestamp satisfying MTP.
    # Once the high value ages out of the 11-block window, timestamps remain far
    # below the first retarget timestamp.
    while len(period) < RETARGET_INTERVAL:
        median = sorted(history[-11:])[5]
        candidate = median + 1
        assert mtp_ok(history, candidate)
        history.append(candidate)
        period.append(candidate)

    last_before_retarget = period[-1]
    print("first_retarget_time", first_retarget_time)
    print("last_before_retarget", last_before_retarget)
    print("last_less_than_first", last_before_retarget < first_retarget_time)

    try:
        if last_before_retarget < first_retarget_time:
            raise OverflowError("u32 subtraction would underflow in Rust with overflow checks")
        print("timespan", last_before_retarget - first_retarget_time)
    except OverflowError as exc:
        print("modeled_result", exc)


if __name__ == "__main__":
    main()
