use axum::{
    Extension, Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use axum_login::AuthUser;
use chrono::TimeDelta;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::auth::middleware::User;
use super::templates::HostInfo;
use crate::db::models::{GroupId, HostId, User as UserDb, UserId};
use crate::logic::groups::GroupsService;
use crate::logic::hosts::{HostError, HostsService};
use crate::logic::users::UsersService;
use crate::web::templates::GroupInfo;

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn host_error_response(err: HostError) -> axum::response::Response {
    let status = match &err {
        HostError::ThereIsNoFreeHosts | HostError::AlreadyLeased(_) | HostError::LeaseLimit => {
            StatusCode::CONFLICT
        }
        HostError::DatabaseError(_) | HostError::UnexpectedError(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (
        status,
        Json(ErrorBody {
            error: err.to_string(),
        }),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct AvailableHostsQuery {
    pub group_id: GroupId,
}

pub async fn list_groups(State(groups_service): State<GroupsService>) -> impl IntoResponse {
    match groups_service.get_all_groups().await {
        Ok(groups) => {
            Json(groups.into_iter().map(GroupInfo::from).collect::<Vec<_>>()).into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorBody {
                error: err.to_string(),
            }),
        )
            .into_response(),
    }
}

pub async fn list_available_hosts(
    Query(query): Query<AvailableHostsQuery>,
    State(hosts_service): State<HostsService>,
) -> impl IntoResponse {
    match hosts_service
        .get_available_group_hosts(&query.group_id)
        .await
    {
        Ok(hosts) => {
            Json(hosts.into_iter().map(HostInfo::from).collect::<Vec<_>>()).into_response()
        }
        Err(err) => host_error_response(err),
    }
}

pub async fn list_my_leased_hosts(
    State(hosts_service): State<HostsService>,
    Extension(user): Extension<User>,
) -> impl IntoResponse {
    match hosts_service.get_leased_hosts(&user.id().into()).await {
        Ok(hosts) => {
            Json(hosts.into_iter().map(HostInfo::from).collect::<Vec<_>>()).into_response()
        }
        Err(err) => host_error_response(err),
    }
}

pub async fn list_all_hosts(
    State(hosts_service): State<HostsService>,
    State(user_service): State<UsersService>,
) -> impl IntoResponse {
    let users: HashMap<UserId, UserDb> = match user_service.get_all_users().await {
        Ok(users) => users.into_iter().map(|u| (u.id, u)).collect(),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody {
                    error: err.to_string(),
                }),
            )
                .into_response();
        }
    };

    match hosts_service.get_all_hosts().await {
        Ok(hosts) => Json(
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
        .into_response(),
        Err(err) => host_error_response(err),
    }
}

#[derive(Deserialize)]
pub struct LeaseRequest {
    #[serde(default)]
    hosts_ids: Vec<HostId>,
    days: u8,
    hours: u8,
}

fn lease_duration(days: u8, hours: u8) -> Result<TimeDelta, String> {
    if days > 63 {
        return Err("days must be between 0 and 63".to_string());
    }
    if hours > 23 {
        return Err("hours must be between 0 and 23".to_string());
    }
    Ok(TimeDelta::hours(i64::from(hours) + i64::from(days) * 24))
}

pub async fn lease_hosts(
    State(hosts_service): State<HostsService>,
    Extension(user): Extension<User>,
    Json(body): Json<LeaseRequest>,
) -> impl IntoResponse {
    let duration = match lease_duration(body.days, body.hours) {
        Ok(d) => d,
        Err(msg) => {
            return (StatusCode::BAD_REQUEST, Json(ErrorBody { error: msg })).into_response();
        }
    };

    match hosts_service
        .lease(&user.id().into(), &user.groups, &body.hosts_ids, duration)
        .await
    {
        Ok(hosts) => {
            Json(hosts.into_iter().map(HostInfo::from).collect::<Vec<_>>()).into_response()
        }
        Err(err) => host_error_response(err),
    }
}

#[derive(Deserialize)]
pub struct LeaseRandomRequest {
    group_id: GroupId,
    days: u8,
    hours: u8,
}

pub async fn lease_random(
    State(hosts_service): State<HostsService>,
    Extension(user): Extension<User>,
    Json(body): Json<LeaseRandomRequest>,
) -> impl IntoResponse {
    let duration = match lease_duration(body.days, body.hours) {
        Ok(d) => d,
        Err(msg) => {
            return (StatusCode::BAD_REQUEST, Json(ErrorBody { error: msg })).into_response();
        }
    };

    match hosts_service
        .lease_random(&user.id().into(), &user.groups, duration, &body.group_id)
        .await
    {
        Ok(host) => Json(HostInfo::from(host)).into_response(),
        Err(err) => host_error_response(err),
    }
}

#[derive(Deserialize)]
pub struct ReleaseRequest {
    hosts_ids: Vec<HostId>,
}

pub async fn release_hosts(
    State(hosts_service): State<HostsService>,
    Extension(user): Extension<User>,
    Json(body): Json<ReleaseRequest>,
) -> impl IntoResponse {
    match hosts_service.free(&user.id().into(), &body.hosts_ids).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => host_error_response(err),
    }
}

pub async fn release_all(
    State(hosts_service): State<HostsService>,
    Extension(user): Extension<User>,
) -> impl IntoResponse {
    match hosts_service.free_all(&user.id().into()).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => host_error_response(err),
    }
}
