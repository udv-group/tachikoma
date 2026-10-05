use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_login::AuthUser;
use serde::Serialize;

use crate::{
    db::Registry,
    ldap::UsersInfo,
    logic::api_tokens::ApiTokensService,
    web::auth::middleware::{AuthError, User},
};

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

pub async fn bearer_auth_middleware(
    State(registry): State<Registry>,
    State(users_info): State<UsersInfo>,
    State(api_tokens_service): State<ApiTokensService>,
    request: Request,
    next: Next,
) -> Response {
    match authenticate_bearer(
        &registry,
        &users_info,
        &api_tokens_service,
        request.headers().get(AUTHORIZATION),
    )
    .await
    {
        Ok(Some(user)) => {
            let mut request = request;
            let span = tracing::Span::current();
            span.record("username", tracing::field::display(&user.username));
            span.record("user_id", tracing::field::display(&user.id()));
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody {
                error: "Unauthorized".to_string(),
            }),
        )
            .into_response(),
        Err(err) => {
            tracing::error!("Bearer auth failed: {err}");
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody {
                    error: "Unauthorized".to_string(),
                }),
            )
                .into_response()
        }
    }
}

pub async fn authenticate_bearer(
    registry: &Registry,
    users_info: &UsersInfo,
    api_tokens_service: &ApiTokensService,
    auth_header: Option<&axum::http::HeaderValue>,
) -> Result<Option<User>, AuthError> {
    let Some(header_value) = auth_header else {
        return Ok(None);
    };
    let Ok(header) = header_value.to_str() else {
        return Ok(None);
    };
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Ok(None);
    };
    let token = token.trim();
    if token.is_empty() {
        return Ok(None);
    }

    let Some(user_id) = api_tokens_service
        .find_user_id_by_plaintext(token)
        .await
        .map_err(|e| AuthError::AnyhowErr(e.into()))?
    else {
        return Ok(None);
    };

    let user = registry.begin().await?.get_user_by_id(&user_id).await?;
    let Some(user) = user else {
        return Ok(None);
    };

    match users_info.get_user_info(&user.dn).await {
        Ok(Some(u_info)) => Ok(Some((user, u_info.groups).into())),
        Ok(None) => {
            tracing::warn!(
                "Missed user info for API token user '{}' ({})",
                user.dn,
                user.email
            );
            Ok(None)
        }
        Err(err) => {
            tracing::error!(
                "Unable to get user info for API token user '{}' ({}): {}",
                user.dn,
                user.email,
                err
            );
            Err(AuthError::AnyhowErr(err))
        }
    }
}
