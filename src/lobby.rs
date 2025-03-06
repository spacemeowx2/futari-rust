use crate::logging::log_utils::{error, info, warn};
use crate::types::{current_time_millis, RecruitRecord};

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

// Time-to-live for recruitment records (in milliseconds)
const MAX_TTL: i64 = 30_000;

/// Type alias for the recruitment storage
type RecruitStore = Arc<DashMap<String, RecruitRecord>>;

/// The FutariLobby service
pub struct FutariLobby {
    recruits: RecruitStore,
}

impl FutariLobby {
    /// Create a new FutariLobby instance
    pub fn new() -> Self {
        Self {
            recruits: Arc::new(DashMap::new()),
        }
    }

    /// Start the lobby HTTP server
    pub async fn start(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        info("Lobby", &format!("Starting FutariLobby on {}", addr));

        // Create the app state
        let state = self.recruits.clone();

        // Build our application with a route for each endpoint
        let app = Router::new()
            .route("/", get(Self::health_check))
            .route("/recruit/start", post(Self::start_recruit))
            .route("/recruit/finish", post(Self::finish_recruit))
            .route("/recruit/list", get(Self::list_recruits))
            .route("/recruit/clean", get(Self::clean_recruits))
            .route("/debug", get(Self::debug_info))
            .with_state(state);

        // Start the server
        let listener = TcpListener::bind(addr).await?;
        info("Lobby", "HTTP server is listening");

        // Spawn the cleanup task
        self.spawn_cleanup_task();

        axum::serve(listener, app).await?;
        Ok(())
    }

    /// Spawn a task to periodically clean up expired recruitment records
    fn spawn_cleanup_task(&self) {
        let recruits = self.recruits.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                Self::do_cleanup(&recruits);
            }
        });
    }

    /// Handler for the root path health check
    async fn health_check() -> &'static str {
        "Running!"
    }

    /// Handler for starting a new recruitment
    async fn start_recruit(
        State(recruits): State<RecruitStore>,
        Json(mut record): Json<RecruitRecord>,
    ) -> StatusCode {
        // Update the record with current time
        record.time = current_time_millis();

        // Store the recruitment
        info(
            "Lobby",
            &format!("New recruitment from keychip: {}", record.keychip),
        );
        recruits.insert(record.keychip.clone(), record);

        StatusCode::OK
    }

    /// Handler for finishing a recruitment
    async fn finish_recruit(
        State(recruits): State<RecruitStore>,
        Json(record): Json<RecruitRecord>,
    ) -> StatusCode {
        if recruits.contains_key(&record.keychip) {
            info(
                "Lobby",
                &format!("Completing recruitment: {}", record.keychip),
            );
            recruits.remove(&record.keychip);
            StatusCode::OK
        } else {
            warn(
                "Lobby",
                &format!("Recruitment not found: {}", record.keychip),
            );
            StatusCode::NOT_FOUND
        }
    }

    /// Handler for listing all recruitments
    async fn list_recruits(State(recruits): State<RecruitStore>) -> Json<Vec<RecruitRecord>> {
        info("Lobby", "Listing all active recruitments");
        let records: Vec<RecruitRecord> =
            recruits.iter().map(|entry| entry.value().clone()).collect();

        Json(records)
    }

    /// Handler for cleaning expired recruitments
    async fn clean_recruits(State(recruits): State<RecruitStore>) -> StatusCode {
        info("Lobby", "Cleaning expired recruitments");
        Self::do_cleanup(&recruits);
        StatusCode::OK
    }

    /// Helper function to remove expired recruitments
    fn do_cleanup(recruits: &RecruitStore) {
        let now = current_time_millis();
        let mut expired_keys = Vec::new();

        // Find expired records
        for entry in recruits.iter() {
            let record = entry.value();
            if now - record.time > MAX_TTL {
                expired_keys.push(entry.key().clone());
            }
        }

        // Remove expired records
        for key in expired_keys {
            info("Lobby", &format!("Removing expired recruitment: {}", key));
            recruits.remove(&key);
        }
    }

    /// Handler for debug information
    async fn debug_info(State(recruits): State<RecruitStore>) -> Json<Value> {
        info("Lobby", "Debug info requested");

        // Get active recruitment count
        let recruit_count = recruits.len();

        // Get system information
        let memory_usage = match std::process::Command::new("ps")
            .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
        {
            Ok(output) => {
                let rss = String::from_utf8_lossy(&output.stdout).trim().to_string();
                format!("{}KB", rss)
            }
            Err(_) => "Unknown".to_string(),
        };

        // Build debug response
        Json(json!({
            "version": "Futari-Rust v0.1.0",
            "uptime": "N/A", // Would need to track server start time
            "activeConnections": recruit_count,
            "memoryUsage": memory_usage,
            "timestamp": current_time_millis(),
        }))
    }

    /// Get a clone of the recruit store for sharing with other components
    pub fn get_recruit_store(&self) -> RecruitStore {
        self.recruits.clone()
    }
}
