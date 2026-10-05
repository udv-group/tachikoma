use std::{fmt::Display, ops::Deref, str::FromStr};

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::types::ipnetwork::IpNetwork;

#[derive(sqlx::Type, Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[sqlx(transparent)]
pub struct UserId(pub i32);

impl Deref for UserId {
    type Target = i32;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<i32> for UserId {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

#[derive(sqlx::Type, Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(try_from = "String")]
#[sqlx(transparent)]
pub struct HostId(pub i32);
impl Display for HostId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for HostId {
    type Target = i32;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl TryFrom<String> for HostId {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.parse::<i32>() {
            Ok(v) => Ok(Self(v)),
            Err(_) => Err(format!("Wrong value {value}, can not parse to as i32")),
        }
    }
}

impl From<i32> for HostId {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

#[derive(sqlx::Type, Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[sqlx(transparent)]
pub struct GroupId(pub i32);
impl Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl FromStr for GroupId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<i32>() {
            Ok(v) => Ok(Self(v)),
            Err(_) => Err(format!("Wrong value {s}, can not parse to as i32")),
        }
    }
}

impl Deref for GroupId {
    type Target = i32;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<i32> for GroupId {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, FromRow)]
pub struct LeasedHost {
    #[sqlx(rename = "hid")]
    pub id: HostId,
    pub hostname: String,
    pub ip_address: IpNetwork,
    pub leased_until: DateTime<Utc>,
    #[sqlx(flatten)]
    pub user: User,
}

#[derive(Serialize, Deserialize, Clone, Debug, FromRow)]
pub struct Host {
    pub id: HostId,
    pub hostname: String,
    pub ip_address: IpNetwork,
    pub leased_until: Option<DateTime<Utc>>,
    pub user_id: Option<UserId>,
    pub group_id: GroupId,
}

#[derive(Serialize, Deserialize, Clone, Debug, FromRow)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, FromRow)]
pub struct User {
    pub id: UserId,
    pub dn: String,
    pub tg_handle: Option<String>,
    pub email: String,
    pub link: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, FromRow)]
pub struct AdGroupLeaseLimit {
    pub group: String,
    pub limit: i32,
}

#[derive(sqlx::Type, Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[sqlx(transparent)]
#[serde(transparent)]
pub struct ApiTokenId(pub i32);

impl Deref for ApiTokenId {
    type Target = i32;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<i32> for ApiTokenId {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl Display for ApiTokenId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ApiTokenId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<i32>() {
            Ok(v) => Ok(Self(v)),
            Err(_) => Err(format!("Wrong value {s}, can not parse as i32")),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, FromRow)]
pub struct ApiToken {
    pub id: ApiTokenId,
    pub user_id: UserId,
    pub name: Option<String>,
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
}
