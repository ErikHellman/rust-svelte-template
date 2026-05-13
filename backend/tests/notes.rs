mod common;

use backend::notes::repo;

#[tokio::test]
async fn crud_lifecycle() {
    let (state, _tmp) = common::test_state().await;
    let user = common::seed_user(&state, "user@example.com").await;

    assert!(repo::list(&state.db, &user).await.unwrap().is_empty());

    let created = repo::create(&state.db, &user, "first", "hello")
        .await
        .expect("create");
    assert_eq!(created.title, "first");
    assert_eq!(created.body, "hello");

    let listed = repo::list(&state.db, &user).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);

    let updated = repo::update(&state.db, &user, &created.id, "first edited", "world")
        .await
        .expect("update")
        .expect("present");
    assert_eq!(updated.title, "first edited");
    assert_eq!(updated.body, "world");

    let deleted = repo::delete(&state.db, &user, &created.id)
        .await
        .expect("delete");
    assert!(deleted);
    assert!(repo::list(&state.db, &user).await.unwrap().is_empty());
}

#[tokio::test]
async fn notes_are_user_scoped() {
    let (state, _tmp) = common::test_state().await;
    let alice = common::seed_user(&state, "alice@example.com").await;
    let bob = common::seed_user(&state, "bob@example.com").await;

    let alice_note = repo::create(&state.db, &alice, "alice's", "secret")
        .await
        .expect("create");

    let bob_view = repo::get(&state.db, &bob, &alice_note.id).await.unwrap();
    assert!(bob_view.is_none(), "bob must not see alice's note");

    let bob_delete = repo::delete(&state.db, &bob, &alice_note.id).await.unwrap();
    assert!(!bob_delete, "bob must not delete alice's note");

    // alice still has it
    let alice_view = repo::get(&state.db, &alice, &alice_note.id)
        .await
        .unwrap()
        .expect("present");
    assert_eq!(alice_view.id, alice_note.id);
}
