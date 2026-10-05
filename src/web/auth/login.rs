use askama::Template;
use axum::{
    Form,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_flash::{Flash, IncomingFlashes};
use secrecy::SecretString;
use serde::Deserialize;
use tracing::error;
use tracing::warn;

use crate::{
    AppInfo,
    web::auth::middleware::{AuthSession, Credentials},
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
) -> impl IntoResponse {
    let credentials = Credentials {
        username: form.username,
        password: form.password,
    };
    tracing::Span::current().record("username", tracing::field::display(&credentials.username));

    let user = match session.authenticate(credentials).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (flash.error("Wrong credentials"), Redirect::to("/login")).into_response();
        }
        Err(e) => {
            warn!("Authentication error: {}", e);
            return (flash.error("Something went wrong"), Redirect::to("/login")).into_response();
        }
    };

    if let Err(err) = session.login(&user).await {
        error!("Got unexpected error: {}", err);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Unexpected error").into_response();
    }
    Redirect::to("/hosts").into_response()
}

#[tracing::instrument(skip(session))]
pub async fn logout(mut session: AuthSession) -> impl IntoResponse {
    if session.logout().await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Unexpected error").into_response();
    }
    Redirect::to("/login").into_response()
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
