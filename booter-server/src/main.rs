mod api;
mod api_companions;
mod config;
mod db;
mod email;
mod mijia;
mod mijia_client;
pub mod wt;
pub mod quic;

use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use include_dir::{include_dir, Dir};

pub static ASSETS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../booter-web/dist");

use crate::config::AppConfig;
use booter_common::{ServerToCompanion, ServerToDashboard};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: AppConfig,
    pub companions: Arc<Mutex<HashMap<String, mpsc::Sender<ServerToCompanion>>>>,
    pub dashboards: Arc<Mutex<HashMap<String, mpsc::Sender<ServerToDashboard>>>>,
    pub pending_commands: Arc<Mutex<Vec<(std::time::Instant, ServerToCompanion)>>>,
    pub active_services: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
    pub otps: Arc<Mutex<HashMap<String, (String, std::time::Instant)>>>,
    pub node_shutdown_deadline: Arc<Mutex<Option<std::time::Instant>>>,
    pub last_boot_time: Arc<Mutex<Option<std::time::Instant>>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lettre=trace,booter_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Loading config...");
    let config = config::AppConfig::load().unwrap_or_else(|e| {
        tracing::error!("Failed to load config: {}", e);
        std::process::exit(1);
    });

    tracing::info!("Initializing database at {}...", config.server.database_url);
    let db_pool = db::init_db(&config.server.database_url).await?;
    
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "reset-admin" {
        tracing::info!("Resetting admin TOTP...");
        let secret = totp_rs::Secret::generate_secret();
        let secret_str = secret.to_encoded().to_string();
        sqlx::query("INSERT INTO system_config (key, value) VALUES ('admin_totp_secret', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
            .bind(&secret_str)
            .execute(&db_pool).await?;
        
        let totp = totp_rs::TOTP::new(
            totp_rs::Algorithm::SHA1,
            6,
            1,
            30,
            secret.to_bytes().unwrap(),
            Some("Booter".into()),
            "admin@booter".to_string(),
        ).unwrap();
        let url = totp.get_url();
        println!("\n=== ADMIN TOTP SETUP ===");
        println!("Scan this URL in your Authenticator app (e.g. Google Authenticator, Authy):");
        println!("{}\n", url);
        println!("Or enter this secret manually: {}", secret_str);
        println!("========================\n");
        return Ok(());
    }

    let state = AppState {
        db: db_pool,
        config: config.clone(),
        companions: Arc::new(Mutex::new(HashMap::new())),
        dashboards: Arc::new(Mutex::new(HashMap::new())),
        pending_commands: Arc::new(Mutex::new(Vec::new())),
        active_services: Arc::new(Mutex::new(HashMap::new())),
        otps: Arc::new(Mutex::new(HashMap::new())),
        node_shutdown_deadline: Arc::new(Mutex::new(None)),
        last_boot_time: Arc::new(Mutex::new(None)),
    };

    let app = api::router(state.clone());

    // Start WebTransport server in background
    tokio::spawn(wt::start_wt_server(state.clone()));

    // Start Raw QUIC server in background
    tokio::spawn(quic::start_quic_server(state.clone()));

    let listener = TcpListener::bind(&config.server.bind_addr).await?;
    tracing::info!("Starting booter-server on {}", config.server.bind_addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Shutdown signal received, shutting down gracefully...");
}
