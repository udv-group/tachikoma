use anyhow::{Context, Ok, Result};
use async_trait::async_trait;
use chrono::Utc;

use crate::db::{
    Registry,
    models::{HostId, User, UserId},
};

pub const EXTEND_CALLBACK_PREFIX: &str = "extend:";

#[derive(Debug, Clone)]
pub enum Notification {
    HostsReleased(Vec<HostId>),
    ExpirationSoon(Vec<HostId>),
}

#[async_trait]
pub trait SendMessage: Send + Sync {
    async fn send_message(&self, msg: String) -> Result<()>;
    async fn send_message_with_buttons(
        &self,
        msg: String,
        _buttons: Vec<Vec<(String, String)>>,
    ) -> Result<()> {
        self.send_message(msg).await
    }
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

        match notification {
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

                msg_sender.send_message(msg).await?;
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

                let buttons = vec![vec![
                    ("+1h".to_string(), format!("{EXTEND_CALLBACK_PREFIX}1")),
                    ("+8h".to_string(), format!("{EXTEND_CALLBACK_PREFIX}8")),
                    ("+1d".to_string(), format!("{EXTEND_CALLBACK_PREFIX}24")),
                ]];

                msg_sender.send_message_with_buttons(msg, buttons).await?;
            }
        };

        Ok(())
    }
}
