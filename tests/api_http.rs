pub mod support;

use std::net::SocketAddr;

use sqlx::PgPool;
use support::generator::Generator;
use support::{configure_db, setup_settings};
use tachikoma::{db::Registry, ldap::UsersInfo, web::Application};

async fn spawn_app() -> (SocketAddr, PgPool) {
    let configuration = setup_settings();
    let pool = configure_db(&configuration.database).await;
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
    (addr, pool)
}

#[tokio::test]
async fn hosts_leased_remains_public_without_auth() {
    let (addr, _pool) = spawn_app().await;
    let response = reqwest::Client::new()
        .get(format!("http://{addr}/hosts/leased"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn hosts_leased_includes_group_and_filters_by_it() {
    let (addr, pool) = spawn_app().await;
    let mut generator = Generator { pool: pool.clone() };
    let group_a = generator.generate_group().await;
    let group_b = generator.generate_group().await;
    let host_a = generator.generate_host_in_group(&group_a.id).await;
    let host_b = generator.generate_host_in_group(&group_b.id).await;
    let free = generator.generate_host_in_group(&group_a.id).await;
    let user = generator.generate_user().await;

    sqlx::query(
        "UPDATE hosts SET user_id = $1, leased_until = now() + interval '1 hour' WHERE id = ANY($2)",
    )
    .bind(user.id.0)
    .bind(vec![host_a.id.0, host_b.id.0])
    .execute(&pool)
    .await
    .unwrap();

    let client = reqwest::Client::new();
    let all = client
        .get(format!("http://{addr}/hosts/leased?mail={}", user.email))
        .send()
        .await
        .unwrap();
    assert_eq!(all.status().as_u16(), 200);
    let all: Vec<serde_json::Value> = serde_json::from_str(&all.text().await.unwrap()).unwrap();
    assert_eq!(all.len(), 2);
    for host in &all {
        assert!(host.get("lease_info").is_some());
        assert!(host["group"]["id"].is_number());
        assert!(host["group"]["name"].is_string());
    }

    let filtered = client
        .get(format!(
            "http://{addr}/hosts/leased?mail={}&group_id={}",
            user.email, group_a.id.0
        ))
        .send()
        .await
        .unwrap();
    let filtered: Vec<serde_json::Value> =
        serde_json::from_str(&filtered.text().await.unwrap()).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["hostname"], host_a.hostname);
    assert_eq!(
        filtered[0]["group"]["id"].as_i64(),
        Some(group_a.id.0 as i64)
    );
    assert_eq!(filtered[0]["group"]["name"], group_a.name);
    assert_eq!(filtered[0]["lease_info"]["leased_by"], user.email);

    let by_group = client
        .get(format!(
            "http://{addr}/hosts/leased?group_id={}",
            group_b.id.0
        ))
        .send()
        .await
        .unwrap();
    let by_group: Vec<serde_json::Value> =
        serde_json::from_str(&by_group.text().await.unwrap()).unwrap();
    assert_eq!(by_group.len(), 1);
    assert_eq!(by_group[0]["hostname"], host_b.hostname);
    assert_eq!(
        by_group[0]["group"]["id"].as_i64(),
        Some(group_b.id.0 as i64)
    );
    assert_eq!(by_group[0]["group"]["name"], group_b.name);

    let group_a_hosts = client
        .get(format!(
            "http://{addr}/hosts/leased?group_id={}",
            group_a.id.0
        ))
        .send()
        .await
        .unwrap();
    let group_a_hosts: Vec<serde_json::Value> =
        serde_json::from_str(&group_a_hosts.text().await.unwrap()).unwrap();
    assert_eq!(group_a_hosts.len(), 2);
    let free_host = group_a_hosts
        .iter()
        .find(|host| host["hostname"] == free.hostname)
        .unwrap();
    assert!(free_host["lease_info"].is_null());
    assert_eq!(free_host["group"]["name"], group_a.name);

    let everyone = client
        .get(format!("http://{addr}/hosts/leased"))
        .send()
        .await
        .unwrap();
    let everyone: Vec<serde_json::Value> =
        serde_json::from_str(&everyone.text().await.unwrap()).unwrap();
    assert_eq!(everyone.len(), 3);
}

#[tokio::test]
async fn api_v1_rejects_missing_bearer_token() {
    let (addr, _pool) = spawn_app().await;
    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/groups"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn api_v1_rejects_invalid_bearer_token() {
    let (addr, _pool) = spawn_app().await;
    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/groups"))
        .header("Authorization", "Bearer tk_this_token_does_not_exist")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn mcp_rejects_missing_bearer_token() {
    let (addr, _pool) = spawn_app().await;
    let response = reqwest::Client::new()
        .post(format!("http://{addr}/mcp"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.0.1"}}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn mcp_rejects_invalid_bearer_token() {
    let (addr, _pool) = spawn_app().await;
    let response = reqwest::Client::new()
        .post(format!("http://{addr}/mcp"))
        .header("Authorization", "Bearer tk_this_token_does_not_exist")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.0.1"}}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 401);
}

#[test]
fn mcp_tool_router_registers_expected_tools() {
    let router = tachikoma::mcp::TachikomaMcp::tool_router();
    for name in [
        "list_groups",
        "list_available_hosts",
        "list_my_leased_hosts",
        "list_all_hosts",
        "lease_hosts",
        "lease_random_host",
        "release_hosts",
        "release_all_hosts",
    ] {
        assert!(router.has_route(name), "missing tool {name}");
    }
}
