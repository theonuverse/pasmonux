mod discover;
mod monitor;
mod router;
mod types;

use std::sync::Arc;

use local_ip_address::local_ip;
use tokio::net::TcpListener;
use tokio::sync::watch;

use types::SystemStats;

#[tokio::main]
async fn main() {
    let (paths, static_info) = discover::discover_device_layout();
    let static_info = Arc::new(static_info);

    let (tx, rx) = watch::channel(SystemStats::default());

    tokio::spawn(monitor::run_monitor(tx, paths, Arc::clone(&static_info)));

    let app = router::build(rx);

    let listener = TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("failed to bind port 3000");

    let host = local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "localhost".into());

    println!("\n\u{1F680} Asmo running on: http://{host}:3000");
    println!("   GET / for all available endpoints\n");

    axum::serve(listener, app).await.expect("server error");
}