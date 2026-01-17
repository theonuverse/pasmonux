mod types;
mod discover;
mod monitor;

use axum::{routing::get, Json, Router};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::watch;
use types::SystemStats;

#[tokio::main]
async fn main() {
    let (paths, static_info) = discover::discover_device_layout();
    let static_info = Arc::new(static_info);
    let (tx, rx) = watch::channel(SystemStats::default());

    let static_clone = Arc::clone(&static_info);
    tokio::spawn(async move {
        monitor::run_super_fast_monitor(tx, paths, static_clone).await;
    });

    let app = Router::new().route("/stats", get(move || {
        let data = rx.borrow().clone();
        async move { Json(data) }
    }));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("\n🚀 Pasmonux API: http://localhost:3000/stats");
    axum::serve(listener, app).await.unwrap();
}
