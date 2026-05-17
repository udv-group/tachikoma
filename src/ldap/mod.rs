use std::sync::Arc;
use std::time::Duration;

use ldap3::{Ldap, LdapConnAsync};
use ldap3::{ResultEntry, SearchEntry};
use secrecy::{ExposeSecret, SecretString};

use anyhow::{Context, Result, anyhow};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio::{select, spawn};

use crate::configuration::LdapSettings;

struct UsersInfoState {
    ldap: Option<Ldap>,
    authorized_ldap: Option<Ldap>,
}

#[derive(Clone)]
pub struct UsersInfo {
    ldap_settings: LdapSettings,
    state: Arc<Mutex<UsersInfoState>>,
}

#[derive(Debug)]
pub struct AdUserInfo {
    pub dn: String,
    pub email: String,
    pub groups: Vec<String>,
}

impl TryFrom<SearchEntry> for AdUserInfo {
    type Error = anyhow::Error;

    fn try_from(value: SearchEntry) -> std::result::Result<Self, Self::Error> {
        let mail = value
            .attrs
            .get("mail")
            .map(|mails| mails.first().cloned())
            .unwrap_or(None);

        let groups = value.attrs.get("memberOf").map(|groups| {
            groups
                .iter()
                .filter_map(|group| {
                    group.split(",").find_map(|item| {
                        item.starts_with("CN=")
                            .then(|| item.split_once("=").unwrap().1.to_string())
                    })
                })
                .collect()
        });

        if let (Some(mail), Some(groups)) = (mail, groups) {
            return Ok(AdUserInfo {
                email: mail,
                dn: value.dn,
                groups,
            });
        };

        Err(anyhow!("Unable to parse SearchEntry"))
    }
}

impl UsersInfo {
    pub async fn new(ldap_settings: LdapSettings) -> Result<Self> {
        Ok(Self {
            ldap_settings,
            state: Arc::new(Mutex::new(UsersInfoState {
                authorized_ldap: None,
                ldap: None,
            })),
        })
    }

    pub async fn work(self) -> Result<()> {
        select! {
            res = keep_authorized_ldap_conn(self.clone()) => {
                if let Err(err) = res {
                    tracing::error!("Failed keep_authorized_ldap_conn task: {}", err);
                } else {
                    tracing::info!("keep_authorized_ldap_conn task finished")
                }
            },
            res = keep_ldap_conn(self.clone()) => {
                if let Err(err) = res {
                    tracing::error!("Failed keep_ldap_conn task: {}", err);
                } else {
                    tracing::info!("keep_ldap_conn task finished")
                }
            },
        }

        Ok(())
    }

    async fn get_authorized_ldap(&self) -> Result<Ldap> {
        let state = self.state.lock().await;
        state
            .authorized_ldap
            .clone()
            .with_context(|| "Authorized ldap connection is broken".to_string())
    }

    async fn get_ldap(&self) -> Result<Ldap> {
        let state = self.state.lock().await;
        state
            .ldap
            .clone()
            .with_context(|| "Ldap connection is broken".to_string())
    }

    async fn do_authorized_ldap_request(
        &self,
        query: &str,
        filter: &str,
    ) -> Result<Vec<ResultEntry>> {
        let mut ldap = self.get_authorized_ldap().await?;
        let res = ldap
            .search(
                query,
                ldap3::Scope::Subtree,
                filter,
                vec!["memberOf", "mail", "sAMAccountName"],
            )
            .await?;

        match res.success() {
            Ok((rs, _res)) => Ok(rs),
            Err(err) => Err(anyhow!("Failed ldap request: {err}")),
        }
    }

    pub async fn get_user_info(&self, user_dn: &str) -> Result<Option<AdUserInfo>> {
        let rs = self.do_authorized_ldap_request(user_dn, "(mail=*)").await?;
        if let Some(entry) = rs.first() {
            return Ok(Some(AdUserInfo::try_from(SearchEntry::construct(
                entry.clone(),
            ))?));
        }
        Ok(None)
    }

    pub async fn find_user_info(&self, login_or_mail: String) -> Result<Option<AdUserInfo>> {
        let rs = self
            .do_authorized_ldap_request(
                &self.ldap_settings.users_query,
                &format!("(|(mail={login_or_mail})(sAMAccountName={login_or_mail}))"),
            )
            .await?;

        if let Some(entry) = rs.first() {
            return Ok(Some(AdUserInfo::try_from(SearchEntry::construct(
                entry.clone(),
            ))?));
        }
        Ok(None)
    }

    pub async fn check_authentication(&self, user_dn: &str, password: &SecretString) -> Result<()> {
        if password.expose_secret().is_empty() {
            return Err(anyhow!("Empty password"));
        };
        let mut ldap = self.get_ldap().await?;
        let result = ldap
            .simple_bind(user_dn, password.expose_secret())
            .await
            .and_then(|r| r.success());

        if let Err(err) = result {
            return Err(err.into());
        }
        Ok(())
    }
}

const RECONNECT_INTERVAL: Duration = Duration::from_secs(10);

async fn keep_ldap_conn(users_info: UsersInfo) -> Result<()> {
    loop {
        tracing::info!("Making unauthorized ldap connection");

        let ldap_settings = users_info.ldap_settings.clone();

        match LdapConnAsync::with_settings(ldap_settings.clone().into(), &ldap_settings.url).await {
            Ok((ldap_conn, ldap)) => {
                users_info.state.lock().await.ldap = Some(ldap);
                tracing::info!("Ldap connection created");

                if let Err(err) = ldap_conn.drive().await {
                    tracing::error!("Ldap connection error: {err}");
                } else {
                    tracing::warn!("Ldap connection closed");
                }
            }
            Err(err) => {
                tracing::error!("Unable to get ldap connection: {err}");
            }
        }
        users_info.state.lock().await.ldap = None;
        sleep(RECONNECT_INTERVAL).await;
    }
}

async fn keep_authorized_ldap_conn(users_info: UsersInfo) -> Result<()> {
    loop {
        tracing::info!("Making authorized ldap connection");

        let ldap_settings = users_info.ldap_settings.clone();

        match LdapConnAsync::with_settings(ldap_settings.clone().into(), &ldap_settings.url).await {
            Ok((authorized_ldap_conn, mut authorized_ldap)) => {
                let authorized_ldap_conn_task =
                    spawn(async move { authorized_ldap_conn.drive().await });

                authorized_ldap
                    .simple_bind(&ldap_settings.login, ldap_settings.password.expose_secret())
                    .await
                    .with_context(|| "Unable to bind credentials")?
                    .success()
                    .with_context(|| "Unable to authorize")?;

                users_info.state.lock().await.authorized_ldap = Some(authorized_ldap);
                tracing::info!("Autorized ldap connection created");

                match authorized_ldap_conn_task.await {
                    Err(err) => {
                        tracing::error!("Unable to get authorized_ldap_conn_task result: {err}");
                    }
                    Ok(Err(err)) => {
                        tracing::error!("Autorized ldap connection error: {err}");
                    }
                    Ok(Ok(_)) => {
                        tracing::warn!("Authorized ldap connection closed");
                    }
                };
            }
            Err(err) => {
                tracing::error!("Unable to get authorized ldap connection: {err}");
            }
        }
        users_info.state.lock().await.authorized_ldap = None;
        sleep(RECONNECT_INTERVAL).await;
    }
}
