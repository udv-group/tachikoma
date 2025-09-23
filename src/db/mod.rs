pub mod models;

use std::ops::Deref;

use chrono::prelude::*;
use models::{AdGroupLeaseLimit, Group, GroupId};
use sqlx::{PgPool, Postgres, Transaction};

use crate::configuration::DatabaseSettings;
use crate::db::models::{Host, HostId, LeasedHost, User, UserId};

#[derive(Clone)]
pub struct Registry {
    pool: PgPool,
}

impl Registry {
    pub async fn new(settings: &DatabaseSettings) -> sqlx::Result<Self> {
        let pool = PgPool::connect_with(settings.with_db()).await?;
        Ok(Registry { pool })
    }
    pub async fn begin(&self) -> sqlx::Result<RegistryTx<'_>> {
        Ok(RegistryTx {
            tx: self.pool.begin().await?,
        })
    }
}
pub struct RegistryTx<'c> {
    tx: Transaction<'c, Postgres>,
}

impl RegistryTx<'_> {
    pub async fn commit(self) -> sqlx::Result<()> {
        self.tx.commit().await
    }

    pub async fn get_all_hosts(&mut self) -> sqlx::Result<Vec<Host>> {
        sqlx::query_as("SELECT * FROM hosts ORDER BY hosts.ip_address ASC")
            .fetch_all(&mut *self.tx)
            .await
    }

    pub async fn get_available_group_hosts(
        &mut self,
        group_id: &GroupId,
    ) -> sqlx::Result<Vec<Host>> {
        sqlx::query_as("SELECT * FROM hosts WHERE user_id is NULL AND group_id = $1 ORDER BY hosts.ip_address ASC")
            .bind(group_id)
            .fetch_all(&mut *self.tx)
            .await
    }

    pub async fn get_available_hosts(&mut self) -> sqlx::Result<Vec<Host>> {
        sqlx::query_as("SELECT * FROM hosts WHERE user_id is NULL ORDER BY hosts.ip_address ASC")
            .fetch_all(&mut *self.tx)
            .await
    }
    pub async fn get_first_available_group_host(
        &mut self,
        group_id: &GroupId,
    ) -> sqlx::Result<Option<Host>> {
        sqlx::query_as("SELECT * FROM hosts WHERE user_id is NULL AND group_id = $1 LIMIT 1")
            .bind(group_id)
            .fetch_optional(&mut *self.tx)
            .await
    }
    pub async fn get_host(&mut self, host_id: &HostId) -> sqlx::Result<Host> {
        sqlx::query_as("SELECT * FROM hosts WHERE id = $1 LIMIT 1")
            .bind(host_id.deref())
            .fetch_one(&mut *self.tx)
            .await
    }
    pub async fn get_hosts(&mut self, hosts_ids: &[HostId]) -> sqlx::Result<Vec<Host>> {
        sqlx::query_as("SELECT * FROM hosts WHERE id = any($1)")
            .bind(hosts_ids)
            .fetch_all(&mut *self.tx)
            .await
    }
    pub async fn get_leased_host(&mut self, host_id: &HostId) -> sqlx::Result<LeasedHost> {
        sqlx::query_as(
            r#"
            SELECT hosts.id as hid, hosts.hostname, hosts.ip_address, hosts.leased_until, hosts.group_id, users.id, users.dn, users.tg_handle, users.email, users.link 
            FROM hosts JOIN users on hosts.user_id = users.id 
            WHERE hosts.id = $1
            "#,
        ).bind(host_id.deref())
        .fetch_one(&mut *self.tx)
        .await
    }
    pub async fn get_leased_hosts(&mut self, user_id: &UserId) -> sqlx::Result<Vec<LeasedHost>> {
        sqlx::query_as(
            r#"
            SELECT hosts.id as hid, hosts.hostname, hosts.ip_address, hosts.leased_until, hosts.group_id, users.id, users.dn, users.tg_handle, users.email, users.link 
            FROM hosts JOIN users on hosts.user_id = users.id 
            WHERE hosts.user_id = $1 ORDER BY hosts.leased_until, hosts.ip_address ASC
            "#,
        ).bind(user_id.deref())
        .fetch_all(&mut *self.tx)
        .await
    }
    pub async fn get_leased_until_hosts(
        &mut self,
        until: DateTime<Utc>,
    ) -> sqlx::Result<Vec<LeasedHost>> {
        sqlx::query_as(
            r#"
            SELECT hosts.id as hid, hosts.hostname, hosts.ip_address, hosts.leased_until, hosts.group_id, users.id, users.dn, users.tg_handle, users.email, users.link  
            FROM hosts JOIN users on hosts.user_id = users.id
            WHERE hosts.leased_until < $1
            "#,
        ).bind(until).fetch_all(&mut *self.tx).await
    }
    pub async fn lease_hosts(
        &mut self,
        user_id: &UserId,
        hosts_ids: &[HostId],
        untill: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        let ids: Vec<_> = hosts_ids.iter().map(|h| h.0).collect();

        sqlx::query!(
            "UPDATE hosts SET user_id = $1, leased_until = $2 WHERE id = any($3)",
            user_id.deref(),
            untill,
            ids.as_slice(),
        )
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }
    pub async fn free_hosts_for_user(
        &mut self,
        hosts_ids: &[HostId],
        user_id: &UserId,
    ) -> sqlx::Result<()> {
        let ids: Vec<_> = hosts_ids.iter().map(|h| h.0).collect();
        sqlx::query!(
            "UPDATE hosts SET user_id = NULL, leased_until = NULL WHERE id = any($1) AND user_id = $2",
            ids.as_slice(),
            user_id.deref(),
        )
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }
    pub async fn free_hosts(&mut self, hosts_ids: &[HostId]) -> sqlx::Result<()> {
        let ids: Vec<_> = hosts_ids.iter().map(|h| h.0).collect();
        sqlx::query!(
            "UPDATE hosts SET user_id = NULL, leased_until = NULL WHERE id = any($1)",
            ids.as_slice(),
        )
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }
    pub async fn free_all(&mut self, user_id: &UserId) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE hosts SET user_id = NULL, leased_until = NULL WHERE user_id = $1",
            user_id.deref()
        )
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }
    pub async fn get_user_by_id(&mut self, user_id: &UserId) -> sqlx::Result<Option<User>> {
        sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", user_id.deref())
            .fetch_optional(&mut *self.tx)
            .await
    }
    pub async fn get_user_by_mail(&mut self, mail: &str) -> sqlx::Result<Option<User>> {
        sqlx::query_as!(User, "SELECT * FROM users WHERE email = $1", mail)
            .fetch_optional(&mut *self.tx)
            .await
    }
    pub async fn get_user_by_dn(&mut self, dn: &str) -> sqlx::Result<Option<User>> {
        sqlx::query_as!(User, "SELECT * FROM users WHERE dn = $1", dn)
            .fetch_optional(&mut *self.tx)
            .await
    }
    pub async fn get_user_by_link(&mut self, link: &str) -> sqlx::Result<Option<User>> {
        sqlx::query_as!(User, "SELECT * FROM users WHERE link = $1", link)
            .fetch_optional(&mut *self.tx)
            .await
    }
    pub async fn set_user_tg_handle(
        &mut self,
        user_id: &UserId,
        tg_handle: &str,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE users SET tg_handle = $1 WHERE id = $2",
            tg_handle,
            user_id.deref(),
        )
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }
    pub async fn add_user(
        &mut self,
        dn: &str,
        tg_handle: Option<&str>,
        email: &str,
    ) -> sqlx::Result<UserId> {
        let rec = sqlx::query!(
            "INSERT INTO users (dn, tg_handle, email) VALUES ($1, $2, $3) RETURNING id",
            dn,
            tg_handle,
            email
        )
        .fetch_one(&mut *self.tx)
        .await?;
        Ok(rec.id.into())
    }

    pub async fn get_groups(&mut self) -> sqlx::Result<Vec<Group>> {
        sqlx::query_as("SELECT * FROM groups ORDER BY name ASC")
            .fetch_all(&mut *self.tx)
            .await
    }
    pub async fn get_all_users(&mut self) -> sqlx::Result<Vec<User>> {
        sqlx::query_as("SELECT * from users")
            .fetch_all(&mut *self.tx)
            .await
    }
    pub async fn get_ad_groups_lease_limits(
        &mut self,
        groups: &Vec<String>,
    ) -> sqlx::Result<Vec<AdGroupLeaseLimit>> {
        sqlx::query_as!(
            AdGroupLeaseLimit,
            "SELECT * FROM lease_limits_by_ad_group lg WHERE lg.group = ANY($1)",
            groups.as_slice()
        )
        .fetch_all(&mut *self.tx)
        .await
    }
}

pub async fn run_migrations(settings: &DatabaseSettings) -> anyhow::Result<()> {
    let pool = PgPool::connect_with(settings.with_db()).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(())
}
