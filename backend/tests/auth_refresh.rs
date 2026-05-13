mod common;

use backend::auth::session;

#[tokio::test]
async fn refresh_rotates_and_revokes_old() {
    let (state, _tmp) = common::test_state().await;
    let user_id = common::seed_user(&state, "rotate@example.com").await;

    let first = session::issue(&state.db, &user_id, 3600)
        .await
        .expect("issue");

    let rotated = session::rotate(&state.db, &first.cookie_value, 3600)
        .await
        .expect("rotate");
    assert_eq!(rotated.user_id, user_id);

    // Using the original cookie again must fail (token reuse detection).
    let reuse = session::rotate(&state.db, &first.cookie_value, 3600).await;
    assert!(
        reuse.is_err(),
        "old refresh must be rejected after rotation"
    );

    // The new cookie still works.
    let rotated_again = session::rotate(&state.db, &rotated.cookie_value, 3600)
        .await
        .expect("rotate new cookie");
    assert_eq!(rotated_again.user_id, user_id);
}

#[tokio::test]
async fn revoke_blocks_further_use() {
    let (state, _tmp) = common::test_state().await;
    let user_id = common::seed_user(&state, "revoke@example.com").await;

    let issued = session::issue(&state.db, &user_id, 3600)
        .await
        .expect("issue");
    session::revoke(&state.db, &issued.cookie_value)
        .await
        .expect("revoke");
    let rotated = session::rotate(&state.db, &issued.cookie_value, 3600).await;
    assert!(rotated.is_err(), "revoked refresh must not rotate");
}
