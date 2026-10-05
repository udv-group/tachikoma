use askama::Template;
use axum::{
    Extension,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
};
use axum_extra::extract::Form;
use axum_flash::{Flash, IncomingFlashes};
use axum_login::AuthUser;
use serde::Deserialize;

use super::AuthLink;
use super::auth::middleware::User;
use super::templates::{HostsPage, TokensPage, TokensTokenInfo};
use crate::AppInfo;
use crate::db::models::ApiTokenId;
use crate::logic::api_tokens::ApiTokensService;

#[derive(Deserialize)]
pub struct CreateTokenForm {
    #[serde(default)]
    name: String,
}

pub async fn tokens_page(
    State(api_tokens_service): State<ApiTokensService>,
    State(AuthLink(auth_link)): State<AuthLink>,
    flashes: IncomingFlashes,
    Extension(user): Extension<User>,
) -> impl IntoResponse {
    let tokens = api_tokens_service
        .list_for_user(&user.id().into())
        .await
        .unwrap_or_default();

    let mut created_token = None;
    let mut error = None;
    for (level, msg) in flashes.iter() {
        match level {
            axum_flash::Level::Success => created_token = Some(msg.to_owned()),
            _ => error = Some(msg.to_owned()),
        }
    }

    let page = TokensPage {
        tokens: tokens
            .into_iter()
            .map(|t| TokensTokenInfo {
                id: t.id,
                name: t.name,
                created_at: t.created_at.to_rfc3339(),
            })
            .collect(),
        created_token,
        error,
    };

    let page = HostsPage {
        user: user.into(),
        auth_link,
        page,
        app_info: AppInfo::new(),
    };

    (flashes, Html(page.render().unwrap()))
}

pub async fn create_token(
    State(api_tokens_service): State<ApiTokensService>,
    flash: Flash,
    Extension(user): Extension<User>,
    Form(form): Form<CreateTokenForm>,
) -> impl IntoResponse {
    match api_tokens_service
        .create(&user.id().into(), Some(form.name.as_str()))
        .await
    {
        Ok(created) => (flash.success(created.plaintext), Redirect::to("/tokens")).into_response(),
        Err(e) => (flash.error(e.to_string()), Redirect::to("/tokens")).into_response(),
    }
}

pub async fn revoke_token(
    State(api_tokens_service): State<ApiTokensService>,
    flash: Flash,
    Extension(user): Extension<User>,
    Path(token_id): Path<ApiTokenId>,
) -> impl IntoResponse {
    match api_tokens_service
        .revoke(&token_id, &user.id().into())
        .await
    {
        Ok(()) => Redirect::to("/tokens").into_response(),
        Err(e) => (flash.error(e.to_string()), Redirect::to("/tokens")).into_response(),
    }
}
