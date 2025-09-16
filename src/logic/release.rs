use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use chrono::Utc;

use tokio::time::sleep;

use crate::db::{
    Registry,
    models::{HostId, LeasedHost, UserId},
};
use anyhow::Result;

use super::notifications::{GetMessageSender, Notification, Notifier};
use itertools::Itertools;
use tracing::{debug, error};

pub async fn hosts_release_timer<T: GetMessageSender>(registry: Registry, notifier: &Notifier<T>) {
    let mut release_timer = ReleaseTimer {
        registry,
        sent_hosts_notification: HashMap::new(),
        expiration_notify_delay_time: Duration::from_secs(30 * 60),
    };
    loop {
        match release_timer.release().await {
            Ok(released_hosts) if !released_hosts.is_empty() => {
                debug!(
                    "Released hosts: {:?}",
                    released_hosts
                        .iter()
                        .map(|h| h.id.clone().to_string())
                        .join(", ")
                );
                if let Err(err) = release_timer
                    .notify_released_hosts(notifier, &released_hosts)
                    .await
                {
                    error!("Failed released hosts notification: {}", err);
                };
            }
            Ok(_) => {}
            Err(err) => error!("Release fail: {err}"),
        };

        if let Err(err) = release_timer.notify_soon_release(notifier).await {
            error!("Notify soon release fail: {err}");
        }
        sleep(Duration::from_secs(10)).await;
    }
}

struct ReleaseTimer {
    registry: Registry,
    sent_hosts_notification: HashMap<UserId, HashSet<HostId>>,
    expiration_notify_delay_time: Duration,
}

impl ReleaseTimer {
    async fn release(&mut self) -> Result<Vec<LeasedHost>> {
        let mut tx = self.registry.begin().await?;

        let expired_hosts = tx.get_leased_until_hosts(Utc::now()).await?;
        let mut users_hosts: HashMap<UserId, HashSet<HostId>> = HashMap::new();
        expired_hosts.iter().for_each(|host| {
            users_hosts.entry(host.user.id).or_default().insert(host.id);
        });

        if !expired_hosts.is_empty() {
            tx.free_hosts(
                expired_hosts
                    .iter()
                    .map(|h| h.id)
                    .collect::<Vec<HostId>>()
                    .as_ref(),
            )
            .await?;
            tx.commit().await?;
        }

        users_hosts
            .drain()
            .for_each(|(user_id, expired_hosts_ids)| {
                self.sent_hosts_notification
                    .entry(user_id)
                    .and_modify(|hosts_ids| {
                        hosts_ids.retain(|host_id| !expired_hosts_ids.contains(host_id));
                    });
            });
        Ok(expired_hosts)
    }

    async fn notify_released_hosts<T: GetMessageSender>(
        &mut self,
        notifier: &Notifier<T>,
        released_hosts: &[LeasedHost],
    ) -> Result<()> {
        let mut expired_notifications: HashMap<UserId, Vec<HostId>> = HashMap::new();
        released_hosts.iter().for_each(|host| {
            expired_notifications
                .entry(host.user.id)
                .and_modify(|v| v.push(host.id))
                .or_insert(vec![host.id]);
        });

        for (user_id, hosts_ids) in expired_notifications.into_iter() {
            let notification = Notification::HostsReleased(hosts_ids);

            if let Err(err) = notifier.notify(user_id, &notification).await {
                error!(
                    "Notification sending error {:?} {:?}: {err}",
                    user_id, notification
                );
            }
        }

        Ok(())
    }

    async fn notify_soon_release<T: GetMessageSender>(
        &mut self,
        notifier: &Notifier<T>,
    ) -> Result<()> {
        let mut tx = self.registry.begin().await?;

        let next_expiration_date = Utc::now() + self.expiration_notify_delay_time;
        let expire_soon_hosts = tx.get_leased_until_hosts(next_expiration_date).await?;

        let mut expire_soon_notifications: HashMap<UserId, HashSet<HostId>> = HashMap::new();

        expire_soon_hosts.into_iter().for_each(|host| {
            expire_soon_notifications
                .entry(host.user.id)
                .and_modify(|v| {
                    v.insert(host.id);
                })
                .or_insert(HashSet::from_iter(vec![host.id]));
        });

        for (user_id, hosts_ids) in expire_soon_notifications.into_iter() {
            let sent_host_ids = self.sent_hosts_notification.entry(user_id).or_default();
            let hosts_ids: Vec<HostId> = hosts_ids.difference(sent_host_ids).copied().collect();

            if hosts_ids.is_empty() {
                continue;
            }

            let notification = Notification::ExpirationSoon(hosts_ids.clone());
            if let Err(err) = notifier.notify(user_id, &notification).await {
                error!(
                    "Notification sending error {:?} {:?}: {err}",
                    user_id, notification
                );
            } else {
                sent_host_ids.extend(hosts_ids.iter());
            }
        }

        Ok(())
    }
}
