use anyhow::Context;
use ldap3::LdapConnAsync;
use secrecy::ExposeSecret;
use tachikoma::{
    bot::build_tg_bot,
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
use tokio::select;
use tracing::info;

use tachikoma::telemetry::init_tracing;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    set_env();
    init_tracing();
    let settings = get_config()?;

    tracing::info!("Starting tachikama");
    run_migrations(&settings.database).await?;

    let bot = Bot::from_env();
    let bot_username = bot
        .get_me()
        .await?
        .user
        .username
        .with_context(|| "Bot hasn't username?!")?;

    let registry = Registry::new(&settings.database).await?;
    let notifier = Notifier::new(registry.clone(), TgMessages::new(bot.clone()));

    let (ldap_conn, ldap) =
        LdapConnAsync::with_settings(settings.ldap.clone().into(), &settings.ldap.url).await?;
    let ldap_conn_task = drive!(ldap_conn);
    let (authorized_ldap_conn, mut authorized_ldap) =
        LdapConnAsync::with_settings(settings.ldap.clone().into(), &settings.ldap.url).await?;
    let authorized_ldap_conn_task = drive!(authorized_ldap_conn);
    authorized_ldap
        .simple_bind(&settings.ldap.login, settings.ldap.password.expose_secret())
        .await?
        .success()?;

    let server = Application::build(
        &settings,
        registry.clone(),
        ldap,
        authorized_ldap,
        format!("https://t.me/{bot_username}"),
    )
    .await?;

    let mut dispatcher = build_tg_bot(bot, UsersService::new(registry.clone()));

    select! {
        _ = server.serve_forever() => {
            info!("Server exited")
        }
        _ = ldap_conn_task => {
            info!("Ldap connection exited")
        }
        _ = authorized_ldap_conn_task => {
            info!("Authorized ldap connection exited")
        }
        _ = hosts_release_timer(registry, &notifier) => {
            info!("Hosts release timer exited")
        }
        _ = dispatcher.dispatch() => {
            info!("Bot exited")
        }
    };
    info!("tachikama shut down");
    Ok(())
}
