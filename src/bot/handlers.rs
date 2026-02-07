use anyhow::Context;
use chrono::Utc;
use teloxide::payloads::EditMessageTextSetters;
use teloxide::types::{CallbackQuery, InlineKeyboardMarkup, MaybeInaccessibleMessage};
use teloxide::{Bot, requests::Requester, types::Message};

use crate::db::Registry;
use crate::logic::notifications::EXTEND_CALLBACK_PREFIX;
use crate::logic::release::EXPIRATION_NOTIFY_WINDOW;
use crate::logic::users::UsersService;

pub type AnyResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub type HandlerResult = AnyResult<()>;

pub async fn main_state_handler(
    bot: Bot,
    msg: Message,
    users_service: UsersService,
) -> HandlerResult {
    tracing::debug!(
        "Handling message. chat_id={} from={:?}",
        msg.chat.id,
        msg.from.as_ref().map(|f| f.id)
    );

    let user_id = msg
        .from
        .as_ref()
        .map(|m_from| m_from.id)
        .with_context(|| "Message without user_id")?;
    let link = msg
        .text()
        .unwrap_or("")
        .trim_matches(|c| c == ' ' || c == '"');

    match users_service
        .link_user(link, user_id.0.to_string().as_ref())
        .await
        .with_context(|| "Failed user link")?
    {
        Some(user) => {
            bot.send_message(
                msg.chat.id,
                format!("Telegram successfully linked for user '{}'", user.email),
            )
            .await?;
        }
        None => {
            bot.send_message(msg.chat.id, "User for this link code not found")
                .await?;
        }
    };
    bot.delete_message(msg.chat.id, msg.id).await?;

    Ok(())
}

pub async fn handle_start_command(bot: Bot, msg: Message) -> HandlerResult {
    tracing::debug!(
        "Handling {:?} command. chat_id={} from={:?}",
        msg.text(),
        msg.chat.id,
        msg.from.as_ref().map(|f| f.id)
    );
    bot.send_message(msg.chat.id, "Hello, send your link code")
        .await?;

    Ok(())
}

pub async fn handle_extend_callback(
    bot: Bot,
    q: CallbackQuery,
    registry: Registry,
) -> HandlerResult {
    let callback_data = q.data.unwrap_or_default();
    let callback_id = q.id;
    let tg_user_id = q.from.id.0.to_string();
    let callback_message = q.message;

    let hours: i32 = match callback_data.strip_prefix(EXTEND_CALLBACK_PREFIX) {
        Some(h) => h.parse().unwrap_or(0),
        None => return Ok(()),
    };

    if hours <= 0 {
        bot.answer_callback_query(callback_id).await?;
        return Ok(());
    }

    let response = match extend_user_hosts(&registry, &tg_user_id, hours).await {
        Ok(msg) => msg,
        Err(err) => {
            tracing::error!("Failed to extend hosts for tg_user {tg_user_id}: {err}");
            format!("Failed to extend lease: {err}")
        }
    };

    bot.answer_callback_query(callback_id).await?;

    if let Some(MaybeInaccessibleMessage::Regular(msg)) = callback_message
        && let Err(err) = bot
            .edit_message_text(msg.chat.id, msg.id, &response)
            .reply_markup(InlineKeyboardMarkup::default())
            .await
    {
        tracing::error!("Failed to edit message: {err}");
    }

    Ok(())
}

async fn extend_user_hosts(
    registry: &Registry,
    tg_handle: &str,
    hours: i32,
) -> anyhow::Result<String> {
    let mut tx = registry
        .begin()
        .await
        .context("Failed to begin transaction")?;

    let user = tx
        .get_user_by_tg_handle(tg_handle)
        .await
        .context("Failed to query user")?
        .context("User not found. Please link your Telegram account first.")?;

    let expiring_before = Utc::now() + EXPIRATION_NOTIFY_WINDOW;
    let expiring_hosts = tx
        .get_expiring_hosts_for_user(&user.id, expiring_before)
        .await
        .context("Failed to query expiring hosts")?;

    if expiring_hosts.is_empty() {
        return Ok("No expiring hosts to extend.".to_string());
    }

    let host_ids: Vec<_> = expiring_hosts.iter().map(|h| h.id).collect();
    tx.extend_hosts_lease(&host_ids, hours)
        .await
        .context("Failed to extend lease")?;

    // Re-query to get updated leased_until values
    let updated_hosts = tx
        .get_hosts(&host_ids)
        .await
        .context("Failed to query updated hosts")?;

    tx.commit().await.context("Failed to commit transaction")?;

    let mut msg = format!("Lease extended by {hours} hour(s).\n\nUpdated hosts:");
    for (idx, host) in updated_hosts.iter().enumerate() {
        let minutes_left = host
            .leased_until
            .map(|until| (until - Utc::now()).num_minutes())
            .unwrap_or(0);
        msg.push_str(&format!(
            "\n{}. {} ({}) - {} minutes left",
            idx + 1,
            host.hostname,
            host.ip_address.ip(),
            minutes_left
        ));
    }

    Ok(msg)
}
