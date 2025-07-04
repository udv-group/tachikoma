use anyhow::{Context, Ok, Result};
use async_trait::async_trait;
use chrono::Utc;

use crate::db::{
    Registry,
    models::{HostId, User, UserId},
};

#[derive(Debug, Clone)]
pub enum Notification {
    HostsReleased(Vec<HostId>),
    ExpirationSoon(Vec<HostId>),
}

#[async_trait]
pub trait SendMessage {
    async fn send_message(&self, msg: String) -> Result<()>;
}

pub trait GetMessageSender {
    fn get_message_sender(&self, user: &User) -> Result<Box<dyn SendMessage>>;
}

pub struct Notifier<T> {
    registry: Registry,
    msg_sender: T,
}

impl<T> Notifier<T>
where
    T: GetMessageSender,
{
    pub fn new(registry: Registry, msg_sender: T) -> Self {
        Self {
            registry,
            msg_sender,
        }
    }

    pub async fn notify(&self, user_id: UserId, notification: &Notification) -> Result<()> {
        let mut tx = self
            .registry
            .begin()
            .await
            .with_context(|| "Failed to begin transaction")?;

        let user = tx
            .get_user_by_id(&user_id)
            .await
            .with_context(|| format!("Failed to read user {user_id:?}"))?
            .with_context(|| format!("User ({user_id:?}) doesn't exist"))?;

        let msg_sender = self.msg_sender.get_message_sender(&user)?;

        let msg = match notification {
            Notification::HostsReleased(hosts_ids) => {
                if hosts_ids.is_empty() {
                    return Ok(());
                }

                let hosts = tx.get_hosts(hosts_ids).await?;

                let mut msg = String::from("Hosts released:");
                msg.extend(hosts.into_iter().enumerate().map(|(idx, host)| {
                    format!(
                        "\n{}. {} ({})",
                        idx + 1,
                        host.hostname,
                        host.ip_address.ip()
                    )
                }));

                msg
            }
            Notification::ExpirationSoon(hosts_ids) => {
                if hosts_ids.is_empty() {
                    return Ok(());
                }

                let hosts = tx.get_hosts(hosts_ids).await?;
                let mut msg = String::from("Hosts expiring soon:");
                msg.extend(hosts.into_iter().enumerate().map(|(idx, host)| {
                    format!(
                        "\n{}. {} ({}) - {} minutes left",
                        idx + 1,
                        host.hostname,
                        host.ip_address.ip(),
                        (host.leased_until.unwrap() - Utc::now()).num_minutes()
                    )
                }));

                msg
            }
        };

        msg_sender.send_message(msg).await?;

        Ok(())
    }
}
