mod types;
mod discover;
mod monitor;

use axum::{routing::get, Json, Router};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::watch;
use types::SystemStats;
use local_ip_address::local_ip;

#[tokio::main]
async fn main() {
    let (paths, static_info) = discover::discover_device_layout();
    let static_info = Arc::new(static_info);
    
    let (tx, rx) = watch::channel(SystemStats::default());

    let static_clone = Arc::clone(&static_info);
    tokio::spawn(async move {
        monitor::run_monitor(tx, paths, static_clone).await;
    });

    let app = Router::new()
        .route("/stats", get(|axum::extract::State(rx): axum::extract::State<watch::Receiver<SystemStats>>| async move {
            Json(rx.borrow().clone())
        }))
        .with_state(rx);

    let addr = "0.0.0.0:3000";
    let listener = TcpListener::bind(addr).await.expect("Failed to bind port 3000");

    let host = local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "localhost".to_string());

    println!("\n🚀 Pasmonux API: http://{}:3000/stats", host);

    axum::serve(listener, app).await.unwrap();
}