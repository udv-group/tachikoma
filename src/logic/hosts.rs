use std::collections::HashSet;

use chrono::Utc;
use thiserror::Error;

use crate::db::RegistryTx;
use crate::db::{
    Registry,
    models::{GroupId, Host, HostId, HostListing, LeasedHost, UserId},
};

#[derive(Error, Debug)]
pub enum HostError {
    #[error("Database error")]
    DatabaseError(#[from] sqlx::Error),

    #[error("There is no free hosts")]
    ThereIsNoFreeHosts,

    #[error("Host is already leased")]
    AlreadyLeased(Vec<HostId>),

    #[error("Hosts lease limit is reached")]
    LeaseLimit,

    #[error("Unexpected error")]
    UnexpectedError(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct HostsService {
    registry: Registry,
    lease_limit: usize,
}

impl HostsService {
    pub fn new(registry: Registry, lease_limit: usize) -> Self {
        HostsService {
            registry,
            lease_limit,
        }
    }

    pub async fn get_all_hosts(&self) -> Result<Vec<Host>, HostError> {
        let mut tx = self.registry.begin().await?;
        let hosts = tx.get_all_hosts().await?;
        tx.commit().await?;
        Ok(hosts)
    }

    pub async fn get_available_group_hosts(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<Host>, HostError> {
        let mut tx = self.registry.begin().await?;
        let hosts = tx.get_available_group_hosts(group_id).await?;

        tx.commit().await?;
        Ok(hosts)
    }

    pub async fn get_available_hosts(&self) -> Result<Vec<Host>, HostError> {
        let mut tx = self.registry.begin().await?;
        let hosts = tx.get_available_hosts().await?;
        tx.commit().await?;
        Ok(hosts)
    }

    pub async fn get_leased_hosts(&self, user_id: &UserId) -> Result<Vec<LeasedHost>, HostError> {
        let mut tx = self.registry.begin().await?;
        let hosts = tx.get_leased_hosts(user_id).await?;
        tx.commit().await?;
        Ok(hosts)
    }

    pub async fn get_hosts_filtered(
        &self,
        email: Option<&str>,
        group_id: Option<GroupId>,
    ) -> Result<Vec<HostListing>, HostError> {
        let mut tx = self.registry.begin().await?;
        let hosts = tx.get_hosts_filtered(email, group_id).await?;
        tx.commit().await?;
        Ok(hosts)
    }

    pub async fn lease(
        &self,
        user_id: &UserId,
        user_groups: &Vec<String>,
        hosts_ids: &[HostId],
        lease_for: chrono::TimeDelta,
    ) -> Result<Vec<LeasedHost>, HostError> {
        let mut tx = self.registry.begin().await?;
        let leased: HashSet<_> = tx
            .get_leased_hosts(user_id)
            .await?
            .into_iter()
            .map(|h| h.id)
            .collect();

        let mut hosts_ids_set: HashSet<_> = HashSet::new();
        hosts_ids_set.extend(hosts_ids.to_vec());

        let intersection: HashSet<_> = leased.intersection(&hosts_ids_set).collect();
        if !intersection.is_empty() {
            return Err(HostError::AlreadyLeased(
                intersection.into_iter().cloned().collect(),
            ));
        }

        let lease_limit = self.get_lease_limit(&mut tx, user_groups).await?;
        if leased.len() + hosts_ids_set.len() > lease_limit {
            return Err(HostError::LeaseLimit);
        };

        tx.lease_hosts(user_id, hosts_ids, Utc::now() + lease_for)
            .await?;

        let leased = tx.get_leased_hosts(user_id).await?;
        tx.commit().await?;
        Ok(leased)
    }

    pub async fn free(&self, user_id: &UserId, hosts_ids: &[HostId]) -> Result<(), HostError> {
        let mut tx = self.registry.begin().await?;
        tx.free_hosts_for_user(hosts_ids.as_ref(), user_id).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn free_all(&self, user_id: &UserId) -> Result<(), HostError> {
        let mut tx = self.registry.begin().await?;

        tx.free_all(user_id).await?;
        tx.commit().await?;

        Ok(())
    }

    pub async fn lease_random(
        &self,
        user_id: &UserId,
        user_groups: &Vec<String>,
        lease_for: chrono::TimeDelta,
        group_id: &GroupId,
    ) -> Result<LeasedHost, HostError> {
        let mut tx = self.registry.begin().await?;
        let lease_limit = self.get_lease_limit(&mut tx, user_groups).await?;

        if tx.get_leased_hosts(user_id).await?.len() >= lease_limit {
            return Err(HostError::LeaseLimit);
        };
        if let Some(host) = tx.get_first_available_group_host(group_id).await? {
            tx.lease_hosts(user_id, &[host.id], Utc::now() + lease_for)
                .await?;
            let leased = tx.get_leased_host(&host.id).await?;
            tx.commit().await?;

            return Ok(leased);
        }
        Err(HostError::ThereIsNoFreeHosts)
    }

    pub async fn get_lease_limit(
        &self,
        tx: &mut RegistryTx<'_>,
        groups: &Vec<String>,
    ) -> anyhow::Result<usize> {
        let groups_limits = tx.get_ad_groups_lease_limits(groups).await?;
        let limit = groups_limits
            .into_iter()
            .map(|gl| gl.limit.try_into().unwrap())
            .max()
            .unwrap_or(self.lease_limit);

        Ok(limit)
    }
}
