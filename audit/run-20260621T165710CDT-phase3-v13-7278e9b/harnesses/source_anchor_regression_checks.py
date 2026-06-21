#!/usr/bin/env python3
"""Source-anchored regression checks for the highest-priority v13 leads.

These checks are intentionally conservative: they do not probe any public
network and fail if the specific vulnerable source ordering changes.
"""

from pathlib import Path


ROOT = Path("/home/nitro/repos/nomic-v13-audit-7278e9b")


def assert_order(path, first, second):
    text = (ROOT / path).read_text()
    a = text.index(first)
    b = text.index(second)
    assert a < b, f"{path}: expected {first!r} before {second!r}"


def assert_in_function_without(path, func_marker, expected, forbidden):
    text = (ROOT / path).read_text()
    start = text.index(func_marker)
    end = text.index("\n    ///", start + len(func_marker)) if "\n    ///" in text[start + len(func_marker):] else len(text)
    body = text[start:end]
    assert expected in body, f"{path}: missing {expected!r}"
    assert forbidden not in body, f"{path}: unexpected {forbidden!r}"


def main():
    assert_order(
        "src/bitcoin/header_queue.rs",
        "removed_work = self.pop_back_to(first.height)?;",
        "let added_work = self.verify_and_add_headers(&headers)?;",
    )
    assert_in_function_without(
        "src/bitcoin/header_queue.rs",
        "fn pop_back_to(&mut self, height: u32) -> Result<Uint256>",
        "work = work + header.work();",
        "self.current_work =",
    )
    assert_order(
        "src/bitcoin/mod.rs",
        "self.processed_outpoints.insert(outpoint, deposit_timeout)?;",
        "if !checkpoint.deposits_enabled",
    )
    assert_order(
        "src/app.rs",
        "ibc.transfer_mut()\n            .mint_coins_execute",
        "if let Err(err) = ibc.deliver_message",
    )
    print("source anchor regression checks passed")


if __name__ == "__main__":
    main()
