use askama::Template;
use chrono::{DateTime, TimeDelta, Utc};

use serde::{Deserialize, Serialize};

use crate::{
    AppInfo,
    db::models::{Group, GroupId, Host, HostId, LeasedHost, User as UserDb},
};

use super::auth::middleware::User;

#[derive(Template, Debug)]
#[template(path = "hosts_page_base.html", escape = "none")]
pub struct HostsPage<T: Template> {
    pub page: T,
    pub user: UserInfo,
    pub auth_link: Option<String>,
    pub app_info: AppInfo,
}

#[derive(Template, Debug)]
#[template(path = "hosts_lease.html", escape = "none")]
pub struct HostsLeasePage {
    pub groups: Vec<GroupInfo>,
    pub selected_group: GroupInfo,
    pub hosts: Vec<HostInfo>,
    pub leased: Vec<HostInfo>,
    pub error: Option<String>,
}

#[derive(Template, Debug)]
#[template(path = "all_hosts.html", escape = "none")]
pub struct AllHostsPage {
    pub hosts: Vec<HostInfo>,
}

#[derive(Deserialize, Debug)]
pub struct UserInfo {
    pub login: String,
    pub groups: Vec<String>,
    pub tg_linked: bool,
    pub link: String,
}
impl From<User> for UserInfo {
    fn from(value: User) -> Self {
        Self {
            login: value.username,
            groups: value.groups,
            tg_linked: value.tg_handle.is_some(),
            link: value.link,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct GroupInfo {
    pub id: GroupId,
    pub name: String,
}
impl From<Group> for GroupInfo {
    fn from(value: Group) -> Self {
        Self {
            id: value.id,
            name: value.name,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LeaseInfo {
    pub leased_until: DateTime<Utc>,
    pub valid_for: String,
    pub leased_by: String,
}
impl From<(UserDb, DateTime<Utc>)> for LeaseInfo {
    fn from(value: (UserDb, DateTime<Utc>)) -> Self {
        let (user, leased_until) = value;
        LeaseInfo {
            leased_by: user.email,
            leased_until,
            valid_for: format_duration(leased_until - Utc::now()),
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct HostInfo {
    pub id: HostId,
    pub hostname: String,
    pub ip_address: String,
    pub lease_info: Option<LeaseInfo>,
}
impl From<(Host, Option<UserDb>)> for HostInfo {
    fn from(value: (Host, Option<UserDb>)) -> Self {
        let (host, user) = value;

        Self {
            id: host.id,
            hostname: host.hostname,
            ip_address: host.ip_address.ip().to_string(),
            lease_info: match (user, host.leased_until) {
                (Some(user), Some(leased_until)) => Some((user, leased_until).into()),
                _ => None,
            },
        }
    }
}
impl From<Host> for HostInfo {
    fn from(value: Host) -> Self {
        (value, None).into()
    }
}
impl From<LeasedHost> for HostInfo {
    fn from(value: LeasedHost) -> Self {
        Self {
            id: value.id,
            hostname: value.hostname,
            ip_address: value.ip_address.ip().to_string(),
            lease_info: Some((value.user, value.leased_until).into()),
        }
    }
}

pub fn format_duration(duration: TimeDelta) -> String {
    let days = duration.num_days();
    let hours = (duration - TimeDelta::days(days)).num_hours();
    let minutes = (duration - TimeDelta::days(days) - TimeDelta::hours(hours)).num_minutes();
    format!("{days} days, {hours} hours, {minutes} minutes")
}
