use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use futures_concurrency::future::Race;
use tachikoma::{
    bot::{build_tg_bot, from_config},
    configuration::get_config,
    db::{Registry, run_migrations},
    ldap::UsersInfo,
    logic::{
        message_senders::TgMessages, notifications::Notifier, release::hosts_release_timer,
        users::UsersService,
    },
    set_env,
    web::Application,
};

use teloxide::{Bot, requests::Requester};
use tracing::info;

use tachikoma::telemetry::init_tracing;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    set_env();
    init_tracing();
    tracing::info!("Starting tachikoma");

    let settings = get_config().expect("Unable to get configuration");

    tracing::info!("Running migrations");
    run_migrations(&settings.database)
        .await
        .with_context(|| "Migration failed")?;
    let registry = Registry::new(&settings.database).await?;

    let (tg_bot, tg_bot_username, tg_notifier): (
        Option<Bot>,
        Option<String>,
        Option<Notifier<TgMessages>>,
    ) = if let Some(telegram_conf) = settings.telegram.clone() {
        info!("Found telegram bot configuration");
        let tg_bot = from_config(telegram_conf);
        let tg_bot_username = tg_bot
            .get_me()
            .await
            .with_context(|| "Unable to get bot username")?
            .user
            .username
            .with_context(|| "Bot hasn't username?!")?;
        let notifier = Notifier::new(registry.clone(), TgMessages::new(tg_bot.clone()));
        (Some(tg_bot), Some(tg_bot_username), Some(notifier))
    } else {
        (None, None, None)
    };

    let users_info = UsersInfo::new(settings.ldap.clone()).await?;

    tracing::info!("Server starting up");
    let server = Application::build(
        &settings,
        registry.clone(),
        users_info.clone(),
        tg_bot_username.map(|username| format!("https://t.me/{username}")),
    )
    .await?;

    let mut tasks: Vec<Pin<Box<dyn Future<Output = ()>>>> = vec![
        Box::pin(async move {
            if let Err(err) = server.serve_forever().await {
                tracing::error!("Server failed {err}");
            } else {
                tracing::info!("Server exited");
            }
        }),
        Box::pin(async move {
            if let Err(err) = users_info.work().await {
                tracing::error!("users_info task failed {err}");
            } else {
                tracing::info!("users_info task finished");
            }
        }),
    ];

    let timer_registry = registry.clone();
    tasks.push(Box::pin(async move {
        hosts_release_timer(timer_registry, &tg_notifier).await;
        tracing::info!("Hosts release timer exited");
    }));

    if let Some(tg_bot) = tg_bot {
        let bot_registry = registry.clone();
        let mut dispatcher = build_tg_bot(
            tg_bot,
            UsersService::new(bot_registry.clone()),
            bot_registry,
        );

        tasks.push(Box::pin(async move {
            dispatcher.dispatch().await;
            info!("Bot exited");
        }));
    }

    tasks.race().await;
    info!("tachikoma shut down");
    Ok(())
}
