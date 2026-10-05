use rand::Rng;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::db::{
    Registry,
    models::{ApiToken, ApiTokenId, UserId},
};

#[derive(Error, Debug)]
pub enum ApiTokenError {
    #[error("Database error")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Token not found")]
    NotFound,

    #[error("API token limit is reached")]
    LimitExceeded,
}

#[derive(Clone)]
pub struct ApiTokensService {
    registry: Registry,
    api_token_limit: usize,
}

#[derive(Debug, Clone)]
pub struct CreatedApiToken {
    pub token: ApiToken,
    pub plaintext: String,
}

pub fn hash_token(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn generate_plaintext() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let secret = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!("tk_{secret}")
}

impl ApiTokensService {
    pub fn new(registry: Registry, api_token_limit: usize) -> Self {
        Self {
            registry,
            api_token_limit,
        }
    }

    pub async fn create(
        &self,
        user_id: &UserId,
        name: Option<&str>,
    ) -> Result<CreatedApiToken, ApiTokenError> {
        let plaintext = generate_plaintext();
        let token_hash = hash_token(&plaintext);
        let name = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_owned());

        let mut tx = self.registry.begin().await?;
        let existing = tx.list_api_tokens_for_user(user_id).await?;
        if existing.len() >= self.api_token_limit {
            return Err(ApiTokenError::LimitExceeded);
        }
        let token = tx
            .create_api_token(user_id, name.as_deref(), &token_hash)
            .await?;
        tx.commit().await?;

        Ok(CreatedApiToken { token, plaintext })
    }

    pub async fn list_for_user(&self, user_id: &UserId) -> Result<Vec<ApiToken>, ApiTokenError> {
        let mut tx = self.registry.begin().await?;
        let tokens = tx.list_api_tokens_for_user(user_id).await?;
        tx.commit().await?;
        Ok(tokens)
    }

    pub async fn find_user_id_by_plaintext(
        &self,
        plaintext: &str,
    ) -> Result<Option<UserId>, ApiTokenError> {
        let token_hash = hash_token(plaintext);
        let mut tx = self.registry.begin().await?;
        let token = tx.get_api_token_by_hash(&token_hash).await?;
        tx.commit().await?;
        Ok(token.map(|t| t.user_id))
    }

    pub async fn revoke(
        &self,
        token_id: &ApiTokenId,
        user_id: &UserId,
    ) -> Result<(), ApiTokenError> {
        let mut tx = self.registry.begin().await?;
        let deleted = tx.delete_api_token_for_user(token_id, user_id).await?;
        tx.commit().await?;
        if deleted {
            Ok(())
        } else {
            Err(ApiTokenError::NotFound)
        }
    }
}
