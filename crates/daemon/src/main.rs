use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = env::var("ENDGOAL_BACKEND_PORT").unwrap_or_else(|_| "3001".to_string());
    let ws_url = format!("ws://localhost:{port}/ws/daemon");
    let token = env::var("ENDGOAL_DAEMON_TOKEN").unwrap_or_else(|_| "dev-token".to_string());

    let scratchpad_root = env::var("ENDGOAL_SCRATCHPAD_ROOT")
        .ok()
        .map(std::path::PathBuf::from);

    println!("Connecting to {ws_url}...");
    endgoal_daemon::ws_client::run_daemon_client(&ws_url, &token, scratchpad_root).await?;

    Ok(())
}
