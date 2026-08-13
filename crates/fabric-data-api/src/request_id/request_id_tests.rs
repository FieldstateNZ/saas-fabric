//! Scoping the correlation id to one request.

use super::*;

#[test]
fn outside_a_request_the_current_id_is_a_safe_marker() {
    assert_eq!(current(), "no-request-context");
}

#[tokio::test]
async fn the_scoped_id_is_readable_for_the_life_of_the_future() {
    let seen = CURRENT.scope("scoped-id".to_owned(), async { current() }).await;

    assert_eq!(seen, "scoped-id");
}
