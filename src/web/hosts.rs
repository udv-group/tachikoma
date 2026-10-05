use askama::Template;
use axum::{
    Extension,
    extract::{Query, State},
    response::{Html, IntoResponse, Json, Redirect},
};
use axum_extra::extract::{CookieJar, Form, OptionalQuery};
use axum_flash::{Flash, IncomingFlashes};
use axum_login::AuthUser;
use chrono::TimeDelta;
use std::{collections::HashMap, ops::Deref};

use serde::Deserialize;

use super::templates::{AllHostsPage, HostInfo, HostsLeasePage, HostsPage};
use crate::{AppInfo, logic::users::UsersService};
use crate::{db::models::UserId, logic::hosts::HostsService};
use crate::{
    db::models::{GroupId, HostId, User as UserDb},
    logic::groups::GroupsService,
};

use super::AuthLink;
use super::auth::middleware::User;
use axum_extra::extract::cookie::Cookie;
#[derive(Deserialize)]
pub struct HostsParams {
    pub group_id: Option<GroupId>,
}

pub async fn get_hosts(
    params: Query<HostsParams>,
    State(hosts_service): State<HostsService>,
    State(groups_service): State<GroupsService>,
    State(AuthLink(auth_link)): State<AuthLink>,
    flashes: IncomingFlashes,
    Extension(user): Extension<User>,
    jar: CookieJar,
) -> impl IntoResponse {
    let groups = groups_service.get_all_groups().await.unwrap();
    let group_id = params.group_id.unwrap_or_else(|| {
        jar.get("group_id").map_or_else(
            || groups[0].id,
            |cookie| cookie.value().parse::<GroupId>().unwrap_or(groups[0].id),
        )
    });

    let selected_group = groups
        .iter()
        .find(|group| group.id == group_id)
        .unwrap_or(&groups[0])
        .clone();
    let hosts = hosts_service
        .get_available_group_hosts(&selected_group.id)
        .await
        .unwrap();

    let leased = hosts_service
        .get_leased_hosts(&user.id().into())
        .await
        .unwrap();

    let error = flashes.into_iter().next().map(|(_, err)| err.to_owned());
    let lease_page = HostsLeasePage {
        groups: groups.into_iter().map(|g| g.into()).collect(),
        selected_group: selected_group.into(),
        hosts: hosts.into_iter().map(|h| h.into()).collect(),
        leased: leased.into_iter().map(|h| h.into()).collect(),
        error,
    };
    let page = HostsPage {
        user: user.into(),
        auth_link,
        page: lease_page,
        app_info: AppInfo::new(),
    };

    (
        jar.add(Cookie::new("group_id", group_id.to_string())),
        flashes,
        Html(page.render().unwrap()),
    )
}
pub async fn get_all_hosts(
    State(hosts_service): State<HostsService>,
    State(user_service): State<UsersService>,
    State(AuthLink(auth_link)): State<AuthLink>,
    Extension(user): Extension<User>,
) -> impl IntoResponse {
    let users: HashMap<UserId, UserDb> = user_service
        .get_all_users()
        .await
        .unwrap()
        .into_iter()
        .map(|u| (u.id, u))
        .collect();
    let hosts = hosts_service.get_all_hosts().await.unwrap();

    let lease_page = AllHostsPage {
        hosts: hosts
            .into_iter()
            .map(|h| {
                let user = h.user_id.and_then(|user_id| users.get(&user_id).cloned());
                (h, user).into()
            })
            .collect(),
    };
    let page = HostsPage {
        user: user.into(),
        auth_link,
        page: lease_page,
        app_info: AppInfo::new(),
    };

    Html(page.render().unwrap())
}

#[derive(Deserialize)]
#[serde(try_from = "String")]
pub struct Days(pub i64);

impl Deref for Days {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for Days {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.parse::<u8>() {
            Ok(v @ 0..=63) => Ok(Self(v as i64)),
            Ok(_) => Err("Value must be between 0 and 63".to_string()),
            Err(_) => Err(format!("Wrong value {value}, can not parse as u8")),
        }
    }
}

#[derive(Deserialize)]
#[serde(try_from = "String")]
pub struct Hours(pub i64);

impl Deref for Hours {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for Hours {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.parse::<u8>() {
            Ok(v @ 0..=23) => Ok(Self(v as i64)),
            Ok(_) => Err("Value must be between 0 and 23".to_string()),
            Err(_) => Err(format!("Wrong value {value}, can not parse as u8")),
        }
    }
}
#[derive(Deserialize)]
pub struct LeaseForm {
    days: Days,
    hours: Hours,
    #[serde(default)]
    hosts_ids: Vec<HostId>,
}

pub async fn lease_hosts(
    State(service): State<HostsService>,
    flash: Flash,
    Extension(user): Extension<User>,
    Form(data): Form<LeaseForm>,
) -> impl IntoResponse {
    let res = service
        .lease(
            &user.id().into(),
            &user.groups,
            &data.hosts_ids,
            TimeDelta::hours(*data.hours + *data.days * 24),
        )
        .await;
    match res {
        Ok(_) => Redirect::to("/hosts").into_response(),
        Err(e) => (flash.error(e.to_string()), Redirect::to("/hosts")).into_response(),
    }
}

#[derive(Deserialize)]
pub struct LeaseRandomHostParams {
    pub group_id: GroupId,
}

pub async fn lease_random(
    params: Query<LeaseRandomHostParams>,
    State(service): State<HostsService>,
    flash: Flash,
    Extension(user): Extension<User>,
    Form(data): Form<LeaseForm>,
) -> impl IntoResponse {
    let res = service
        .lease_random(
            &user.id().into(),
            &user.groups,
            TimeDelta::hours(*data.hours + *data.days * 24),
            &params.group_id,
        )
        .await;
    match res {
        Ok(_) => Redirect::to("/hosts").into_response(),
        Err(e) => (flash.error(e.to_string()), Redirect::to("/hosts")).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ReleaseForm {
    hosts_ids: Vec<HostId>,
}

pub async fn release_hosts(
    State(service): State<HostsService>,
    Extension(user): Extension<User>,
    Form(data): Form<ReleaseForm>,
) -> impl IntoResponse {
    service
        .free(&user.id().into(), &data.hosts_ids)
        .await
        .unwrap();
    Redirect::to("/hosts")
}

pub async fn release_all(
    State(service): State<HostsService>,
    Extension(user): Extension<User>,
) -> impl IntoResponse {
    service.free_all(&user.id().into()).await.unwrap();
    Redirect::to("/hosts")
}

#[derive(Deserialize)]
pub struct GetHostsQuery {
    mail: String,
}

pub async fn get_hosts_json(
    State(hosts_service): State<HostsService>,
    State(user_service): State<UsersService>,
    OptionalQuery(query): OptionalQuery<GetHostsQuery>,
) -> impl IntoResponse {
    match query {
        Some(GetHostsQuery { mail }) => match user_service.get_user_by_mail(&mail).await.unwrap() {
            None => Json(vec![]),
            Some(user) => Json(
                hosts_service
                    .get_leased_hosts(&user.id)
                    .await
                    .unwrap()
                    .into_iter()
                    .map(HostInfo::from)
                    .collect::<Vec<_>>(),
            ),
        },
        None => {
            let users: HashMap<UserId, UserDb> = HashMap::from_iter(
                user_service
                    .get_all_users()
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|user| (user.id, user)),
            );

            Json(
                hosts_service
                    .get_all_hosts()
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|host| {
                        let user = host
                            .user_id
                            .and_then(|user_id| users.get(&user_id).cloned());

                        (host, user).into()
                    })
                    .collect::<Vec<HostInfo>>(),
            )
        }
    }
}
