#!/usr/bin/env python3
"""Local model for the legacy deposit commitment comparison bug.

The source loop in src/bitcoin/mod.rs currently recomputes the expected script
with `dest_bytes` inside the legacy loop instead of the loop variable `bytes`.
"""

from pathlib import Path

src = Path("src/bitcoin/mod.rs").read_text()
needle = "for bytes in legacy_commitments"
start = src.index(needle)
snippet = src[start : src.index("if !matched", start)]

print("Relevant source snippet:")
print(snippet)

current_commitment = "current-v0"
legacy_commitment = "legacy"
output_script = f"script({legacy_commitment})"

matched_current_code = False
dest_bytes = current_commitment
for bytes_ in [legacy_commitment]:
    expected_script = f"script({dest_bytes})"  # mirrors the current bug
    if output_script == expected_script:
        matched_current_code = True
        dest_bytes = bytes_
        break

matched_fixed_code = False
dest_bytes = current_commitment
for bytes_ in [legacy_commitment]:
    expected_script = f"script({bytes_})"  # intended comparison
    if output_script == expected_script:
        matched_fixed_code = True
        dest_bytes = bytes_
        break

print("Model result:")
print(f"  current code accepts legacy output: {matched_current_code}")
print(f"  fixed code accepts legacy output:   {matched_fixed_code}")
assert not matched_current_code and matched_fixed_code

