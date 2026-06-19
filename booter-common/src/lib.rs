use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Messages sent from the Companion to the Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum CompanionToServer {
    Hello {
        client_id: String,
    },
    Status {
        client_id: String,
        active: bool,
        active_service: Option<String>,
        probe_type: String,
    },
    Bye {
        client_id: String,
    },
}

/// Messages sent from the Server to the Companion
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum ServerToCompanion {
    Command {
        target_id: Option<String>,
        cmd: String,
    },
    ConfigUpdate {
        scripts: HashMap<String, String>,
    },
}

/// Messages sent from the Dashboard to the Server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum DashboardToServer {
    Auth {
        token: String,
    },
    Command {
        target_id: Option<String>,
        cmd: String,
    },
}

/// Messages sent from the Server to the Dashboard
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum ServerToDashboard {
    NodeStatus {
        online_count: usize,
        shutdown_deadline: Option<i64>,
        forbidden_time: Option<String>,
        cooldown_deadline: Option<i64>,
        absolute_cooldown_deadline: Option<i64>,
    },
    CommandResult {
        success: bool,
        message: String,
    },
    Status {
        client_id: String,
        active: bool,
        active_service: Option<String>,
        probe_type: String,
    },
}
