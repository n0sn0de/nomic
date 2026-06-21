#!/usr/bin/env python3
"""Model v13 signatory threshold arithmetic.

Sources:
- src/bitcoin/signatory.rs signature_threshold(): floor(present_vp * 2 / 3)
- src/bitcoin/signatory.rs has_quorum(): present_vp >= floor(possible_vp / 2)
- src/bitcoin/threshold_sig.rs signed(): signed > threshold
"""


def accepts_sigset(possible_vp, present_vp):
    return present_vp >= possible_vp // 2


def required_to_spend(present_vp, numerator=2, denominator=3):
    threshold = present_vp * numerator // denominator
    return threshold + 1


def main():
    cases = [
        (100, 50),
        (100, 51),
        (100, 67),
        (3, 3),
        (4, 2),
    ]
    for possible, present in cases:
        accepted = accepts_sigset(possible, present)
        spend = required_to_spend(present)
        print(
            f"possible={possible} present={present} accepted={accepted} "
            f"required_present_to_spend={spend} required_possible_pct={spend / possible:.2%}"
        )

    assert accepts_sigset(100, 50)
    assert required_to_spend(50) == 34
    assert required_to_spend(3) == 3


if __name__ == "__main__":
    main()
