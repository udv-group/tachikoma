pub mod support;

use crate::support::registry::create_registry;
use tachikoma::logic::api_tokens::{ApiTokenError, ApiTokensService, hash_token};

#[tokio::test]
async fn create_token_stores_hash_and_returns_plaintext_once() {
    let (mut generator, registry) = create_registry().await;
    let user = generator.generate_user().await;
    let service = ApiTokensService::new(registry, 10);

    let created = service
        .create(&user.id, Some("laptop"))
        .await
        .expect("create token");

    assert!(created.plaintext.starts_with("tk_"));
    assert_eq!(created.token.name.as_deref(), Some("laptop"));

    let listed = service.list_for_user(&user.id).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_ne!(listed[0].token_hash, created.plaintext);
    assert_eq!(listed[0].token_hash, hash_token(&created.plaintext));
}

#[tokio::test]
async fn find_user_id_by_plaintext_and_revoke() {
    let (mut generator, registry) = create_registry().await;
    let user = generator.generate_user().await;
    let other = generator.generate_user().await;
    let service = ApiTokensService::new(registry, 10);

    let created = service.create(&user.id, None).await.unwrap();

    let found = service
        .find_user_id_by_plaintext(&created.plaintext)
        .await
        .unwrap();
    assert_eq!(found, Some(user.id));

    let revoke_other = service.revoke(&created.token.id, &other.id).await;
    assert!(matches!(revoke_other, Err(ApiTokenError::NotFound)));

    assert!(
        service
            .find_user_id_by_plaintext(&created.plaintext)
            .await
            .unwrap()
            .is_some()
    );

    service.revoke(&created.token.id, &user.id).await.unwrap();

    assert!(
        service
            .find_user_id_by_plaintext(&created.plaintext)
            .await
            .unwrap()
            .is_none()
    );
    assert!(service.list_for_user(&user.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn blank_name_stored_as_none() {
    let (mut generator, registry) = create_registry().await;
    let user = generator.generate_user().await;
    let service = ApiTokensService::new(registry, 10);

    let created = service.create(&user.id, Some("   ")).await.unwrap();
    assert!(created.token.name.is_none());
}

#[tokio::test]
async fn create_rejects_when_token_limit_reached() {
    let (mut generator, registry) = create_registry().await;
    let user = generator.generate_user().await;
    let limit = 3usize;
    let service = ApiTokensService::new(registry, limit);

    for i in 0..limit {
        service
            .create(&user.id, Some(&format!("token-{i}")))
            .await
            .expect("create within limit");
    }

    let over = service.create(&user.id, Some("over-limit")).await;
    assert!(matches!(over, Err(ApiTokenError::LimitExceeded)));

    let listed = service.list_for_user(&user.id).await.unwrap();
    assert_eq!(listed.len(), limit);

    service
        .revoke(&listed[0].id, &user.id)
        .await
        .expect("revoke one");

    service
        .create(&user.id, Some("after-revoke"))
        .await
        .expect("create after revoke frees a slot");

    assert_eq!(service.list_for_user(&user.id).await.unwrap().len(), limit);
}
