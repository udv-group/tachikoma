use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use futures_concurrency::future::Race;
use ldap3::LdapConnAsync;
use secrecy::ExposeSecret;
use tachikoma::{
    bot::{build_tg_bot, from_config},
    configuration::get_config,
    db::{Registry, run_migrations},
    logic::{
        message_senders::TgMessages, notifications::Notifier, release::hosts_release_timer,
        users::UsersService,
    },
    set_env,
    web::Application,
};

use ldap3::drive;
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

    tracing::info!("Making unauthorized ldap connection");
    let (ldap_conn, ldap) =
        LdapConnAsync::with_settings(settings.ldap.clone().into(), &settings.ldap.url)
            .await
            .with_context(|| "Unable to get ldap connection")?;
    let ldap_conn_task = drive!(ldap_conn);

    tracing::info!("Making authorized ldap connection");
    let (authorized_ldap_conn, mut authorized_ldap) =
        LdapConnAsync::with_settings(settings.ldap.clone().into(), &settings.ldap.url)
            .await
            .with_context(|| "Unable to get authorized ldap connection")?;
    let authorized_ldap_conn_task = drive!(authorized_ldap_conn);
    authorized_ldap
        .simple_bind(&settings.ldap.login, settings.ldap.password.expose_secret())
        .await
        .with_context(|| "Unable to bind credentials")?
        .success()
        .with_context(|| "Unable to authorize")?;

    tracing::info!("Server starting up");
    let server = Application::build(
        &settings,
        registry.clone(),
        ldap,
        authorized_ldap,
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
            if let Err(err) = ldap_conn_task.await {
                tracing::error!("Ldap connection failed {err}");
            } else {
                tracing::info!("Ldap connection exited");
            }
        }),
        Box::pin(async move {
            if let Err(err) = authorized_ldap_conn_task.await {
                tracing::error!("Authorized ldap connection failed {err}");
            } else {
                tracing::info!("Authorized ldap connection exited");
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
