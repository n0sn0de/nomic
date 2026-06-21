#!/usr/bin/env python3
"""Local model for Nomic signatory quorum/threshold arithmetic.

This does not interact with any network.  It mirrors:
  src/bitcoin/signatory.rs::has_quorum/signature_threshold
  src/bitcoin/threshold_sig.rs::signed
"""

from math import floor


def accepted_sigset(possible_vp: int, present_vp: int) -> bool:
    return present_vp >= possible_vp // 2


def spend_threshold(present_vp: int, ratio: tuple[int, int]) -> int:
    n, d = ratio
    return floor(present_vp * n / d)


def can_spend(signed_vp: int, present_vp: int, ratio: tuple[int, int]) -> bool:
    # Code and Bitcoin script use strict greater-than.
    return signed_vp > spend_threshold(present_vp, ratio)


cases = [
    ("mainnet 2/3, one of two equal-power validators publishes xpub", 2, 1, (2, 3)),
    ("mainnet 2/3, 51% of validator power publishes xpub", 100, 51, (2, 3)),
    ("testnet 9/10, 51% of validator power publishes xpub", 100, 51, (9, 10)),
    ("mainnet 2/3, all 3 equal-power validators publish xpub", 3, 3, (2, 3)),
]

for label, possible, present, ratio in cases:
    threshold = spend_threshold(present, ratio)
    min_sign = threshold + 1
    print(label)
    print(f"  accepted_sigset={accepted_sigset(possible, present)}")
    print(f"  possible_vp={possible} present_vp={present} ratio={ratio[0]}/{ratio[1]}")
    print(f"  strict threshold is signed_vp > {threshold}; minimum signed_vp={min_sign}")
    print(f"  minimum signed_vp as share of possible_vp={min_sign / possible:.2%}")
    print()

