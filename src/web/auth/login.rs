use askama::Template;
use axum::{
    Form,
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_flash::{Flash, IncomingFlashes};
use secrecy::SecretString;
use serde::Deserialize;
use tracing::error;
use tracing::warn;

use crate::{
    AppInfo,
    web::{
        auth::middleware::{AuthSession, Credentials},
        flash_redirect,
    },
};

#[derive(Deserialize)]
pub struct FormData {
    username: String,
    password: SecretString,
}

#[tracing::instrument(
    skip(form, flash, session),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    mut session: AuthSession,
    flash: Flash,
    Form(form): Form<FormData>,
) -> axum::response::Result<Redirect> {
    let credentials = Credentials {
        username: form.username,
        password: form.password,
    };
    tracing::Span::current().record("username", tracing::field::display(&credentials.username));

    let user = match session.authenticate(credentials).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Err(flash_redirect("Wrong credentials", "/login", flash));
        }
        Err(e) => {
            warn!("Authentication error: {}", e);
            return Err(flash_redirect("Something went wrong", "/login", flash));
        }
    };

    session.login(&user).await.map_err(|err| {
        error!("Got unexpected error: {}", err);
        "Unexpected error".to_string()
    })?;
    Ok(Redirect::to("/hosts"))
}

#[tracing::instrument(skip(session))]
pub async fn logout(mut session: AuthSession) -> axum::response::Result<Redirect> {
    session
        .logout()
        .await
        .map_err(|_| "Unexpected error".to_string())?;

    Ok(Redirect::to("/login"))
}

#[derive(Template)]
#[template(path = "login.html", escape = "none")]
struct LoginPage {
    error: Option<String>,
    app_info: AppInfo,
}

#[tracing::instrument(skip_all)]
pub async fn login_page(flashes: IncomingFlashes) -> Response {
    let error = flashes.into_iter().next().map(|(_, text)| text.to_string());
    let resp = Html(
        LoginPage {
            error,
            app_info: AppInfo::new(),
        }
        .to_string(),
    );
    (flashes, resp).into_response()
}
