mod lobby;
mod logging;
mod relay;
mod types;

use anyhow::Result;
use lobby::FutariLobby;
use logging::log_utils::info;
use relay::FutariRelay;

// Default ports and addresses
const DEFAULT_HTTP_ADDR: &str = "0.0.0.0:20100"; // HTTP API server address
const DEFAULT_RELAY_ADDR: &str = "0.0.0.0:20101"; // TCP relay server address

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    logging::init_logging();

    info("Main", "Starting WorldLinkd - Rust Implementation");

    // Create the FutariLobby (HTTP API service)
    let lobby = FutariLobby::new();
    let lobby_handle = tokio::spawn(async move {
        if let Err(e) = lobby.start(DEFAULT_HTTP_ADDR).await {
            eprintln!("FutariLobby error: {}", e);
        }
    });

    // Create the FutariRelay (TCP relay service)
    let (relay, message_rx) = FutariRelay::new();

    // Start the message processing task
    let relay_clone = relay.clone();
    let message_task = tokio::spawn(async move {
        relay_clone.process_messages(message_rx).await;
    });

    // Start the relay server
    let relay_handle = tokio::spawn(async move {
        if let Err(e) = relay.start(DEFAULT_RELAY_ADDR).await {
            eprintln!("FutariRelay error: {}", e);
        }
    });

    // Wait for all services to complete (which should be never)
    tokio::try_join!(
        async {
            lobby_handle.await.unwrap();
            Ok::<_, anyhow::Error>(())
        },
        async {
            relay_handle.await.unwrap();
            Ok::<_, anyhow::Error>(())
        },
        async {
            message_task.await.unwrap();
            Ok::<_, anyhow::Error>(())
        }
    )?;

    Ok(())
}
