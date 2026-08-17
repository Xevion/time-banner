use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr};
use tracing::Level;

/// Application environment configuration.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Production,
    Development,
}

/// Main configuration struct parsed from environment variables.
///
/// Environment variables:
/// - `ENV`: "production" or "development" (default: development)
/// - `PORT`: TCP port number (default: 3000)
#[derive(Deserialize, Debug)]
pub struct Configuration {
    #[serde(default = "default_env")]
    pub env: Environment,

    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    3000
}

fn default_env() -> Environment {
    Environment::Development
}

impl Configuration {
    /// Loads configuration from environment variables using figment.
    pub fn load() -> Self {
        use figment::{Figment, providers::Env};

        Figment::new()
            .merge(Env::raw())
            .extract()
            .expect("Failed to parse configuration")
    }

    /// Returns the IP address to bind to based on environment.
    ///
    /// - Production: 0.0.0.0 (all interfaces)
    /// - Development: 127.0.0.1 (localhost only)
    pub fn socket_addr(&self) -> IpAddr {
        match self.env {
            Environment::Production => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            Environment::Development => IpAddr::V4(Ipv4Addr::LOCALHOST),
        }
    }

    /// Returns the appropriate log level for the environment.
    ///
    /// - Production: INFO
    /// - Development: DEBUG
    pub fn log_level(&self) -> Level {
        match self.env {
            Environment::Production => Level::INFO,
            Environment::Development => Level::DEBUG,
        }
    }

    /// Returns true if the application is running in production.
    pub fn is_production(&self) -> bool {
        matches!(self.env, Environment::Production)
    }
}
