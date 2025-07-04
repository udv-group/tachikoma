use ldap3::Ldap;
use ldap3::{ResultEntry, SearchEntry};
use secrecy::{ExposeSecret, SecretString};

use anyhow::{Result, anyhow};

#[derive(Clone)]
pub struct UsersInfo {
    pub ldap: Ldap,
    pub authorized_ldap: Ldap,
    pub users_query: String,
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
    pub async fn new(ldap: Ldap, authorized_ldap: Ldap, users_query: String) -> Result<Self> {
        Ok(Self {
            ldap,
            users_query,
            authorized_ldap,
        })
    }

    async fn do_authorized_ldap_request(
        &self,
        query: &str,
        filter: &str,
    ) -> Result<Vec<ResultEntry>> {
        let mut ldap = self.authorized_ldap.clone();
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
                &self.users_query,
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
        let mut ldap = self.ldap.clone();
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
