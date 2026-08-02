use nomic_bridge_harness::retry::{run, RetryBudget, RetryClass, RetryOutcome};
use std::time::{Duration, Instant};

#[test]
fn absolute_retry_deadline_is_passed_through_exactly() {
    let deadline = Instant::now() + Duration::from_millis(100);
    let mut observed = None;
    let report = run(
        RetryBudget::until(1, deadline, Duration::from_millis(1)).unwrap(),
        |attempt| {
            observed = Some(attempt.deadline());
            Ok::<_, RetryClass<()>>(())
        },
    );
    report.into_result().unwrap();
    assert_eq!(observed, Some(deadline));
}

#[test]
fn transient_retries_have_an_exact_success_receipt() {
    let mut seen = Vec::new();
    let report = run(
        RetryBudget::new(4, Duration::from_millis(200), Duration::from_millis(1)).unwrap(),
        |attempt| {
            seen.push(attempt.number());
            if attempt.number() < 3 {
                Err(RetryClass::Transient("not-ready"))
            } else {
                Ok("ready")
            }
        },
    );
    assert_eq!(seen, [1, 2, 3]);
    assert_eq!(report.receipt().attempts(), 3);
    assert_eq!(report.receipt().outcome(), RetryOutcome::Succeeded);
    assert_eq!(report.into_result().unwrap(), "ready");
}

#[test]
fn permanent_failure_is_never_retried() {
    let mut calls = 0;
    let report = run(
        RetryBudget::new(8, Duration::from_millis(200), Duration::from_millis(1)).unwrap(),
        |_| {
            calls += 1;
            Err::<(), _>(RetryClass::Permanent("malformed"))
        },
    );
    assert_eq!(calls, 1);
    assert_eq!(report.receipt().attempts(), 1);
    assert_eq!(report.receipt().outcome(), RetryOutcome::Permanent);
}

#[test]
fn a_transient_result_after_the_deadline_cannot_turn_green() {
    let mut calls = 0;
    let report = run(
        RetryBudget::new(4, Duration::from_millis(20), Duration::from_millis(2)).unwrap(),
        |_| {
            calls += 1;
            std::thread::sleep(Duration::from_millis(30));
            if calls == 1 {
                Err(RetryClass::Transient("late"))
            } else {
                Ok("must-not-run")
            }
        },
    );
    assert_eq!(calls, 1);
    assert_eq!(report.receipt().outcome(), RetryOutcome::DeadlineExceeded);
    assert!(report.into_result().is_err());
}

#[test]
fn a_permanent_result_after_the_deadline_is_deadline_exceeded() {
    let report = run(
        RetryBudget::new(2, Duration::from_millis(20), Duration::from_millis(2)).unwrap(),
        |_| {
            std::thread::sleep(Duration::from_millis(30));
            Err::<(), _>(RetryClass::Permanent("late-permanent"))
        },
    );
    assert_eq!(report.receipt().attempts(), 1);
    assert_eq!(report.receipt().outcome(), RetryOutcome::DeadlineExceeded);
    assert!(matches!(
        report.into_result(),
        Err(nomic_bridge_harness::retry::RetryFailure::DeadlineExceeded(
            Some("late-permanent")
        ))
    ));
}

#[test]
fn receipt_contains_only_stable_monotonic_fields() {
    let report = run(
        RetryBudget::new(1, Duration::from_millis(50), Duration::from_millis(1)).unwrap(),
        |_| Ok::<_, RetryClass<&str>>(42),
    );
    let receipt = report.receipt();
    assert_eq!(receipt.attempts(), 1);
    assert_eq!(receipt.elapsed_millis(), receipt.elapsed().as_millis());
    let debug = format!("{receipt:?}");
    for forbidden in ["/home/", "127.0.0.1", "timestamp", "system_time"] {
        assert!(!debug.contains(forbidden));
    }
}

#[test]
fn retry_budgets_reject_zero_and_unbounded_values() {
    for result in [
        RetryBudget::new(0, Duration::from_millis(10), Duration::from_millis(1)),
        RetryBudget::new(1, Duration::ZERO, Duration::from_millis(1)),
        RetryBudget::new(1, Duration::from_millis(10), Duration::ZERO),
        RetryBudget::new(1, Duration::from_secs(3_601), Duration::from_millis(1)),
    ] {
        assert!(result.is_err());
    }
}
