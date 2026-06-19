use crate::AppState;
use booter_common::{CompanionToServer, ServerToCompanion, ServerToDashboard};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use tracing::{error, info, warn};

fn reset_deadline_without_shortening(
    current: Option<std::time::Instant>,
    reset_deadline: std::time::Instant,
    force_reset: bool,
) -> std::time::Instant {
    match current {
        None => reset_deadline,
        Some(existing) if force_reset && reset_deadline > existing => reset_deadline,
        Some(existing) => existing,
    }
}

pub async fn start_quic_server(state: AppState) {
    let cert_path = state
        .config
        .server
        .cert_path
        .as_ref()
        .expect("Cert path missing");
    let key_path = state
        .config
        .server
        .key_path
        .as_ref()
        .expect("Key path missing");

    let mut cert_reader =
        std::io::BufReader::new(std::fs::File::open(cert_path).expect("Failed to open cert"));
    let cert_chain = rustls_pemfile::certs(&mut cert_reader)
        .map(|c| c.expect("Failed to parse cert"))
        .collect::<Vec<_>>();

    let mut key_reader =
        std::io::BufReader::new(std::fs::File::open(key_path).expect("Failed to open key"));
    let priv_key = rustls_pemfile::private_key(&mut key_reader)
        .expect("Failed to read private key")
        .expect("No private key found in file");

    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, priv_key)
        .unwrap();

    server_crypto.alpn_protocols = vec![b"h3".to_vec()];

    let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto).unwrap(),
    ));
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_idle_timeout(Some(
        std::time::Duration::from_secs(120).try_into().unwrap(),
    ));
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(60)));
    transport_config.initial_mtu(1200);
    server_config.transport_config(std::sync::Arc::new(transport_config));

    let endpoint = match quinn::Endpoint::server(server_config, "[::]:2693".parse().unwrap()) {
        Ok(ep) => ep,
        Err(e) => {
            error!("Failed to bind QUIC on UDP 2693: {}", e);
            return;
        }
    };
    info!("Starting QUIC server on UDP 2693 for Companions");

    let state_bg = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let mut should_shutdown = false;
            {
                let mut deadline_lock = state_bg.node_shutdown_deadline.lock().await;
                if let Some(deadline) = *deadline_lock {
                    if std::time::Instant::now() >= deadline {
                        info!("Global node_shutdown_deadline reached!");
                        should_shutdown = true;
                        *deadline_lock = None; // Reset so we don't spam
                    }
                }
            }

            if should_shutdown {
                let mut companions = state_bg.companions.lock().await;
                let mut sent_count = 0;
                for (cid, tx) in companions.iter_mut() {
                    info!("Sending auto-shutdown to companion: {}", cid);
                    if tx.try_send(ServerToCompanion::Command {
                        target_id: None,
                        cmd: "shutdown".into(),
                    }).is_ok() {
                        sent_count += 1;
                    }
                }
                drop(companions); // Explicitly drop before broadcasting

                if sent_count > 0 {
                    let _ = sqlx::query(
                        "INSERT INTO user_logs (email, action) VALUES ('system', 'AutoShutdown')",
                    )
                    .execute(&state_bg.db)
                    .await;
                }

                broadcast_node_status(&state_bg).await;
            }
        }
    });

    while let Some(incoming) = endpoint.accept().await {
        let state_clone = state.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(connection) => {
                    info!(
                        "QUIC connection accepted from {}",
                        connection.remote_address()
                    );
                    handle_quic_connection(connection, state_clone).await;
                }
                Err(e) => {
                    warn!("Incoming QUIC connection failed: {}", e);
                }
            }
        });
    }
}

pub async fn broadcast_node_status(state: &AppState) {
    let c_len = state.companions.lock().await.len();

    let deadline_opt = *state.node_shutdown_deadline.lock().await;
    let shutdown_deadline = deadline_opt.map(|d| {
        let now = std::time::Instant::now();
        let sys_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        if d > now {
            sys_now + (d.duration_since(now).as_secs() as i64)
        } else {
            sys_now - (now.duration_since(d).as_secs() as i64)
        }
    });

    let forbidden_record: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_config WHERE key = 'boot_forbidden_time'")
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    let forbidden_time = forbidden_record
        .map(|(v,)| v)
        .filter(|v| !v.trim().is_empty());

    let cooldown_record: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_config WHERE key = 'boot_cooldown_minutes'")
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    let cooldown_minutes = cooldown_record
        .and_then(|(v,)| v.parse::<u32>().ok())
        .unwrap_or(0);

    let mut cooldown_deadline = None;
    if cooldown_minutes > 0 {
        if let Some(last_time) = *state.last_boot_time.lock().await {
            let elapsed_secs = last_time.elapsed().as_secs_f64();
            let total_secs = (cooldown_minutes * 60) as f64;
            if elapsed_secs < total_secs {
                let remaining = total_secs - elapsed_secs;
                let sys_now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                cooldown_deadline = Some(sys_now + remaining as i64);
            }
        }
    }

    let absolute_cooldown_record: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM system_config WHERE key = 'absolute_boot_cooldown_minutes'",
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let absolute_cooldown_minutes = absolute_cooldown_record
        .and_then(|(v,)| v.parse::<u32>().ok())
        .unwrap_or(0);

    let mut absolute_cooldown_deadline = None;
    if absolute_cooldown_minutes > 0 {
        if let Some(last_time) = *state.last_offline_time.lock().await {
            let elapsed_secs = last_time.elapsed().as_secs_f64();
            let total_secs = (absolute_cooldown_minutes * 60) as f64;
            if elapsed_secs < total_secs {
                let remaining = total_secs - elapsed_secs;
                let sys_now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                absolute_cooldown_deadline = Some(sys_now + remaining as i64);
            }
        }
    }

    let mut dashboards = state.dashboards.lock().await;
    for (_, dash_tx) in dashboards.iter_mut() {
        let _ = dash_tx
            .send(ServerToDashboard::NodeStatus {
                online_count: c_len,
                shutdown_deadline,
                forbidden_time: forbidden_time.clone(),
                cooldown_deadline,
                absolute_cooldown_deadline,
            })
            .await;
    }
}

pub async fn check_and_update_global_deadline(state: &AppState, force_reset: bool) {
    let records: Vec<(String, String)> = sqlx::query_as("SELECT id, scripts FROM companions")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let mut has_probes = false;
    let online_companions = state.companions.lock().await;

    for (id, scripts) in records {
        if online_companions.contains_key(&id) {
            let parsed: HashMap<String, String> =
                serde_json::from_str(&scripts).unwrap_or_default();
            if !parsed.is_empty() {
                has_probes = true;
                break;
            }
        }
    }

    let mut deadline_lock = state.node_shutdown_deadline.lock().await;

    if !has_probes {
        let had_deadline = deadline_lock.is_some();
        *deadline_lock = None;
        if had_deadline {
            drop(deadline_lock);
            drop(online_companions);
            broadcast_node_status(state).await;
        }
        return;
    }

    let minutes_record: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_config WHERE key = 'auto_shutdown_minutes'")
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

    let minutes = minutes_record
        .and_then(|(v,)| v.parse::<u32>().ok())
        .unwrap_or(0);

    if minutes == 0 {
        let had_deadline = deadline_lock.is_some();
        *deadline_lock = None;
        if had_deadline {
            drop(deadline_lock);
            drop(online_companions);
            broadcast_node_status(state).await;
        }
        return;
    }

    let automatic_deadline =
        std::time::Instant::now() + std::time::Duration::from_secs((minutes * 60) as u64);
    let current = *deadline_lock;
    let new_deadline =
        reset_deadline_without_shortening(current, automatic_deadline, force_reset);
    if current != Some(new_deadline) {
        *deadline_lock = Some(new_deadline);
        drop(deadline_lock);
        drop(online_companions);
        broadcast_node_status(state).await;
    }
}

#[cfg(test)]
mod tests {
    use super::reset_deadline_without_shortening;

    #[test]
    fn reset_does_not_shorten_a_longer_existing_deadline() {
        let now = std::time::Instant::now();
        let existing = now + std::time::Duration::from_secs(40 * 60);
        let reset = now + std::time::Duration::from_secs(10 * 60);

        assert_eq!(
            reset_deadline_without_shortening(Some(existing), reset, true),
            existing
        );
    }

    #[test]
    fn reset_extends_a_shorter_existing_deadline() {
        let now = std::time::Instant::now();
        let existing = now + std::time::Duration::from_secs(10 * 60);
        let reset = now + std::time::Duration::from_secs(30 * 60);

        assert_eq!(
            reset_deadline_without_shortening(Some(existing), reset, true),
            reset
        );
    }
}

async fn handle_quic_connection(connection: quinn::Connection, state: AppState) {
    match connection.accept_bi().await {
        Ok((send_stream, recv_stream)) => {
            let (tx, mut rx) = mpsc::channel::<ServerToCompanion>(32);
            let mut client_id_opt: Option<String> = None;

            let mut framed_write = FramedWrite::new(send_stream, LengthDelimitedCodec::new());
            let mut framed_read = FramedRead::new(recv_stream, LengthDelimitedCodec::new());

            let mut send_task = tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if framed_write.send(json.into()).await.is_err() {
                            break;
                        }
                    }
                }
            });

            let state_clone = state.clone();
            let tx_for_recv = tx.clone();
            let mut recv_task = tokio::spawn(async move {
                while let Some(Ok(bytes)) = framed_read.next().await {
                    if let Ok(line) = String::from_utf8(bytes.to_vec()) {
                        if let Ok(parsed) = serde_json::from_str::<CompanionToServer>(&line) {
                            match parsed {
                                CompanionToServer::Hello { client_id } => {
                                    let record: Option<(String,)> = sqlx::query_as(
                                        "SELECT scripts FROM companions WHERE id = ?",
                                    )
                                    .bind(&client_id)
                                    .fetch_optional(&state_clone.db)
                                    .await
                                    .unwrap_or(None);

                                    match record {
                                        Some((scripts_json,)) => {
                                            info!("QUIC Client authenticated: {}", client_id);
                                            client_id_opt = Some(client_id.clone());
                                            state_clone
                                                .companions
                                                .lock()
                                                .await
                                                .insert(client_id.clone(), tx.clone());

                                            // Flush pending commands
                                            {
                                                let mut pc =
                                                    state_clone.pending_commands.lock().await;
                                                let now = std::time::Instant::now();
                                                pc.retain(|(t, _)| {
                                                    now.duration_since(*t).as_secs() < 60
                                                });

                                                let mut to_remove = Vec::new();
                                                for (i, (_, cmd_msg)) in pc.iter().enumerate() {
                                                    let mut should_send = false;
                                                    if let ServerToCompanion::Command {
                                                        target_id,
                                                        ..
                                                    } = cmd_msg
                                                    {
                                                        if target_id.is_none()
                                                            || target_id.as_ref()
                                                                == Some(&client_id)
                                                        {
                                                            should_send = true;
                                                        }
                                                    }
                                                    if should_send {
                                                        if let ServerToCompanion::Command {
                                                            cmd,
                                                            ..
                                                        } = cmd_msg
                                                        {
                                                            info!(
                                                                "Flushing pending command {} to newly connected companion {}",
                                                                cmd, client_id
                                                            );
                                                            let _ = tx.try_send(
                                                                ServerToCompanion::Command {
                                                                    target_id: None,
                                                                    cmd: cmd.clone(),
                                                                },
                                                            );
                                                            to_remove.push(i);
                                                        }
                                                    }
                                                }
                                                for i in to_remove.into_iter().rev() {
                                                    pc.remove(i);
                                                }
                                            }

                                            let scripts: HashMap<String, String> =
                                                serde_json::from_str(&scripts_json)
                                                    .unwrap_or_default();
                                            info!(
                                                "Sending ConfigUpdate to {} with {} scripts",
                                                client_id,
                                                scripts.len()
                                            );
                                            let _ = tx.try_send(ServerToCompanion::ConfigUpdate {
                                                scripts,
                                            });

                                            broadcast_node_status(&state_clone).await;
                                            check_and_update_global_deadline(&state_clone, false)
                                                .await;
                                        }
                                        None => {
                                            warn!(
                                                "QUIC Client rejected: UUID not found in companions table: {}",
                                                client_id
                                            );
                                            break;
                                        }
                                    }
                                }
                                CompanionToServer::Status {
                                    client_id,
                                    active,
                                    active_service,
                                    probe_type,
                                } => {
                                    if let Some(ref cid) = client_id_opt {
                                        if &client_id == cid {
                                            info!(
                                                "QUIC Status from {}: probe={}, active={}, service={:?}",
                                                cid, probe_type, active, active_service
                                            );
                                            let mut procs =
                                                state_clone.active_services.lock().await;
                                            let client_procs = procs
                                                .entry(cid.clone())
                                                .or_insert_with(std::collections::HashMap::new);
                                            if active {
                                                if probe_type != "heartbeat" {
                                                    if let Some(p) = &active_service {
                                                        client_procs
                                                            .insert(probe_type.clone(), p.clone());
                                                    } else {
                                                        client_procs.insert(
                                                            probe_type.clone(),
                                                            "Unknown".to_string(),
                                                        );
                                                    }
                                                    let _ = client_procs;
                                                    drop(procs);
                                                    check_and_update_global_deadline(
                                                        &state_clone,
                                                        true,
                                                    )
                                                    .await;
                                                }
                                            } else {
                                                client_procs.remove(&probe_type);
                                            }

                                            let mut dashboards =
                                                state_clone.dashboards.lock().await;
                                            for (_, dash_tx) in dashboards.iter_mut() {
                                                let _ =
                                                    dash_tx.try_send(ServerToDashboard::Status {
                                                        client_id: cid.clone(),
                                                        active,
                                                        active_service: active_service.clone(),
                                                        probe_type: probe_type.clone(),
                                                    });
                                            }
                                        }
                                    }
                                }
                                CompanionToServer::Bye { client_id } => {
                                    if let Some(ref cid) = client_id_opt {
                                        if &client_id == cid {
                                            info!("Received Bye from {}, disconnecting...", cid);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(cid) = client_id_opt {
                    info!("QUIC Client disconnected: {}", cid);
                    let mut lock = state_clone.companions.lock().await;
                    let should_remove = if let Some(current_tx) = lock.get(&cid) {
                        current_tx.same_channel(&tx_for_recv)
                    } else {
                        false
                    };

                    if should_remove {
                        lock.remove(&cid);
                        state_clone.active_services.lock().await.remove(&cid);
                        if lock.is_empty() {
                            *state_clone.last_offline_time.lock().await =
                                Some(std::time::Instant::now());
                        }
                    }
                    drop(lock);

                    if should_remove {
                        broadcast_node_status(&state_clone).await;
                        check_and_update_global_deadline(&state_clone, false).await;
                    }
                }
            });

            tokio::select! {
                _ = &mut send_task => recv_task.abort(),
                _ = &mut recv_task => send_task.abort(),
            }
        }
        Err(e) => {
            warn!("Failed to accept bi-directional QUIC stream: {}", e);
        }
    }
}
