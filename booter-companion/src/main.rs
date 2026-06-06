use booter_common::{CompanionToServer, ServerToCompanion};
use futures_util::{SinkExt, StreamExt};
use rhai::{Engine, Map};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{Duration, sleep};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use tracing::{error, info, warn};

struct DangerVerifier;
impl rustls::client::ServerCertVerifier for DangerVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

fn http_get_json(url: String) -> rhai::Map {
    let mut map = rhai::Map::new();
    match ureq::get(&url).call() {
        Ok(response) => {
            map.insert("status".into(), (response.status() as i64).into());
            if let Ok(json_val) = response.into_json::<serde_json::Value>() {
                map.insert(
                    "body".into(),
                    rhai::serde::to_dynamic(json_val).unwrap_or(rhai::Dynamic::UNIT),
                );
            } else {
                map.insert("body".into(), rhai::Dynamic::UNIT);
            }
        }
        Err(ureq::Error::Status(code, _)) => {
            map.insert("status".into(), (code as i64).into());
            map.insert("body".into(), rhai::Dynamic::UNIT);
        }
        Err(e) => {
            warn!("http_get_json error for {}: {}", url, e);
            map.insert("status".into(), 0_i64.into());
            map.insert("body".into(), rhai::Dynamic::UNIT);
        }
    }
    map
}

fn http_get(url: String) -> rhai::Map {
    let mut map = rhai::Map::new();
    match ureq::get(&url).call() {
        Ok(response) => {
            map.insert("status".into(), (response.status() as i64).into());
            map.insert(
                "body".into(),
                response.into_string().unwrap_or_default().into(),
            );
        }
        Err(ureq::Error::Status(code, response)) => {
            map.insert("status".into(), (code as i64).into());
            map.insert(
                "body".into(),
                response.into_string().unwrap_or_default().into(),
            );
        }
        Err(e) => {
            warn!("http_get error for {}: {}", url, e);
            map.insert("status".into(), 0_i64.into());
            map.insert("body".into(), rhai::Dynamic::UNIT);
        }
    }
    map
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let server_host =
        std::env::var("BOOTER_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let client_id = std::env::var("BOOTER_CLIENT_ID").unwrap_or_else(|_| {
        hostname::get()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });
    let allow_insecure =
        std::env::var("BOOTER_ALLOW_INSECURE").unwrap_or_else(|_| "false".to_string()) == "true";

    info!("Booter Companion starting (Raw QUIC mode)...");
    info!("Target Server: {}", server_host);
    info!("Client ID: {}", client_id);

    loop {
        if let Err(e) = run_client(&server_host, &client_id, allow_insecure).await {
            error!(
                "Connection ended with error: {}. Retrying in 5 seconds...",
                e
            );
        } else {
            warn!("Connection closed cleanly. Retrying in 5 seconds...");
        }
        sleep(Duration::from_secs(5)).await;
    }
}

async fn run_client(
    server_host: &str,
    client_id: &str,
    allow_insecure: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut root_store = rustls::RootCertStore::empty();
    let cert_result = rustls_native_certs::load_native_certs();
    for cert in cert_result.certs {
        let _ = root_store.add(&rustls::Certificate(cert.to_vec()));
    }
    if !cert_result.errors.is_empty() {
        warn!("Some errors occurred while loading native certificates: {:?}", cert_result.errors);
    } else {
        info!("Successfully loaded native root certificates.");
    }

    let mut client_crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    if allow_insecure {
        info!("WARNING: allow_insecure is TRUE. Skipping TLS certificate validation!");
        client_crypto = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(DangerVerifier))
            .with_no_client_auth();
    } else {
        info!("Using strict TLS with native root certificates.");
    }

    client_crypto.alpn_protocols = vec![b"h3".to_vec()];

    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())?;
    let mut client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_idle_timeout(Some(
        std::time::Duration::from_secs(120).try_into().unwrap(),
    ));
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(60)));
    transport_config.initial_mtu(1200);
    client_config.transport_config(std::sync::Arc::new(transport_config));
    endpoint.set_default_client_config(client_config);

    let server_addr_str = format!("{}:2693", server_host);
    let server_addr = tokio::net::lookup_host(&server_addr_str)
        .await?
        .next()
        .ok_or(format!("Failed to resolve host: {}", server_host))?;
    info!("Connecting to QUIC server at {}...", server_addr);

    let server_name =
        std::env::var("BOOTER_SERVER_NAME").unwrap_or_else(|_| server_host.to_string());
    let connection = endpoint.connect(server_addr, &server_name)?.await?;
    info!("Connected successfully!");

    let (send_stream, recv_stream) = connection.open_bi().await?;

    let (tx, mut rx) = mpsc::channel::<CompanionToServer>(32);
    let mut framed_write = FramedWrite::new(send_stream, LengthDelimitedCodec::new());
    let mut framed_read = FramedRead::new(recv_stream, LengthDelimitedCodec::new());

    // Send Hello
    let hello = CompanionToServer::Hello {
        client_id: client_id.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&hello) {
        framed_write.send(json.into()).await?;
    }

    // Shared state for probes
    let probes_state = Arc::new(RwLock::new(HashMap::<String, String>::new()));
    let eval_notify = Arc::new(tokio::sync::Notify::new());

    // Rhai engine task
    let probes_clone = probes_state.clone();
    let tx_clone = tx.clone();
    let client_id_clone = client_id.to_string();
    let notify_clone = eval_notify.clone();
    let engine_task = tokio::spawn(async move {
        let mut engine = Engine::new();
        engine.register_fn("http_get_json", http_get_json);
        engine.register_fn("http_get", http_get);
        loop {
            let probes = probes_clone.read().await.clone();
            if probes.is_empty() {
                let _ = tx_clone
                    .send(CompanionToServer::Status {
                        client_id: client_id_clone.clone(),
                        active: true,
                        active_service: None,
                        probe_type: "heartbeat".to_string(),
                    })
                    .await;
            } else {
                for (probe_name, script) in probes {
                    let ast = match engine.compile(&script) {
                        Ok(a) => a,
                        Err(e) => {
                            warn!("Failed to compile probe {}: {}", probe_name, e);
                            continue;
                        }
                    };

                    match engine.eval_ast::<Map>(&ast) {
                        Ok(result) => {
                            let active = result
                                .get("active")
                                .and_then(|v| v.as_bool().ok())
                                .unwrap_or(false);
                            let service = result.get("service").map(|v| v.to_string());

                            let _ = tx_clone
                                .send(CompanionToServer::Status {
                                    client_id: client_id_clone.clone(),
                                    active,
                                    active_service: service,
                                    probe_type: probe_name,
                                })
                                .await;
                        }
                        Err(e) => {
                            warn!("Error executing probe {}: {}", probe_name, e);
                        }
                    }
                }
            }
            tokio::select! {
                _ = sleep(Duration::from_secs(300)) => {}
                _ = notify_clone.notified() => {
                    info!("Probe evaluation triggered immediately by config update");
                }
            }
        }
    });

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if framed_write.send(json.into()).await.is_err() {
                    break;
                }
            }
        }
    });

    let probes_ref = probes_state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(bytes)) = framed_read.next().await {
            if let Ok(line) = String::from_utf8(bytes.to_vec()) {
                if let Ok(cmd) = serde_json::from_str::<ServerToCompanion>(&line) {
                    match cmd {
                        ServerToCompanion::Command { target_id: _, cmd } => {
                            info!("Received command from server: {}", cmd);
                            if cmd == "shutdown" {
                                info!("Executing shutdown sequence!");
                                #[cfg(target_os = "windows")]
                                {
                                    let _ = std::process::Command::new("shutdown")
                                        .args(["/s", "/t", "300"])
                                        .spawn();
                                }
                                #[cfg(not(target_os = "windows"))]
                                {
                                    let _ = std::process::Command::new("shutdown")
                                        .args(["-h", "now"])
                                        .spawn();
                                }
                            }
                        }
                        ServerToCompanion::ConfigUpdate { scripts } => {
                            info!("Received ConfigUpdate with {} scripts", scripts.len());
                            *probes_ref.write().await = scripts;
                            eval_notify.notify_one();
                        }
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
            engine_task.abort();
        },
        _ = &mut recv_task => {
            send_task.abort();
            engine_task.abort();
        },
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl-C (SIGINT), sending Bye message...");
            let bye = CompanionToServer::Bye {
                client_id: client_id.to_string(),
            };
            let _ = tx.send(bye).await;
            // Allow a short time for the send_task to process and flush
            sleep(Duration::from_millis(500)).await;
            std::process::exit(0);
        }
    }

    Ok(())
}
