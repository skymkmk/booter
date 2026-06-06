use tokio::sync::mpsc;
use tracing::{info, warn};
use std::sync::Arc;
use quinn::{ServerConfig, Endpoint};
use web_transport_quinn::{Server, Session};

use crate::AppState;
use booter_common::{DashboardToServer, ServerToCompanion, ServerToDashboard};

pub async fn start_wt_server(state: AppState) {
    let cert_path = state.config.server.cert_path.as_ref().expect("Cert path missing");
    let key_path = state.config.server.key_path.as_ref().expect("Key path missing");

    let mut cert_reader = std::io::BufReader::new(std::fs::File::open(cert_path).expect("Failed to open cert"));
    let cert_chain = rustls_pemfile::certs(&mut cert_reader)
        .map(|c| c.expect("Failed to parse cert"))
        .collect::<Vec<_>>();

    let mut key_reader = std::io::BufReader::new(std::fs::File::open(key_path).expect("Failed to open key"));
    let priv_key = rustls_pemfile::private_key(&mut key_reader)
        .expect("Failed to read private key")
        .expect("No private key found in file");

    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, priv_key)
        .unwrap();

    server_crypto.alpn_protocols = vec![b"h3".to_vec()];

    let mut server_config = ServerConfig::with_crypto(Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto).unwrap()));
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_idle_timeout(Some(std::time::Duration::from_secs(20).try_into().unwrap()));
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(3)));
    transport_config.initial_mtu(1200);
    server_config.transport_config(std::sync::Arc::new(transport_config));

    let endpoint = match Endpoint::server(server_config, "[::]:8080".parse().unwrap()) {
        Ok(ep) => ep,
        Err(e) => {
            warn!("Failed to bind WebTransport on UDP 8080: {}", e);
            return;
        }
    };
    info!("Starting WebTransport server on UDP 8080");

    let mut server = Server::new(endpoint);

    loop {
        let request = match server.accept().await {
            Some(req) => req,
            None => break,
        };

        let state_clone = state.clone();
        tokio::spawn(async move {
            let session = match request.ok().await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to accept WebTransport session: {}", e);
                    return;
                }
            };
            info!("WebTransport connection accepted");
            handle_wt_session(session, state_clone).await;
        });
    }
}

async fn handle_wt_session(session: Session, state: AppState) {
    loop {
        match session.accept_bi().await {
            Ok((mut send_stream, recv_stream)) => {
                let state = state.clone();
                tokio::spawn(async move {
                    use tokio::io::AsyncBufReadExt;
                    let mut buf_reader = tokio::io::BufReader::new(recv_stream);
                    let mut first_line = String::new();
                    
                    if buf_reader.read_line(&mut first_line).await.is_err() || first_line.is_empty() {
                        return;
                    }
                    
                    let is_authenticated = match serde_json::from_str::<DashboardToServer>(&first_line) {
                        Ok(DashboardToServer::Auth { token }) => {
                            crate::api::verify_token_impl(&token, None, &state.db).await.is_some()
                        },
                        _ => false,
                    };
                    
                    if !is_authenticated {
                        warn!("WebTransport stream authentication failed");
                        let _ = send_stream.write_all(b"{\"type\":\"command_result\",\"payload\":{\"success\":false,\"message\":\"Authentication failed\"}}\n").await;
                        return;
                    }
                    
                    let (tx, mut rx) = mpsc::channel::<ServerToDashboard>(32);
                    let dash_id = uuid::Uuid::new_v4().to_string();
                    state.dashboards.lock().await.insert(dash_id.clone(), tx.clone());
                    info!("Dashboard authenticated and connected: {}", dash_id);

                    let c_len = state.companions.lock().await.len();
                    let deadline_opt = *state.node_shutdown_deadline.lock().await;
                    let shutdown_deadline = deadline_opt.map(|d| {
                        let now = std::time::Instant::now();
                        let sys_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
                        if d > now {
                            sys_now + (d.duration_since(now).as_secs() as i64)
                        } else {
                            sys_now - (now.duration_since(d).as_secs() as i64)
                        }
                    });
                    let forbidden_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'boot_forbidden_time'")
                        .fetch_optional(&state.db).await.unwrap_or(None);
                    let forbidden_time = forbidden_record.map(|(v,)| v).filter(|v| !v.trim().is_empty());

                    let cooldown_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'boot_cooldown_minutes'")
                        .fetch_optional(&state.db).await.unwrap_or(None);
                    let cooldown_minutes = cooldown_record.and_then(|(v,)| v.parse::<u32>().ok()).unwrap_or(0);
                    
                    let mut cooldown_deadline = None;
                    if cooldown_minutes > 0 {
                        if let Some(last_time) = *state.last_boot_time.lock().await {
                            let elapsed_secs = last_time.elapsed().as_secs_f64();
                            let total_secs = (cooldown_minutes * 60) as f64;
                            if elapsed_secs < total_secs {
                                let remaining = total_secs - elapsed_secs;
                                let sys_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
                                cooldown_deadline = Some(sys_now + remaining as i64);
                            }
                        }
                    }

                    let _ = tx.send(ServerToDashboard::NodeStatus { 
                        online_count: c_len, 
                        shutdown_deadline,
                        forbidden_time,
                        cooldown_deadline,
                    }).await;

                    let rx_task = {
                        let dash_id = dash_id.clone();
                        let state = state.clone();
                        let dashboards = state.dashboards.clone();
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            loop {
                                let mut line = String::new();
                                match buf_reader.read_line(&mut line).await {
                                    Ok(0) => break, // EOF
                                    Ok(_) => {
                                        if let Ok(msg) = serde_json::from_str::<DashboardToServer>(&line) {
                                            match msg {
                                                DashboardToServer::Command { target_id, cmd } => {
                                                    if cmd == "shutdown" {
                                                        let active_services = state.active_services.lock().await;
                                                        let has_active = if let Some(cid) = &target_id {
                                                            active_services.get(cid).map(|m| !m.is_empty()).unwrap_or(false)
                                                        } else {
                                                            active_services.values().any(|m| !m.is_empty())
                                                        };
                                                        if has_active {
                                                            let _ = tx.send(ServerToDashboard::CommandResult { 
                                                                success: false, 
                                                                message: "当前有服务在线，拒绝执行关机指令".into() 
                                                            }).await;
                                                            continue;
                                                        }
                                                    }

                                                    info!("Dashboard {} requested command: {:?}", dash_id, cmd);
                                                    let c_map = state.companions.lock().await;
                                                    let mut sent_count = 0;
                                                    for (cid, sender) in c_map.iter() {
                                                        if target_id.is_none() || target_id.as_ref() == Some(cid) {
                                                            info!("Forwarding command to companion {}", cid);
                                                            let _ = sender.send(ServerToCompanion::Command { target_id: target_id.clone(), cmd: cmd.clone() }).await;
                                                            sent_count += 1;
                                                        }
                                                    }
                                                    
                                                    let _ = tx.send(ServerToDashboard::CommandResult {
                                                        success: true,
                                                        message: format!("已成功向 {} 个节点发送指令", sent_count),
                                                    }).await;
                                                },
                                                DashboardToServer::Auth { .. } => {
                                                    // Already handled during handshake
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to read from dashboard {}: {}", dash_id, e);
                                        break;
                                    }
                                }
                            }
                            dashboards.lock().await.remove(&dash_id);
                            info!("Dashboard disconnected: {}", dash_id);
                        })
                    };

                    let tx_task = {
                        let dash_id = dash_id.clone();
                        let dashboards = state.dashboards.clone();
                        tokio::spawn(async move {
                            while let Some(msg) = rx.recv().await {
                                if let Ok(mut encoded) = serde_json::to_vec(&msg) {
                                    encoded.push(b'\n');
                                    if let Err(e) = send_stream.write_all(&encoded).await {
                                        warn!("Failed to write msg to dashboard {}: {}", dash_id, e);
                                        break;
                                    }
                                }
                            }
                            dashboards.lock().await.remove(&dash_id);
                        })
                    };

                    let _ = tokio::join!(rx_task, tx_task);
                });
            }
            Err(e) => {
                warn!("WebTransport session closed: {}", e);
                break;
            }
        }
    }
}
