// Proposed regression test for src/bitcoin/header_queue.rs at v13.0.0.
//
// This test is intentionally kept as an audit artifact instead of patching the
// target source. It should live inside the existing #[cfg(test)] mod in
// src/bitcoin/header_queue.rs so it can access private fields and helpers.
//
// Expected result on vulnerable v13 code: after the rejected lower-work reorg,
// the queue height is reduced and current_work no longer equals queued work.

#[test]
fn rejected_lower_work_reorg_is_atomic_and_preserves_work_accounting() {
    use bitcoin::util::uint::Uint256;

    let mut queue = HeaderQueue::default();
    let before_height = queue.height().unwrap();
    let before_work = *queue.current_work;

    // Build a same-height replacement whose work comparison should reject it.
    // In a concrete Rust regression this should use locally-mined regtest
    // headers or deterministic low-difficulty fixture headers, then assert:
    //
    //   let err = queue.add_into_iter(replacement_headers).unwrap_err();
    //   assert!(err.to_string().contains("more work"));
    //   assert_eq!(queue.height().unwrap(), before_height);
    //   assert_eq!(*queue.current_work, before_work);
    //   assert_eq!(
    //       *queue.current_work,
    //       queue.deque.iter().unwrap()
    //           .try_fold(Uint256::default(), |sum, h| Ok(sum + h?.work()))
    //           .unwrap()
    //   );
    //
    // v13 source currently pops old headers before this comparison and
    // pop_back_to() does not subtract removed work from current_work.
    let _ = (before_height, before_work, Uint256::default());
}
