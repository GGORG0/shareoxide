use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
};

use color_eyre::{eyre::Context as _, Section as _};
use config::{Config, ConfigError, Environment, File};
use http::Uri;
use openidconnect::{ClientId, ClientSecret, IssuerUrl};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString, IntoStaticStr};
use tracing::warn;

const ENV_PREFIX: &str = "SO";
const ENV_SEPARATOR: &str = "_";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ListenAddress {
    Single(SocketAddr),
    Multiple(Vec<SocketAddr>),
}

impl Default for ListenAddress {
    fn default() -> Self {
        const DEFAULT_PORT: u16 = 8080;

        ListenAddress::Multiple(vec![
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), DEFAULT_PORT),
            SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), DEFAULT_PORT),
        ])
    }
}

impl From<ListenAddress> for Vec<SocketAddr> {
    fn from(val: ListenAddress) -> Self {
        match val {
            ListenAddress::Single(addr) => vec![addr],
            ListenAddress::Multiple(addrs) => addrs,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct General {
    pub listen_address: ListenAddress,

    #[serde(with = "http_serde_ext::uri")]
    pub public_url: Uri,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Db {
    pub endpoint: String,

    pub namespace: String,
    pub database: String,

    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Oidc {
    pub issuer: IssuerUrl,
    pub client_id: ClientId,
    pub client_secret: Option<ClientSecret>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Settings {
    pub general: General,
    pub db: Db,
    pub oidc: Oidc,
}

pub type ArcSettings = Arc<Settings>;

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let custom_config_file = env_var("CONFIG_FILE").ok();
        let environment_type = get_environment_type();

        if environment_type == EnvironmentType::Development {
            warn!("Running in development mode");
        }

        let mut settings = Config::builder()
            .add_source(File::with_name("config").required(custom_config_file.is_none()));

        if let Some(file) = custom_config_file {
            settings = settings.add_source(File::with_name(&file));
        }

        settings = settings
            .add_source(File::with_name(&format!("config-{environment_type}")).required(false))
            .add_source(File::with_name("config-local").required(false))
            .add_source(Environment::with_prefix(ENV_PREFIX).separator(ENV_SEPARATOR));

        settings.build()?.try_deserialize()
    }

    pub fn try_load() -> color_eyre::Result<Self> {
        let res = Settings::new();

        let add_suggestion = matches!(
            &res,
            Err(ConfigError::Foreign(foreign_error))
                if matches!(
                    foreign_error.downcast_ref::<std::io::Error>(),
                    Some(io_error)
                        if io_error.kind() == std::io::ErrorKind::NotFound
                            && io_error.get_ref().is_some_and(|custom_error| {
                                let custom_error = custom_error.to_string();
                                custom_error.starts_with("configuration file \"")
                                    && custom_error.ends_with("\" not found")
                            })
                )
        );

        let mut res = res.wrap_err("failed to load settings");

        if add_suggestion && !std::path::Path::new("config.toml").exists() {
            let example_settings = Settings::example();
            let example_settings = toml::to_string_pretty(&example_settings)?;

            std::fs::write("config.toml", example_settings)?;

            res = res.suggestion("An example configuration file has been created at `config.toml` in the current directory.");
        }

        res
    }

    pub fn example() -> Self {
        Self {
            general: General {
                listen_address: ListenAddress::default(),
                public_url: "http://localhost:8080"
                    .parse()
                    .expect("hardcoded uri should parse"),
            },
            db: Db {
                endpoint: "ws://localhost:8000".to_string(),

                namespace: "shareoxide".to_string(),
                database: "shareoxide".to_string(),

                username: "root".to_string(),
                password: "root".to_string(),
            },
            oidc: Oidc {
                issuer: IssuerUrl::new("https://example.com".to_string()).unwrap(),
                client_id: ClientId::new("client_id".to_string()),
                client_secret: Some(ClientSecret::new("client_secret".to_string())),
            },
        }
    }
}

#[derive(Debug, Eq, PartialEq, EnumString, Display, AsRefStr, IntoStaticStr)]
#[strum(ascii_case_insensitive, serialize_all = "snake_case")]
enum EnvironmentType {
    #[strum(serialize = "development", serialize = "dev", serialize = "d")]
    Development,

    #[strum(serialize = "production", serialize = "prod", serialize = "p")]
    Production,
}

fn get_environment_type() -> EnvironmentType {
    let from_env = env_var("ENVIRONMENT")
        .inspect_err(|err| {
            if let std::env::VarError::NotUnicode(_) = err {
                warn!(
                    "Environment variable '{}' is not valid unicode",
                    env_name("ENVIRONMENT")
                );
            }
        })
        .ok()
        .map(|val| val.trim().to_string());

    let from_env = from_env.and_then(|env| env.parse::<EnvironmentType>().inspect_err(|err| {
        warn!(error = ?err, "Environment variable '{}' is not a valid environment type", env_name("ENVIRONMENT"));
    }).ok());

    from_env.unwrap_or({
        if cfg!(debug_assertions) {
            EnvironmentType::Development
        } else {
            EnvironmentType::Production
        }
    })
}

pub fn env_name(name: &str) -> String {
    format!("{}{}{}", ENV_PREFIX, ENV_SEPARATOR, name.to_uppercase())
}

pub fn env_var(name: &str) -> Result<String, std::env::VarError> {
    std::env::var(env_name(name))
}
