mod api;
mod auth;
mod hosts;
mod templates;
mod tokens;

use axum::http::StatusCode;
use axum::{
    Router,
    body::Body,
    extract::{FromRef, MatchedPath, Request},
    middleware,
    response::{ErrorResponse, IntoResponse, Redirect},
    routing::{MethodRouter, get, post},
};

use axum_extra::extract::cookie::Key;
use axum_flash::Flash;
use axum_login::AuthManagerLayerBuilder;
use md5::{Digest, Md5};
use std::{convert::Infallible, net::SocketAddr};
use tokio::net::TcpListener;

use self::auth::{
    bearer::bearer_auth_middleware,
    login,
    middleware::{Backend, auth_middleware},
};
use crate::ldap::UsersInfo;
use crate::{
    configuration::Settings,
    db::Registry,
    logic::{
        api_tokens::ApiTokensService, groups::GroupsService, hosts::HostsService,
        users::UsersService,
    },
};
use tower_http::trace::TraceLayer;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer, cookie::time::Duration};
use tracing::info;
use uuid::Uuid;

#[derive(FromRef, Clone)]
struct AppState {
    hosts_service: HostsService,
    groups_service: GroupsService,
    users_service: UsersService,
    api_tokens_service: ApiTokensService,
    flash_config: axum_flash::Config,
    auth_link: AuthLink,
    registry: Registry,
    users_info: UsersInfo,
}

#[derive(Clone)]
pub struct AuthLink(pub Option<String>);

pub struct Application {
    listening_addr: SocketAddr,
    server: Server,
}

impl Application {
    pub async fn build(
        settings: &Settings,
        registry: Registry,
        users_info: UsersInfo,
        auth_link: Option<String>,
    ) -> Result<Application, anyhow::Error> {
        let tracing_layer = TraceLayer::new_for_http().make_span_with(|req: &Request<Body>| {
            let method = req.method();
            let uri = req.uri();
            let matched_path = req.extensions().get::<MatchedPath>().map(|p| p.as_str());

            tracing::debug_span!("http-request", %method, %uri, matched_path, request_id = %Uuid::new_v4())
        });

        let session_layer = SessionManagerLayer::new(MemoryStore::default())
            .with_secure(true)
            .with_expiry(Expiry::OnInactivity(Duration::days(7)));

        let auth_layer = AuthManagerLayerBuilder::new(
            Backend::new(registry.clone(), users_info.clone()),
            session_layer,
        )
        .build();

        let assets_router = Router::new()
            .route(
                "/assets/htmx.min.js",
                cached_asset(
                    include_bytes!("../../assets/htmx.min.js"),
                    "text/javascript",
                ),
            )
            .route(
                "/assets/tailwindcss.css",
                cached_asset(include_bytes!("../../assets/tailwindcss.css"), "text/css"),
            )
            .route(
                "/assets/tachikoma.png",
                get(|| async {
                    (
                        [(axum::http::header::CONTENT_TYPE, "image/png")],
                        include_bytes!("../../assets/tachikoma.png"),
                    )
                        .into_response()
                }),
            );

        let authed_router = Router::new()
            .route("/logout", get(login::logout))
            .route("/hosts", get(hosts::get_hosts))
            .route("/hosts/all", get(hosts::get_all_hosts))
            .route("/hosts/lease", post(hosts::lease_hosts))
            .route("/hosts/lease/random", post(hosts::lease_random))
            .route("/hosts/release", post(hosts::release_hosts))
            .route("/hosts/release/all", post(hosts::release_all))
            .route(
                "/tokens",
                get(tokens::tokens_page).post(tokens::create_token),
            )
            .route("/tokens/{id}/revoke", post(tokens::revoke_token));

        let state = AppState {
            hosts_service: HostsService::new(registry.clone(), settings.app.lease_limit),
            groups_service: GroupsService::new(registry.clone()),
            users_service: UsersService::new(registry.clone()),
            api_tokens_service: ApiTokensService::new(
                registry.clone(),
                settings.app.api_token_limit,
            ),
            flash_config: axum_flash::Config::new(Key::derive_from(&settings.app.hmac_secret)),
            auth_link: AuthLink(auth_link),
            registry,
            users_info,
        };

        let api_router = Router::new()
            .route("/api/v1/groups", get(api::list_groups))
            .route("/api/v1/hosts/available", get(api::list_available_hosts))
            .route("/api/v1/hosts/leased", get(api::list_my_leased_hosts))
            .route("/api/v1/hosts", get(api::list_all_hosts))
            .route("/api/v1/hosts/lease", post(api::lease_hosts))
            .route("/api/v1/hosts/lease/random", post(api::lease_random))
            .route("/api/v1/hosts/release", post(api::release_hosts))
            .route("/api/v1/hosts/release/all", post(api::release_all))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                bearer_auth_middleware,
            ));

        let app = Router::new()
            .route("/login", post(login::login).get(login::login_page))
            .route("/hosts/leased", get(hosts::get_hosts_json))
            .merge(assets_router)
            .merge(authed_router.route_layer(middleware::from_fn(auth_middleware)))
            .merge(api_router)
            .fallback(|| async { Redirect::to("/hosts").into_response() })
            .layer(auth_layer)
            .layer(tracing_layer)
            .with_state(state);

        let listener = TcpListener::bind(settings.app.socket_addr()).await?;
        Ok(Self {
            listening_addr: listener.local_addr()?,
            server: Server::new(listener, app),
        })
    }
    pub async fn serve_forever(self) -> Result<(), std::io::Error> {
        info!("Web server is listening on {}", self.listening_addr);
        self.server.serve().await
    }
    pub fn listening_addr(&self) -> SocketAddr {
        self.listening_addr
    }
}

struct Server {
    listener: TcpListener,
    app: Router,
}
impl Server {
    pub fn new(listener: TcpListener, app: Router) -> Self {
        Self { listener, app }
    }

    pub async fn serve(self) -> Result<(), std::io::Error> {
        axum::serve(self.listener, self.app).await
    }
}

pub fn flash_redirect(msg: &str, path: &str, flash: Flash) -> ErrorResponse {
    (flash.error(msg), Redirect::to(path))
        .into_response()
        .into()
}

pub fn cached_asset<S>(
    content: &'static [u8],
    content_type: &'static str,
) -> MethodRouter<S, Infallible>
where
    S: Clone + Send + Sync + 'static,
{
    let mut hasher = Md5::new();
    hasher.update(content);

    let content_hash = format!("{:x}", hasher.finalize());
    get(move |request: axum::extract::Request| async move {
        if let Some(header_value) = request.headers().get(axum::http::header::IF_NONE_MATCH)
            && header_value.to_str().unwrap_or("").eq(&content_hash)
        {
            return StatusCode::NOT_MODIFIED.into_response();
        }

        (
            [
                (axum::http::header::CONTENT_TYPE, content_type),
                (axum::http::header::ETAG, &content_hash),
                (
                    axum::http::header::CACHE_CONTROL,
                    "no-cache, must-revalidate",
                ),
            ],
            content,
        )
            .into_response()
    })
}
