use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use time_banner::build_router;
use time_banner::config::Configuration;
use time_banner_core::geoip::GeoipDb;
use tracing_subscriber::EnvFilter;

/// How long the server waits for in-flight requests to drain after a shutdown
/// signal before forcing exit.
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() {
    // Development-only: Parse dotenv files and expose them as environment variables
    #[cfg(debug_assertions)]
    dotenvy::dotenv().ok();

    let config = Configuration::load();

    // Initialize tracing with RUST_LOG support, falling back to config-based level
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.log_level().to_string()));

    if config.is_production() {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .compact()
            .with_env_filter(env_filter)
            .init();
    }

    let geoip = match config.geoip_db_path.as_deref() {
        Some(path) => match GeoipDb::open(path) {
            Ok(db) => {
                tracing::info!(path = %path.display(), "loaded geoip database");
                Some(Arc::new(db))
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    path = %path.display(),
                    "failed to load geoip database, ?tz=auto will fall back to UTC"
                );
                None
            }
        },
        None => {
            tracing::warn!(
                "GEOIP_DB_PATH not set: ?tz=auto, the favicon, and the index page will fall back to UTC"
            );
            None
        }
    };

    // Bundling an OFL face carries an attribution obligation, so what is
    // compiled in says so on every start rather than only in a source comment.
    for face in time_banner_render::font::bundled_faces() {
        tracing::info!(
            family = face.family,
            license = face.license,
            copyright = face.copyright,
            bytes = face.subset_bytes,
            "bundled face"
        );
    }

    let app = build_router(geoip);

    let addr = SocketAddr::new(config.socket_addr(), config.port);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    let local_addr = listener.local_addr().unwrap();
    let display_host = if local_addr.ip().is_unspecified() {
        "localhost".to_string()
    } else {
        local_addr.ip().to_string()
    };
    tracing::info!("Listening on http://{}:{}", display_host, local_addr.port());

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .unwrap();
}

/// Waits for a shutdown signal, then arms a watchdog that forces the process
/// to exit if the drain (waiting for in-flight requests) runs long.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, draining in-flight requests");
    tokio::spawn(async {
        tokio::time::sleep(SHUTDOWN_DRAIN_TIMEOUT).await;
        tracing::warn!(
            timeout = ?SHUTDOWN_DRAIN_TIMEOUT,
            "drain exceeded timeout, forcing exit"
        );
        std::process::exit(1);
    });
}
