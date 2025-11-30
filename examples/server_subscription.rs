//! Subscription server example
//!
//! Demonstrates a server that handles live subscriptions with updates.
//!
//! Run with: cargo run --example server_subscription

use axum::{
    extract::State,
    response::IntoResponse,
    routing::get,
    Router,
};
use braid_axum_http::{BraidState, Update, Version, Patch};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

#[derive(Clone)]
pub struct AppState {
    data: Arc<RwLock<String>>,
}

#[tokio::main]
async fn main() {
    println!("Braid Server Subscription Example");
    println!("=================================\n");
    println!("Starting server on http://localhost:3001");
    println!("Try subscribing with the client:\n");
    println!("  let mut sub = client.subscribe(");
    println!("    \"http://localhost:3001/updates\",");
    println!("    BraidRequest::new().subscribe()");
    println!("  ).await?;\n");

    let state = AppState {
        data: Arc::new(RwLock::new(r#"{"count": 0}"#.to_string())),
    };

    let app = Router::new()
        .route("/updates", get(handle_subscription))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}

async fn handle_subscription(
    State(state): State<AppState>,
    braid_state: BraidState,
) -> impl IntoResponse {
    println!("\nSubscription request received:");
    println!("  Subscribe: {}", braid_state.subscribe);
    println!("  Requested version: {:?}", braid_state.version);

    if !braid_state.subscribe {
        let data = state.data.read().await;
        let update = Update::snapshot(Version::from("v1"), data.as_str());
        return update.into_response();
    }

    let parent_version = braid_state
        .version
        .unwrap_or_else(|| vec![Version::from("v0")]);

    let mut update = Update::snapshot(Version::from("v1"), r#"{"count": 0}"#)
        .with_parents(parent_version);

    update.status = 209;

    println!("Sending subscription response (209):");
    println!("  Version: {:?}", update.version);

    update.into_response()
}
