//! Basic Braid HTTP server example
//!
//! Demonstrates a simple server that sends versioned updates.
//!
//! Run with: cargo run --example server_basic

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, Router},
};
use braid_axum_http::{BraidState, Update, Version};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    current_version: Arc<RwLock<String>>,
}

#[tokio::main]
async fn main() {
    println!("Braid Server Basic Example");
    println!("==========================\n");
    println!("Starting server on http://localhost:3000");

    let state = AppState {
        current_version: Arc::new(RwLock::new("v1".to_string())),
    };

    let app = Router::new()
        .route("/data", get(handle_get_data))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}

async fn handle_get_data(
    State(state): State<AppState>,
    braid_state: BraidState,
) -> impl IntoResponse {
    println!("\nReceived request:");
    println!("  Subscribe: {}", braid_state.subscribe);
    println!("  Version: {:?}", braid_state.version);
    println!("  Peer: {:?}", braid_state.peer);

    let version = braid_state
        .version
        .unwrap_or_else(|| vec![Version::from("v0")]);

    let update = Update::snapshot(
        Version::from("v1"),
        r#"{"message": "Hello from Braid server", "timestamp": "2024-01-01T00:00:00Z"}"#,
    )
    .with_parents(version);

    println!("Sending update:");
    println!("  Version: {:?}", update.version);
    println!("  Body: {:?}", String::from_utf8_lossy(&update.body.as_ref().unwrap()));

    update
}
