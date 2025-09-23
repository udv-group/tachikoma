pub mod handlers;

use crate::{
    bot::handlers::{handle_start_command, main_state_handler},
    logic::users::UsersService,
};

use std::error::Error;
use teloxide::{
    Bot,
    dispatching::{DefaultKey, Dispatcher, dialogue::InMemStorage},
    filter_command,
    macros::BotCommands,
    prelude::*,
};

#[derive(Clone, Default, Debug)]
pub enum BotState {
    #[default]
    Default,
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    #[command(description = "Start dialogue")]
    Start,
}

pub fn build_tg_bot(
    bot: Bot,
    users_service: UsersService,
) -> Dispatcher<Bot, Box<dyn Error + Send + Sync>, DefaultKey> {
    tracing::info!("Starting tachikama");

    let commands_handler = filter_command::<Command, _>()
        .branch(dptree::case![Command::Start].endpoint(handle_start_command));

    let messages_handler = Update::filter_message()
        .enter_dialogue::<Message, InMemStorage<BotState>, BotState>()
        .branch(commands_handler)
        .endpoint(main_state_handler);

    Dispatcher::builder(bot, dptree::entry().branch(messages_handler))
        .dependencies(dptree::deps![
            InMemStorage::<BotState>::new(),
            users_service
        ])
        .default_handler(|upd| async move {
            tracing::warn!("Unhandled update: {:?}", upd);
        })
        .error_handler(LoggingErrorHandler::with_custom_text(
            "An error has occurred in the dispatcher",
        ))
        .enable_ctrlc_handler()
        .build()
}
