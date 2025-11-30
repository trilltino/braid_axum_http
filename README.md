# Braid-HTTP: Synchronization for HTTP

This crate implements Braid-HTTP, a set of extensions that generalize HTTP from a state *transfer* protocol into a full state *synchronization* protocol.

Based on [draft-toomim-httpbis-braid-http-04](https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http)

## Features

- **Versioning**: Track resource history as a directed acyclic graph (DAG)
- **Patch-based Updates**: Send incremental changes instead of full snapshots
- **Real-time Subscriptions**: Live updates via HTTP 209 status code
- **Conflict Resolution**: Built-in CRDT support using Diamond-Types for collaborative text editing
- **Axum Integration**: Server middleware for easy integration with Rust web applications

## Quick Start

```rust,ignore
use braid_axum_http::{BraidLayer, Update, Version};
use axum::{Router, routing::get, middleware};

async fn handler() -> Update {
    Update::snapshot(Version::new("v1"), "Hello, World!")
}

let braid = BraidLayer::new();
let app = Router::new()
    .route("/resource", get(handler))
    .layer(middleware::from_fn(braid.middleware()));
```

## License

MIT

