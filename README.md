# TACHIKOMA

Simple and opinionated web server to facilitate the reuse of development servers and infrastructure. 

Named after adorable robots from [Ghost in the Shell](https://en.wikipedia.org/wiki/Tachikoma)

# Deployment

[Releases](https://github.com/udv-group/tachikoma/releases) page has two prebuild binaries for Linux: `tachikama` which includes telegram integration and `sever` which just runs a web server.

App is expected to be deployed in the private network. It is recommended to use a reverse proxy (like Nginx) to enable TLS and Docker to run a web server. For integration with telegram also network access
to telegram servers is required.

Web server is using LDAP to manage auth and is designed to be used with AD servers. 

## Configuration

### Environment variables

The application loads a local `.env` file automatically if it exists. See [.env.example](.env.example) for a development example.

Runtime variables:

- `APP_ENVIRONMENT` - selects configuration profile. Supported values: `local` (default) and `production`.
- `CONFIG_DIR` - directory with configuration files. Defaults to `configuration` for `local` and `/etc/tachikoma` for `production`.
- `INCLUDE_SPAN_EVENTS` - if set to `true`, tracing logs include span enter/exit events.
- `RUST_LOG` - optional tracing filter, for example `RUST_LOG=debug`. Defaults to `info`.

Configuration can also be overridden via environment variables with the `APP_` prefix. This is useful in production when you do not want to bake secrets into TOML files.

Examples:

- `APP_DATABASE__HOST`
- `APP_DATABASE__PORT`
- `APP_DATABASE__USERNAME`
- `APP_DATABASE__PASSWORD`
- `APP_DATABASE__DATABASE_NAME`
- `APP_DATABASE__REQUIRE_SSL`
- `APP_LDAP__URL`
- `APP_LDAP__USE_TLS`
- `APP_LDAP__NO_TLS_VERIFY`
- `APP_LDAP__LOGIN`
- `APP_LDAP__PASSWORD`
- `APP_LDAP__USERS_QUERY`
- `APP_APP__HOST`
- `APP_APP__PORT`
- `APP_APP__LEASE_LIMIT`
- `APP_APP__HMAC_SECRET`

Development-only variables:

- `DATABASE_URL` - used by `sqlx`, migrations, and helper scripts such as [scripts/init_db.sh](scripts/init_db.sh). It is not used by the application runtime configuration.
- `SQLX_OFFLINE` - useful for local development with `sqlx`; present in `.env.example`.

### Configuration files

Consult [base.toml](configuration/base.toml) for a list of all possible configuration options.

Both `base.toml` and `production.toml` (or `local.toml` for `APP_ENVIRONMENT=local`) need to be present in `CONFIG_DIR`.

Configuration presidence is as follows: `env` > `production.toml`/`local.toml` > `base.toml`. 


# TL;DR -- Local Setup

```bash
# 1. Install dependencies (Arch/Manjaro)
sudo pacman -S openldap postgresql
cargo install sqlx-cli --no-default-features --features postgres

# 2. Start PostgreSQL and run migrations
scripts/init_db.sh
# If Postgres is already running natively:
# SKIP_START=true scripts/init_db.sh

# 3. Set up and start LDAP
just ldap-setup
just ldap-start

# 4. Create .env from example
cp .env.example .env

# 5. Run
cargo run --bin tachikoma
```

Open http://localhost:8080 and log in with `jdoe@example.org` / `test` (default LDAP user from [user.ldif](ldap/user.ldif)).

To link Telegram: copy link code from the web UI, send `/start` to the bot, then paste the code.

# Development

Prerequisites:
- Up to date Rust compiler ([rustup](https://www.rust-lang.org/tools/install) is strongly recommended)
- Postgres (>=13)
- [sqlx-cli](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md)
- LDAP server (slapd)
- [NPM](https://docs.npmjs.com/downloading-and-installing-node-js-and-npm)

To initialize the database and run migrations use [init_db.sh](scripts/init_db.sh) script. 
If you don't have podman or running a native server you can skip the initialization stage with `SKIP_START=true` environment variable to just run migrations.

For LDAP you'll need `slapd` and `ldap-utils`. To set up the database run `just ldap-setup`. After it finishes run `just ldap-start` to start `slapd`.

To allow sqlx to connect to your development database create `.env` file in the root of this repository with a line 
```
DATABASE_URL=postgres://postgres:password@localhost:5432/tachikoma
```
Replace `localhost:5432` with the address and port of your postgres server

### Notes about LDAP/AD server

Setup is a bit cursed, info mainly pulled from [Arch docs](https://wiki.archlinux.org/title/OpenLDAP#The_server)
and [this article](https://www.adimian.com/blog/how-to-enable-memberof-using-openldap/).

To add users/groups or change credentials edit [user.ldif](tests/ldap-setup/user.ldif). To change credentials modify `userPassword` and `mail`.
Don't forget to run `just ldap-clean` and `just ldap-setup` to apply your changes.

### Tailwind

To update css file

```bash
> npm install
> just build-css
```

Source of tailwind styles located here - [src](./tailwind_src/)
