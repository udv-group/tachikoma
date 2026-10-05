use std::collections::HashMap;
use std::sync::Arc;

use axum_login::AuthUser;
use chrono::TimeDelta;
use http::request::Parts;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::wrapper::Parameters,
    model::*,
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::Deserialize;

use crate::db::models::{GroupId, HostId, User as UserDb, UserId};
use crate::logic::groups::GroupsService;
use crate::logic::hosts::{HostError, HostsService};
use crate::logic::users::UsersService;
use crate::web::auth::middleware::User;
use crate::web::templates::{GroupInfo, HostInfo};

#[derive(Clone)]
pub struct TachikomaMcp {
    hosts_service: HostsService,
    groups_service: GroupsService,
    users_service: UsersService,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupIdArgs {
    pub group_id: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LeaseHostsArgs {
    #[serde(default)]
    pub hosts_ids: Vec<i32>,
    pub days: u8,
    pub hours: u8,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LeaseRandomArgs {
    pub group_id: i32,
    pub days: u8,
    pub hours: u8,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReleaseHostsArgs {
    pub hosts_ids: Vec<i32>,
}

fn lease_duration(days: u8, hours: u8) -> Result<TimeDelta, McpError> {
    if days > 63 {
        return Err(McpError::invalid_params(
            "days must be between 0 and 63",
            None,
        ));
    }
    if hours > 23 {
        return Err(McpError::invalid_params(
            "hours must be between 0 and 23",
            None,
        ));
    }
    Ok(TimeDelta::hours(i64::from(hours) + i64::from(days) * 24))
}

fn host_error(err: HostError) -> McpError {
    McpError::internal_error(err.to_string(), None)
}

fn json_result<T: serde::Serialize>(value: T) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![ContentBlock::json(value)?]))
}

fn require_user(ctx: &RequestContext<rmcp::RoleServer>) -> Result<User, McpError> {
    let parts = ctx.extensions.get::<Parts>().ok_or_else(|| {
        McpError::internal_error("missing HTTP request parts in MCP context", None)
    })?;
    parts
        .extensions
        .get::<User>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("Unauthorized", None))
}

impl TachikomaMcp {
    pub fn new(
        hosts_service: HostsService,
        groups_service: GroupsService,
        users_service: UsersService,
    ) -> Self {
        Self {
            hosts_service,
            groups_service,
            users_service,
        }
    }
}

#[tool_router(vis = "pub")]
impl TachikomaMcp {
    #[tool(description = "List all host groups")]
    async fn list_groups(&self) -> Result<CallToolResult, McpError> {
        let groups = self
            .groups_service
            .get_all_groups()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        json_result(groups.into_iter().map(GroupInfo::from).collect::<Vec<_>>())
    }

    #[tool(description = "List available (unleased) hosts in a group")]
    async fn list_available_hosts(
        &self,
        Parameters(args): Parameters<GroupIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let hosts = self
            .hosts_service
            .get_available_group_hosts(&GroupId(args.group_id))
            .await
            .map_err(host_error)?;
        json_result(hosts.into_iter().map(HostInfo::from).collect::<Vec<_>>())
    }

    #[tool(description = "List hosts currently leased by the authenticated user")]
    async fn list_my_leased_hosts(
        &self,
        ctx: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = require_user(&ctx)?;
        let hosts = self
            .hosts_service
            .get_leased_hosts(&user.id().into())
            .await
            .map_err(host_error)?;
        json_result(hosts.into_iter().map(HostInfo::from).collect::<Vec<_>>())
    }

    #[tool(description = "List all hosts with lease information")]
    async fn list_all_hosts(&self) -> Result<CallToolResult, McpError> {
        let users: HashMap<UserId, UserDb> = self
            .users_service
            .get_all_users()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();
        let hosts = self
            .hosts_service
            .get_all_hosts()
            .await
            .map_err(host_error)?;
        json_result(
            hosts
                .into_iter()
                .map(|host| {
                    let user = host
                        .user_id
                        .and_then(|user_id| users.get(&user_id).cloned());
                    HostInfo::from((host, user))
                })
                .collect::<Vec<_>>(),
        )
    }

    #[tool(description = "Lease specific hosts for the authenticated user")]
    async fn lease_hosts(
        &self,
        Parameters(args): Parameters<LeaseHostsArgs>,
        ctx: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = require_user(&ctx)?;
        let duration = lease_duration(args.days, args.hours)?;
        let hosts_ids: Vec<HostId> = args.hosts_ids.into_iter().map(HostId).collect();
        let hosts = self
            .hosts_service
            .lease(&user.id().into(), &user.groups, &hosts_ids, duration)
            .await
            .map_err(host_error)?;
        json_result(hosts.into_iter().map(HostInfo::from).collect::<Vec<_>>())
    }

    #[tool(description = "Lease a random available host from a group")]
    async fn lease_random_host(
        &self,
        Parameters(args): Parameters<LeaseRandomArgs>,
        ctx: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = require_user(&ctx)?;
        let duration = lease_duration(args.days, args.hours)?;
        let host = self
            .hosts_service
            .lease_random(
                &user.id().into(),
                &user.groups,
                duration,
                &GroupId(args.group_id),
            )
            .await
            .map_err(host_error)?;
        json_result(HostInfo::from(host))
    }

    #[tool(description = "Release specific hosts leased by the authenticated user")]
    async fn release_hosts(
        &self,
        Parameters(args): Parameters<ReleaseHostsArgs>,
        ctx: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = require_user(&ctx)?;
        let hosts_ids: Vec<HostId> = args.hosts_ids.into_iter().map(HostId).collect();
        self.hosts_service
            .free(&user.id().into(), &hosts_ids)
            .await
            .map_err(host_error)?;
        json_result(serde_json::json!({ "ok": true }))
    }

    #[tool(description = "Release all hosts leased by the authenticated user")]
    async fn release_all_hosts(
        &self,
        ctx: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = require_user(&ctx)?;
        self.hosts_service
            .free_all(&user.id().into())
            .await
            .map_err(host_error)?;
        json_result(serde_json::json!({ "ok": true }))
    }
}

#[tool_handler]
impl ServerHandler for TachikomaMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_instructions(
            "Tachikoma host leasing tools. Authenticate with Authorization: Bearer <api-token>. \
             Tools mirror the /api/v1 JSON API for groups, hosts, lease, and release."
                .to_string(),
        )
    }
}

pub fn streamable_http_service(
    hosts_service: HostsService,
    groups_service: GroupsService,
    users_service: UsersService,
) -> StreamableHttpService<TachikomaMcp, LocalSessionManager> {
    let hosts_service = hosts_service;
    let groups_service = groups_service;
    let users_service = users_service;
    StreamableHttpService::new(
        move || {
            Ok(TachikomaMcp::new(
                hosts_service.clone(),
                groups_service.clone(),
                users_service.clone(),
            ))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    )
}
