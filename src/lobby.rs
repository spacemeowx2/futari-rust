use crate::logging::log_utils::{info, warn};
use crate::types::{current_time_millis, RecruitRecord};

use axum::{
    extract::State,
    http::{Request, StatusCode},
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::net::TcpListener;

// Time-to-live for recruitment records (in milliseconds)
const MAX_TTL: i64 = 30_000;

/// Type alias for the recruitment storage
type RecruitStore = Arc<DashMap<u32, RecruitRecord>>;

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
            .route("/info", get(Self::relay_info))
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

        // Get IP address from record
        let ip = record.recruit_info.mecha_info.ip_address;

        // Log if this is a new recruitment
        let exists = recruits.contains_key(&ip);
        if !exists {
            info("Lobby", &format!("New recruitment from IP: {}", ip));
        }

        // Store the recruitment
        recruits.insert(ip, record);

        StatusCode::OK
    }

    /// Handler for finishing a recruitment
    async fn finish_recruit(
        State(recruits): State<RecruitStore>,
        Json(record): Json<RecruitRecord>,
    ) -> StatusCode {
        let ip = record.recruit_info.mecha_info.ip_address;

        if !recruits.contains_key(&ip) {
            warn("Lobby", &format!("Recruitment not found for IP: {}", ip));
            return StatusCode::NOT_FOUND;
        }

        // Check if keychip matches
        if let Some(existing) = recruits.get(&ip) {
            if existing.keychip != record.keychip {
                warn("Lobby", &format!("Keychip mismatch for IP: {}", ip));
                return StatusCode::BAD_REQUEST;
            }
        }

        info("Lobby", &format!("Completing recruitment for IP: {}", ip));
        recruits.remove(&ip);
        StatusCode::OK
    }

    /// Handler for listing all recruitments
    async fn list_recruits(State(recruits): State<RecruitStore>) -> String {
        info("Lobby", "Listing all active recruitments");

        // Remove expired recruitments first
        Self::do_cleanup(&recruits);

        // Collect all active recruitments
        let mut result = String::new();
        for record in recruits.iter() {
            // Create a filtered version of the record without keychip and time
            let filtered_record = json!({
                "RecruitInfo": record.value().recruit_info,
                "Server": record.value().server
            });

            result.push_str(&filtered_record.to_string());
            result.push('\n');
        }

        result
    }

    /// Handler for cleaning expired recruitments
    #[allow(dead_code)]
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
    async fn debug_info<B>(req: Request<B>) -> Json<Value> {
        info("Lobby", "Debug info requested");

        // Build debug response similar to the Kotlin implementation
        Json(json!({
            "serverHost": req.headers().get("host")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("unknown"),
            "remoteHost": req.headers().get("x-forwarded-for")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("unknown"),
            "localHost": "127.0.0.1",
            "serverPort": 20100,
            "remotePort": 0,
            "localPort": 0,
            "uri": req.uri().to_string(),
            "method": req.method().to_string(),
            "headers": req.headers().iter()
                .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or("invalid")))
                .collect::<Vec<String>>()
                .join("\n")
        }))
    }

    /// Handler for relay server information
    async fn relay_info<B>(req: Request<B>) -> Json<Value> {
        let host_override = std::env::var("HOST_OVERRIDE").ok();

        // Get server host from the request if host override is not set
        let server_host = match host_override {
            Some(host) => host,
            None => {
                let headers = req.headers();
                let host_header = headers
                    .get("host")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.split(':').next().unwrap_or("127.0.0.1").to_string())
                    .unwrap_or_else(|| "127.0.0.1".to_string());
                host_header
            }
        };

        Json(json!({
            "relayHost": server_host,
            "relayPort": 20101
        }))
    }

    /// Get a clone of the recruit store for sharing with other components
    #[allow(dead_code)]
    pub fn get_recruit_store(&self) -> RecruitStore {
        self.recruits.clone()
    }
}
