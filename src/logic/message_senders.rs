use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use teloxide::types::ChatId;
use teloxide::{Bot, prelude::Requester};

use super::notifications::{GetMessageSender, SendMessage};
use crate::db::models::User;

pub struct DisabledMessageSender;
#[async_trait]
impl SendMessage for DisabledMessageSender {
    async fn send_message(&self, _msg: String) -> Result<()> {
        unreachable!()
    }
}
impl GetMessageSender for DisabledMessageSender {
    fn get_message_sender(&self, _user: &User) -> Result<Box<dyn SendMessage>> {
        Err(anyhow::anyhow!("Message sending disabled"))
    }
}

pub struct TgMessages {
    bot: Bot,
}

impl TgMessages {
    pub fn new(bot: Bot) -> Self {
        Self { bot }
    }
}

impl GetMessageSender for TgMessages {
    fn get_message_sender(&self, user: &User) -> Result<Box<dyn SendMessage>> {
        let tg_handle = user
            .tg_handle
            .clone()
            .with_context(|| format!("User ({:?}) tg_handle is None", user.id))?;
        let chat_id = tg_handle
            .parse::<i64>()
            .with_context(|| format!("Failed parse chat_id from {tg_handle}"))?;

        Ok(Box::new(TgUser {
            bot: self.bot.clone(),
            chat_id,
        }))
    }
}

struct TgUser {
    bot: Bot,
    chat_id: i64,
}

#[async_trait]
impl SendMessage for TgUser {
    async fn send_message(&self, msg: String) -> Result<()> {
        self.bot.send_message(ChatId(self.chat_id), msg).await?;
        Ok(())
    }
}
