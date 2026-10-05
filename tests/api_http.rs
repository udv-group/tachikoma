pub mod support;

use std::net::SocketAddr;

use support::{configure_db, setup_settings};
use tachikoma::{db::Registry, ldap::UsersInfo, web::Application};

async fn spawn_app() -> SocketAddr {
    let configuration = setup_settings();
    configure_db(&configuration.database).await;
    let registry = Registry::new(&configuration.database).await.unwrap();
    let users_info = UsersInfo::new(configuration.ldap.clone()).await.unwrap();
    let app = Application::build(&configuration, registry, users_info, None)
        .await
        .unwrap();
    let addr = app.listening_addr();
    tokio::spawn(async move {
        app.serve_forever().await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    addr
}

#[tokio::test]
async fn hosts_leased_remains_public_without_auth() {
    let addr = spawn_app().await;
    let response = reqwest::Client::new()
        .get(format!("http://{addr}/hosts/leased"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn api_v1_rejects_missing_bearer_token() {
    let addr = spawn_app().await;
    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/groups"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn api_v1_rejects_invalid_bearer_token() {
    let addr = spawn_app().await;
    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/groups"))
        .header("Authorization", "Bearer tk_this_token_does_not_exist")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 401);
}
