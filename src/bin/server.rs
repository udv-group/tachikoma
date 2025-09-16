use tachikoma::db::{Registry, run_migrations};
use tachikoma::logic::message_senders::DisabledMessageSender;
use tachikoma::logic::notifications::Notifier;
use tachikoma::{configuration::get_config, set_env, web::Application};
use tracing::info;

use ldap3::{LdapConnAsync, drive};
use secrecy::ExposeSecret;
use tachikoma::logic::release::hosts_release_timer;
use tachikoma::telemetry::init_tracing;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    set_env();
    init_tracing();
    let settings = get_config()?;

    run_migrations(&settings.database).await?;
    let registry = Registry::new(&settings.database).await?;
    let notifier = Notifier::new(registry.clone(), DisabledMessageSender {});

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
        "bot_username".into(),
    )
    .await?;

    tokio::select! {
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
    }
    Ok(())
}
