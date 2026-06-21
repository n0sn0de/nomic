#!/usr/bin/env python3
"""Executable model for v13 HeaderQueue reorg accounting.

The model mirrors the order in src/bitcoin/header_queue.rs:
- add_into_iter() calls pop_back_to(first.height) before verifying the candidate
  replacement headers.
- pop_back_to() returns removed work but does not subtract it from current_work.
- failed replacement work checks return an error after the old suffix is gone.
"""

from dataclasses import dataclass


@dataclass(frozen=True)
class Header:
    height: int
    work: int
    valid: bool = True


class HeaderQueue:
    def __init__(self, work_by_height):
        self.deque = [Header(h, w) for h, w in work_by_height]
        self.current_work = sum(h.work for h in self.deque)

    def height(self):
        return self.deque[-1].height

    def pop_back_to_v13(self, height):
        removed = 0
        while self.height() >= height:
            header = self.deque.pop()
            removed += header.work
        return removed

    def verify_and_add_headers_v13(self, headers):
        added = 0
        for header in headers:
            if not header.valid:
                raise ValueError("invalid replacement header")
            added += header.work
            chain_work = self.current_work + header.work
            self.deque.append(Header(header.height, header.work))
            self.current_work = chain_work
        return added

    def add_into_iter_v13(self, headers):
        current_height = self.height()
        first = headers[0]
        removed_work = 0
        if first.height <= current_height:
            removed_work = self.pop_back_to_v13(first.height)

        added_work = self.verify_and_add_headers_v13(headers)
        if added_work <= removed_work:
            raise ValueError("new best chain must include more work")

    def invariant_current_work_matches_queue(self):
        return self.current_work == sum(h.work for h in self.deque)


def main():
    # Trusted header 100 plus three accepted headers.
    q = HeaderQueue([(100, 100), (101, 10), (102, 10), (103, 10)])
    before_height = q.height()
    before_work = q.current_work

    # A lower-work reorg starts at height 102. Source rejects it only after
    # pop_back_to has removed heights 102 and 103.
    try:
        q.add_into_iter_v13([Header(102, 1), Header(103, 1)])
    except ValueError as err:
        outcome = str(err)
    else:
        raise AssertionError("model expected replacement to be rejected")

    print("outcome:", outcome)
    print("before_height:", before_height)
    print("after_height:", q.height())
    print("before_current_work:", before_work)
    print("after_current_work:", q.current_work)
    print("queue_work_sum:", sum(h.work for h in q.deque))
    print("work_invariant_holds:", q.invariant_current_work_matches_queue())

    assert q.height() == before_height, "replacement suffix is left in place after rejection"
    assert q.current_work == before_work + 2, "added replacement work remains counted"
    assert not q.invariant_current_work_matches_queue()


if __name__ == "__main__":
    main()
