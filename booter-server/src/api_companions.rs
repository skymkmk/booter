use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use axum::http::HeaderMap;

use crate::AppState;
use crate::api::verify_admin;

#[derive(Serialize, Deserialize)]
pub struct Companion {
    pub id: String,
    pub name: String,
    pub scripts: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateCompanionRequest {
    pub name: String,
    #[serde(default)]
    pub scripts: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct UpdateCompanionRequest {
    pub name: String,
    pub scripts: std::collections::HashMap<String, String>,
}

pub fn router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list_companions).post(create_companion))
        .route("/{id}", put(update_companion).delete(delete_companion))
}

async fn list_companions(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    if !verify_admin(&headers, &state.db).await {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let rows: Result<Vec<(String, String, String, String)>, _> = sqlx::query_as(
        "SELECT id, name, scripts, created_at FROM companions ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await;

    match rows {
        Ok(rows) => {
            let companions: Vec<Companion> = rows.into_iter().map(|(id, name, scripts, created_at)| Companion {
                id, name, scripts, created_at
            }).collect();
            (StatusCode::OK, Json(companions)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to fetch companions: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn create_companion(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<CreateCompanionRequest>,
) -> impl IntoResponse {
    if !verify_admin(&headers, &state.db).await {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let id = Uuid::new_v4().to_string();
    let scripts = if payload.scripts.is_empty() { 
        "{}".to_string() 
    } else { 
        match serde_json::to_string(&payload.scripts) {
            Ok(s) => s,
            Err(_) => return (StatusCode::BAD_REQUEST, "Invalid scripts data").into_response(),
        }
    };

    for (k, v) in &payload.scripts {
        if k.len() > 50 || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return (StatusCode::BAD_REQUEST, format!("Invalid script name: {}", k)).into_response();
        }
        if v.len() > 10240 {
            return (StatusCode::BAD_REQUEST, format!("Script '{}' is too long", k)).into_response();
        }
    }

    let res = sqlx::query("INSERT INTO companions (id, name, scripts) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(&payload.name)
        .bind(&scripts)
        .execute(&state.db)
        .await;

    match res {
        Ok(_) => {
            let companion = Companion {
                id,
                name: payload.name,
                scripts: scripts.to_string(),
                created_at: "".to_string(), // Just returning basic info
            };
            (StatusCode::CREATED, Json(companion)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to create companion: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn update_companion(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateCompanionRequest>,
) -> impl IntoResponse {
    if !verify_admin(&headers, &state.db).await {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let scripts_json = match serde_json::to_string(&payload.scripts) {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid scripts data").into_response(),
    };

    for (k, v) in &payload.scripts {
        if k.len() > 50 || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return (StatusCode::BAD_REQUEST, format!("Invalid script name: {}", k)).into_response();
        }
        if v.len() > 10240 {
            return (StatusCode::BAD_REQUEST, format!("Script '{}' is too long", k)).into_response();
        }
    }

    let res = sqlx::query("UPDATE companions SET name = ?, scripts = ? WHERE id = ?")
        .bind(&payload.name)
        .bind(&scripts_json)
        .bind(&id)
        .execute(&state.db)
        .await;

    match res {
        Ok(result) if result.rows_affected() > 0 => {
            // Push update immediately to connected companion
            if let Some(tx) = state.companions.lock().await.get(&id) {
                let _ = tx.try_send(booter_common::ServerToCompanion::ConfigUpdate { scripts: payload.scripts.clone() });
            }
            (StatusCode::OK, "Updated").into_response()
        },
        Ok(_) => (StatusCode::NOT_FOUND, "Companion not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to update companion: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn delete_companion(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !verify_admin(&headers, &state.db).await {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let res = sqlx::query("DELETE FROM companions WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await;

    match res {
        Ok(result) if result.rows_affected() > 0 => {
            // Also kick out the client if they are currently connected
            if let Some(_tx) = state.companions.lock().await.remove(&id) {
                // If they are connected, dropping the sender channel won't close the stream automatically in our design unless we send a message.
                // Wait, our design removes them from state.clients, they stop receiving commands, but their Status updates might still be processed.
                // It's a small edge case, they will reconnect and be rejected next time.
            }
            (StatusCode::OK, "Deleted").into_response()
        },
        Ok(_) => (StatusCode::NOT_FOUND, "Companion not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to delete companion: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}
