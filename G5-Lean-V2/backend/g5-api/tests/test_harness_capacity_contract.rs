//! Regression contract for the shared TestApp connection-budget guard.

use crate::helpers;

use helpers::{
    acquire_test_app_capacity, try_acquire_test_app_capacity, TestApp, TEST_APP_MAX_CONCURRENCY,
};
use tokio::sync::TryAcquireError;

#[tokio::test]
async fn test_app_capacity_blocks_excess_fanout_and_survives_implicit_cleanup() {
    let mut held = Vec::with_capacity(TEST_APP_MAX_CONCURRENCY);
    for _ in 0..TEST_APP_MAX_CONCURRENCY {
        held.push(acquire_test_app_capacity().await);
    }

    assert!(
        matches!(
            try_acquire_test_app_capacity(),
            Err(TryAcquireError::NoPermits)
        ),
        "a seventh in-process TestApp must have no connection-budget capacity"
    );
    drop(held);

    let mut reserved = Vec::with_capacity(TEST_APP_MAX_CONCURRENCY - 1);
    for _ in 0..(TEST_APP_MAX_CONCURRENCY - 1) {
        reserved.push(acquire_test_app_capacity().await);
    }

    let app = TestApp::spawn().await;
    drop(app); // Exercise panic/omitted-cleanup fallback instead of cleanup().

    let recovered = acquire_test_app_capacity().await;

    drop(recovered);
    drop(reserved);
}
