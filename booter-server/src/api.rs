use axum::{
    extract::State,
    routing::{get, post, delete},
    Json, Router,
    http::{HeaderMap, Uri, header, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Secret, TOTP};
use crate::AppState;
use rand::{RngCore, Rng};
use base64::{Engine as _, engine::general_purpose::STANDARD as b64};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/auth/email/request", post(request_email_code))
        .route("/api/v1/auth/email/verify", post(verify_email_code))
        .route("/api/v1/auth/admin/totp", post(totp_verify))
        .route("/api/v1/admin/autoshutdown", get(get_autoshutdown).post(set_autoshutdown))
        .route("/api/v1/admin/mijia/status", get(mijia_status))
        .route("/api/v1/admin/mijia/qr/start", get(mijia_qr_start))
        .route("/api/v1/admin/mijia/qr/poll", post(mijia_qr_poll))
        .route("/api/v1/admin/mijia/devices", get(mijia_devices))
        .route("/api/v1/admin/mijia/devices/select", post(mijia_select_device))
        .route("/api/v1/admin/boot_restrictions", get(get_boot_restrictions).post(set_boot_restrictions))
        .route("/api/v1/admin/users", get(get_users).post(add_user))
        .route("/api/v1/admin/users/{email}", delete(delete_user))

        .route("/api/v1/system/start", post(request_startup))
        .route("/api/v1/system/turnstile", get(get_turnstile_config))
        .route("/api/v1/admin/delay", post(admin_delay_shutdown))
        .nest("/api/v1/admin/companions", crate::api_companions::router(state.clone()))
        .fallback(static_handler)
        .with_state(state)
}


async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        path = "index.html";
    }

    match crate::ASSETS_DIR.get_file(&path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], file.contents()).into_response()
        }
        None => {
            if let Some(index) = crate::ASSETS_DIR.get_file("index.html") {
                ([(header::CONTENT_TYPE, "text/html")], index.contents()).into_response()
            } else {
                (StatusCode::NOT_FOUND, "404 Not Found").into_response()
            }
        }
    }
}

pub async fn verify_admin_token(token: &str, db: &sqlx::SqlitePool) -> bool {
    let record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'admin_session'")
        .fetch_optional(db)
        .await
        .unwrap_or(None);
        
    if let Some((stored_token,)) = record {
        if stored_token == token {
            return true;
        }
    }
    false
}

pub async fn verify_admin(headers: &HeaderMap, db: &sqlx::SqlitePool) -> bool {
    if let Some(auth_header) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                let token = &auth_str[7..];
                return verify_admin_token(token, db).await;
            }
        }
    }
    false
}

async fn verify_turnstile(secret: &str, token: &str) -> bool {
    let client = reqwest::Client::new();
    let res = client.post("https://challenges.cloudflare.com/turnstile/v0/siteverify")
        .form(&[("secret", secret), ("response", token)])
        .send()
        .await;

    if let Ok(res) = res {
        if let Ok(json) = res.json::<serde_json::Value>().await {
            return json.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        }
    }
    false
}

#[derive(Serialize)]
pub struct TurnstileConfigResponse {
    pub site_key: Option<String>,
}

#[axum::debug_handler]
async fn get_turnstile_config(State(state): State<AppState>) -> Json<TurnstileConfigResponse> {
    Json(TurnstileConfigResponse {
        site_key: state.config.turnstile.clone().map(|t| t.site_key),
    })
}

#[derive(Deserialize)]
pub struct TotpVerifyRequest {
    pub code: String,
    pub turnstile_token: Option<String>,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub success: bool,
    pub token: Option<String>,
    pub role: Option<String>,
    pub message: Option<String>,
}

#[axum::debug_handler]
async fn totp_verify(
    State(state): State<AppState>,
    Json(payload): Json<TotpVerifyRequest>,
) -> Json<AuthResponse> {
    if let Some(ref t_cfg) = state.config.turnstile {
        let Some(ref token) = payload.turnstile_token else {
            return Json(AuthResponse { success: false, token: None, role: None, message: Some("Missing Turnstile token".into()) });
        };
        if !verify_turnstile(&t_cfg.secret_key, token).await {
            return Json(AuthResponse { success: false, token: None, role: None, message: Some("Turnstile verification failed".into()) });
        }
    }

    #[derive(sqlx::FromRow)]
    struct AdminRecord {
        totp_secret: Option<String>,
    }

    let record: Option<AdminRecord> = sqlx::query_as("SELECT totp_secret FROM admins LIMIT 1")
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let Some(admin_record) = record else {
        return Json(AuthResponse {
            success: false,
            token: None,
            role: None,
            message: Some("Admin not initialized. Run booter-server setup.".into()),
        });
    };

    let Some(secret_str) = admin_record.totp_secret else {
        return Json(AuthResponse {
            success: false,
            token: None,
            role: None,
            message: Some("TOTP not set up.".into()),
        });
    };

    // Decode secret
    let secret = Secret::Encoded(secret_str).to_bytes().unwrap();
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret,
        Some("Booter".into()),
        "admin".into(),
    ).unwrap();

    if totp.check_current(&payload.code).unwrap_or(false) {
        // Generate a random token
        let token_str = {
            let mut rng = rand::thread_rng();
            let mut token_bytes = [0u8; 32];
            rng.fill_bytes(&mut token_bytes);
            b64.encode(token_bytes)
        };

        // Store token in system_config
        let _ = sqlx::query(
            "INSERT INTO system_config (key, value) VALUES ('admin_session', ?) 
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP"
        )
        .bind(token_str.clone())
        .execute(&state.db)
        .await;

        Json(AuthResponse {
            success: true,
            token: Some(token_str),
            role: Some("admin".into()),
            message: None,
        })
    } else {
        Json(AuthResponse {
            success: false,
            token: None,
            role: None,
            message: Some("Invalid TOTP code".into()),
        })
    }
}

#[derive(Deserialize)]
pub struct EmailRequest {
    pub email: String,
    pub turnstile_token: Option<String>,
}

#[derive(Deserialize)]
pub struct EmailVerifyRequest {
    pub email: String,
    pub code: String,
}

#[axum::debug_handler]
async fn request_email_code(
    State(state): State<AppState>,
    Json(payload): Json<EmailRequest>,
) -> Json<AuthResponse> {
    if let Some(ref t_cfg) = state.config.turnstile {
        let Some(ref token) = payload.turnstile_token else {
            return Json(AuthResponse { success: false, token: None, role: None, message: Some("Missing Turnstile token".into()) });
        };
        if !verify_turnstile(&t_cfg.secret_key, token).await {
            return Json(AuthResponse { success: false, token: None, role: None, message: Some("Turnstile verification failed".into()) });
        }
    }

    // Validate user is in whitelist
    let user_exists: Option<(String,)> = sqlx::query_as("SELECT email FROM users WHERE email = ?")
        .bind(&payload.email)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    if user_exists.is_none() {
        return Json(AuthResponse {
            success: false,
            token: None,
            role: None,
            message: Some("User not authorized. Contact administrator.".into()),
        });
    }

    // Generate a 6-digit OTP
    let otp_str = {
        let mut rng = rand::thread_rng();
        let otp: u32 = rng.gen_range(100_000..999_999);
        otp.to_string()
    };

    let expires_at = std::time::Instant::now() + std::time::Duration::from_secs(300);
    state.otps.lock().await.insert(payload.email.clone(), (otp_str.clone(), expires_at));

    // Send email (we do it in the background or block)
    // To block:
    if let Err(e) = crate::email::send_otp_email(&state.config.smtp, &payload.email, &otp_str).await {
        return Json(AuthResponse {
            success: false,
            token: None,
            role: None,
            message: Some(format!("Failed to send email: {}", e)),
        });
    }

    Json(AuthResponse {
        success: true,
        token: None,
        role: None,
        message: Some("OTP sent".into()),
    })
}

#[axum::debug_handler]
async fn verify_email_code(
    State(state): State<AppState>,
    Json(payload): Json<EmailVerifyRequest>,
) -> Json<AuthResponse> {
    let mut otps = state.otps.lock().await;
    let valid = if let Some((stored_code, expires_at)) = otps.get(&payload.email) {
        if std::time::Instant::now() > *expires_at {
            false
        } else {
            stored_code == &payload.code
        }
    } else {
        false
    };

    if !valid {
        return Json(AuthResponse {
            success: false,
            token: None,
            role: None,
            message: Some("OTP expired or invalid".into()),
        });
    }

    otps.remove(&payload.email);

    // Generate user token
    let token_str = {
        let mut rng = rand::thread_rng();
        let mut token_bytes = [0u8; 32];
        rng.fill_bytes(&mut token_bytes);
        b64.encode(token_bytes)
    };

    // Store token
    let session_key = format!("user_session_{}", payload.email);
    let _ = sqlx::query(
        "INSERT INTO system_config (key, value) VALUES (?, ?) 
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP"
    )
    .bind(session_key)
    .bind(token_str.clone())
    .execute(&state.db)
    .await;

    // Clear the OTP
    let _ = sqlx::query("DELETE FROM otps WHERE email = ?")
        .bind(&payload.email)
        .execute(&state.db)
        .await;

    Json(AuthResponse {
        success: true,
        token: Some(token_str),
        role: Some("user".into()),
        message: None,
    })
}


#[derive(Serialize)]
pub struct AutoShutdownResponse {
    pub success: bool,
    pub minutes: u32,
}

#[derive(Deserialize)]
pub struct AutoShutdownRequest {
    pub minutes: u32,
}

#[axum::debug_handler]
async fn get_autoshutdown(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Json<AutoShutdownResponse> {
    if !verify_admin(&headers, &state.db).await {
        return Json(AutoShutdownResponse { success: false, minutes: 0 });
    }
    
    let record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'auto_shutdown_minutes'")
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        
    let minutes = record.and_then(|(v,)| v.parse::<u32>().ok()).unwrap_or(0);
    Json(AutoShutdownResponse { success: true, minutes })
}

#[axum::debug_handler]
async fn set_autoshutdown(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<AutoShutdownRequest>,
) -> Json<StartupResponse> {
    if !verify_admin(&headers, &state.db).await {
        return Json(StartupResponse { success: false, message: "Unauthorized".into() });
    }
    
    if let Err(e) = sqlx::query(
        "INSERT INTO system_config (key, value) VALUES ('auto_shutdown_minutes', ?) 
         ON CONFLICT(key) DO UPDATE SET value = excluded.value"
    )
    .bind(payload.minutes.to_string())
    .execute(&state.db)
    .await {
        return Json(StartupResponse { success: false, message: format!("DB Error: {}", e) });
    }
    
    // Update the deadline immediately and broadcast to clients
    crate::quic::check_and_update_global_deadline(&state, true).await;
    
    Json(StartupResponse { success: true, message: "Saved".into() })
}

#[derive(Serialize)]
pub struct BootRestrictionsResponse {
    pub success: bool,
    pub cooldown_minutes: u32,
    pub forbidden_time: String,
}

#[derive(Deserialize)]
pub struct BootRestrictionsRequest {
    pub cooldown_minutes: u32,
    pub forbidden_time: String,
}

#[axum::debug_handler]
async fn get_boot_restrictions(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Json<BootRestrictionsResponse> {
    if !verify_admin(&headers, &state.db).await {
        return Json(BootRestrictionsResponse { success: false, cooldown_minutes: 0, forbidden_time: "".into() });
    }
    
    let cooldown_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'boot_cooldown_minutes'")
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    let cooldown_minutes = cooldown_record.and_then(|(v,)| v.parse::<u32>().ok()).unwrap_or(0);
    
    let forbidden_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'boot_forbidden_time'")
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    let forbidden_time = forbidden_record.map(|(v,)| v).unwrap_or_else(|| "".into());
        
    Json(BootRestrictionsResponse { success: true, cooldown_minutes, forbidden_time })
}

#[axum::debug_handler]
async fn set_boot_restrictions(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<BootRestrictionsRequest>,
) -> Json<StartupResponse> {
    if !verify_admin(&headers, &state.db).await {
        return Json(StartupResponse { success: false, message: "Unauthorized".into() });
    }
    
    let _ = sqlx::query("INSERT INTO system_config (key, value) VALUES ('boot_cooldown_minutes', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(payload.cooldown_minutes.to_string())
        .execute(&state.db)
        .await;
        
    let _ = sqlx::query("INSERT INTO system_config (key, value) VALUES ('boot_forbidden_time', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(payload.forbidden_time)
        .execute(&state.db)
        .await;
    
    crate::quic::broadcast_node_status(&state).await;

    Json(StartupResponse { success: true, message: "Saved".into() })
}

#[derive(Serialize)]
pub struct StartupResponse {
    pub success: bool,
    pub message: String,
}

#[axum::debug_handler]
async fn request_startup(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> (axum::http::StatusCode, Json<StartupResponse>) {
    let Some(_) = headers.get("Authorization") else {
        return (axum::http::StatusCode::UNAUTHORIZED, Json(StartupResponse { success: false, message: "Missing Authorization header".into() }));
    };
    
    let is_admin = verify_admin(&headers, &state.db).await;
    if !is_admin {
        // Cooldown check
        let cooldown_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'boot_cooldown_minutes'")
            .fetch_optional(&state.db).await.unwrap_or(None);
        let cooldown_minutes = cooldown_record.and_then(|(v,)| v.parse::<u32>().ok()).unwrap_or(0);
        
        if cooldown_minutes > 0 {
            if let Some(last_time) = *state.last_boot_time.lock().await {
                let elapsed_mins = last_time.elapsed().as_secs_f64() / 60.0;
                if elapsed_mins < cooldown_minutes as f64 {
                    return (axum::http::StatusCode::FORBIDDEN, Json(StartupResponse { success: false, message: "距离上次开机时间过短，请稍后再试".into() }));
                }
            }
        }
        
        // Forbidden time check
        let forbidden_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'boot_forbidden_time'")
            .fetch_optional(&state.db).await.unwrap_or(None);
        if let Some((forbidden_str,)) = forbidden_record {
            if !forbidden_str.trim().is_empty() {
                if let Some((start_str, end_str)) = forbidden_str.split_once('-') {
                    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc()).time();
                    let parse_time = |s: &str| -> Option<time::Time> {
                        let mut parts = s.trim().split(':');
                        let h: u8 = parts.next()?.parse().ok()?;
                        let m: u8 = parts.next()?.parse().ok()?;
                        time::Time::from_hms(h, m, 0).ok()
                    };
                    
                    if let (Some(start), Some(end)) = (
                        parse_time(start_str),
                        parse_time(end_str)
                    ) {
                        let is_forbidden = if start <= end {
                            now >= start && now <= end
                        } else {
                            now >= start || now <= end
                        };
                        
                        if is_forbidden {
                            return (axum::http::StatusCode::FORBIDDEN, Json(StartupResponse { success: false, message: "当前处于禁止开机时段".into() }));
                        }
                    }
                }
            }
        }
    }
    
    let auth_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'mijia_auth'")
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        
    let did_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'mijia_did'")
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let Some((auth_json,)) = auth_record else {
        return (axum::http::StatusCode::BAD_REQUEST, Json(StartupResponse { success: false, message: "Mijia auth not configured in system_config".into() }));
    };
    
    let Some((did,)) = did_record else {
        return (axum::http::StatusCode::BAD_REQUEST, Json(StartupResponse { success: false, message: "Mijia DID not configured in system_config".into() }));
    };

    let auth_data: crate::mijia_client::MijiaAuthData = match serde_json::from_str(&auth_json) {
        Ok(data) => data,
        Err(_) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(StartupResponse { success: false, message: "Invalid Mijia auth format in DB".into() })),
    };

    let mut client = crate::mijia_client::MijiaClient::new(
        "booter_backend_client".into(),
        "booter_backend_pass".into(),
        "Booter/1.0".into()
    );
    client.set_auth_data(auth_data);

    match client.set_devices_prop(&did, 2, 1, serde_json::json!(true)).await {
        Ok(_) => {
            *state.last_boot_time.lock().await = Some(std::time::Instant::now());
            crate::quic::broadcast_node_status(&state).await;
            (axum::http::StatusCode::OK, Json(StartupResponse { success: true, message: "Smart plug turned on successfully".into() }))
        },
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(StartupResponse { success: false, message: format!("Failed to trigger smart plug: {}", e) }))
    }
}

// User Management APIs
#[derive(Serialize)]
pub struct UserListResponse {
    pub success: bool,
    pub users: Vec<String>,
}

#[derive(Deserialize)]
pub struct AddUserRequest {
    pub email: String,
}

#[axum::debug_handler]
async fn get_users(headers: HeaderMap, State(state): State<AppState>) -> Json<UserListResponse> {
    if !verify_admin(&headers, &state.db).await {
        return Json(UserListResponse { success: false, users: vec![] });
    }
    
    let records: Vec<(String,)> = sqlx::query_as("SELECT email FROM users")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        
    Json(UserListResponse {
        success: true,
        users: records.into_iter().map(|(e,)| e).collect(),
    })
}

#[axum::debug_handler]
async fn add_user(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<AddUserRequest>,
) -> Json<StartupResponse> {
    if !verify_admin(&headers, &state.db).await {
        return Json(StartupResponse { success: false, message: "Unauthorized".into() });
    }
    
    match sqlx::query("INSERT INTO users (email) VALUES (?)")
        .bind(&payload.email)
        .execute(&state.db)
        .await 
    {
        Ok(_) => Json(StartupResponse { success: true, message: "User added".into() }),
        Err(_) => Json(StartupResponse { success: false, message: "Failed to add user".into() }),
    }
}

#[axum::debug_handler]
async fn delete_user(
    headers: HeaderMap,
    axum::extract::Path(email): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Json<StartupResponse> {
    if !verify_admin(&headers, &state.db).await {
        return Json(StartupResponse { success: false, message: "Unauthorized".into() });
    }
    
    let _ = sqlx::query("DELETE FROM users WHERE email = ?")
        .bind(&email)
        .execute(&state.db)
        .await;
        
    Json(StartupResponse { success: true, message: "User deleted".into() })
}

// Mijia Management APIs
#[derive(Serialize)]
pub struct MijiaStatusResponse {
    pub success: bool,
    pub is_logged_in: bool,
    pub current_did: Option<String>,
}

#[derive(Serialize)]
pub struct MijiaQrStartResponse {
    pub success: bool,
    pub qr_url: String,
    pub lp_url: String,
}

#[derive(Deserialize)]
pub struct MijiaQrPollRequest {
    pub lp_url: String,
}

#[derive(Serialize)]
pub struct MijiaDevicesResponse {
    pub success: bool,
    pub devices: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct MijiaSelectDeviceRequest {
    pub did: String,
}

#[axum::debug_handler]
async fn mijia_status(headers: HeaderMap, State(state): State<AppState>) -> Json<MijiaStatusResponse> {
    if !verify_admin(&headers, &state.db).await {
         return Json(MijiaStatusResponse { success: false, is_logged_in: false, current_did: None });
    }
    
    let auth_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'mijia_auth'").fetch_optional(&state.db).await.unwrap_or(None);
    let did_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'mijia_did'").fetch_optional(&state.db).await.unwrap_or(None);
    
    Json(MijiaStatusResponse {
        success: true,
        is_logged_in: auth_record.is_some(),
        current_did: did_record.map(|r| r.0),
    })
}

#[axum::debug_handler]
async fn mijia_qr_start(headers: HeaderMap, State(state): State<AppState>) -> Json<MijiaQrStartResponse> {
    if !verify_admin(&headers, &state.db).await {
         return Json(MijiaQrStartResponse { success: false, qr_url: "".into(), lp_url: "".into() });
    }
    let client = crate::mijia_client::MijiaClient::new("booter_backend_client".into(), "booter_backend_pass".into(), "Booter/1.0".into());
    match client.qr_login_step1().await {
        Ok((qr_url, lp_url)) => Json(MijiaQrStartResponse { success: true, qr_url, lp_url }),
        Err(_) => Json(MijiaQrStartResponse { success: false, qr_url: "".into(), lp_url: "".into() }),
    }
}

#[axum::debug_handler]
async fn mijia_qr_poll(headers: HeaderMap, State(state): State<AppState>, Json(payload): Json<MijiaQrPollRequest>) -> Json<StartupResponse> {
    if !verify_admin(&headers, &state.db).await {
         return Json(StartupResponse { success: false, message: "".into() });
    }
    let mut client = crate::mijia_client::MijiaClient::new("booter_backend_client".into(), "booter_backend_pass".into(), "Booter/1.0".into());
    match client.qr_login_step2(&payload.lp_url).await {
        Ok(auth_data) => {
            let auth_str = serde_json::to_string(&auth_data).unwrap_or_default();
            let _ = sqlx::query("INSERT INTO system_config (key, value) VALUES ('mijia_auth', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value").bind(auth_str).execute(&state.db).await;
            Json(StartupResponse { success: true, message: "Logged in".into() })
        }
        Err(e) => Json(StartupResponse { success: false, message: e }),
    }
}

#[axum::debug_handler]
async fn mijia_devices(headers: HeaderMap, State(state): State<AppState>) -> Json<MijiaDevicesResponse> {
    if !verify_admin(&headers, &state.db).await {
         return Json(MijiaDevicesResponse { success: false, devices: vec![] });
    }
    let auth_record: Option<(String,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'mijia_auth'").fetch_optional(&state.db).await.unwrap_or(None);
    if let Some((auth_json,)) = auth_record {
        if let Ok(auth_data) = serde_json::from_str::<crate::mijia_client::MijiaAuthData>(&auth_json) {
            let mut client = crate::mijia_client::MijiaClient::new("booter_backend_client".into(), "booter_backend_pass".into(), "Booter/1.0".into());
            client.set_auth_data(auth_data);
            if let Ok(res) = client.request("/home/device_list", &serde_json::json!({"getVirtualModel": false, "getHuamiDevices": 1})).await {
                if let Some(list) = res.get("list").and_then(|l| l.as_array()) {
                    return Json(MijiaDevicesResponse { success: true, devices: list.clone() });
                }
            }
        }
    }
    Json(MijiaDevicesResponse { success: false, devices: vec![] })
}

#[axum::debug_handler]
async fn mijia_select_device(headers: HeaderMap, State(state): State<AppState>, Json(payload): Json<MijiaSelectDeviceRequest>) -> Json<StartupResponse> {
    if !verify_admin(&headers, &state.db).await {
         return Json(StartupResponse { success: false, message: "".into() });
    }
    let _ = sqlx::query("INSERT INTO system_config (key, value) VALUES ('mijia_did', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value").bind(&payload.did).execute(&state.db).await;
    Json(StartupResponse { success: true, message: "Saved".into() })
}

async fn admin_delay_shutdown() -> &'static str { "Not implemented" }
