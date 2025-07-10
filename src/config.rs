use serde::Deserialize;
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
    /// Returns the socket address to bind to based on environment.
    ///
    /// - Production: 0.0.0.0 (all interfaces)
    /// - Development: 127.0.0.1 (localhost only)
    pub fn socket_addr(&self) -> [u8; 4] {
        match self.env {
            Environment::Production => {
                let socket = [0, 0, 0, 0];
                tracing::info!(
                    "Starting Production on {:?}:{}",
                    socket.as_slice(),
                    self.port
                );
                socket
            }
            Environment::Development => {
                let socket = [127, 0, 0, 1];
                tracing::info!(
                    "Starting Development on {:?}:{}",
                    socket.as_slice(),
                    self.port
                );
                socket
            }
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
}
