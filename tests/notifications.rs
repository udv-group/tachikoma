pub mod support;

use std::sync::Arc;

use crate::support::registry::create_registry;
use async_cell::sync::AsyncCell;
use async_trait::async_trait;
use tachikoma::{
    db::models::User,
    logic::notifications::{GetMessageSender, Notification, Notifier, SendMessage},
};

use anyhow::{Context, Result};

struct TestAdapter {
    pub sent: Arc<AsyncCell<Vec<String>>>,
    pub user: Option<User>,
}

impl TestAdapter {
    pub fn new(sent: Arc<AsyncCell<Vec<String>>>) -> Self {
        Self { sent, user: None }
    }
}

impl GetMessageSender for TestAdapter {
    fn get_message_sender(&self, user: &User) -> Result<Box<dyn SendMessage>> {
        Ok(Box::new(TestAdapter {
            sent: self.sent.clone(),
            user: Some(user.clone()),
        }))
    }
}

#[async_trait]
impl SendMessage for TestAdapter {
    async fn send_message(&self, _msg: String) -> Result<()> {
        let mut sent = self.sent.take().await;
        self.user
            .clone()
            .map(|u| u.tg_handle.map(|tg| sent.push(tg)));

        self.sent.set(sent);
        Ok(())
    }
}

#[tokio::test]
async fn release_notification_send() -> Result<()> {
    let (mut test_registry, registry) = create_registry().await;

    let sent = AsyncCell::<Vec<String>>::new().into_shared();
    sent.set(Vec::new());

    let test_adapter = TestAdapter::new(sent.clone());

    let host = test_registry.generate_host().await;
    let user = test_registry.generate_user().await;

    let notifier = Notifier::new(registry, test_adapter);

    notifier
        .notify(user.id, &Notification::HostsReleased(vec![host.id]))
        .await?;
    let user_id = sent
        .try_take()
        .unwrap()
        .pop()
        .with_context(|| "Failed receive expected message")?;
    assert_eq!(user_id, user.tg_handle);

    Ok(())
}
