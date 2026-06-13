mod agents;
mod config;
mod engine;
mod github;
mod mcp;
mod profile;
mod synapsis;
mod types;

use config::Config;
use engine::LoopEngine;
use mcp::McpServer;

fn check_updates() {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    if !project.join(".git").exists() {
        return;
    }

    // Detect default branch
    let head_ref = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(project)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().trim_start_matches("refs/remotes/origin/").to_string())
        .unwrap_or_else(|| "main".into());

    // Fetch in background so it doesn't block startup
    let project_owned = project.to_path_buf();
    std::thread::spawn(move || {
        let _ = std::process::Command::new("git")
            .args(["fetch", "--quiet"])
            .current_dir(&project_owned)
            .output();
    });

    let behind = std::process::Command::new("git")
        .args(["rev-list", "--count", &format!("HEAD..origin/{}", head_ref)])
        .current_dir(project)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<i32>().ok())
        .unwrap_or(0);

    if behind > 0 {
        eprintln!("[Oura] Update available: {} commits behind. Run oura_update to upgrade.", behind);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Signal handling for graceful shutdown - spawn a watcher thread
    let (sig_tx, sig_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        // Simple polling approach: try to detect stdin close or signal
        // In production, use tokio::signal::ctrl_c()
        let _ = sig_rx.recv();
        std::process::exit(0);
    });

    Config::init().ok();
    let config = Config::load();

    let quiet = std::env::var("OURA_QUIET").is_ok() || std::env::var("QUIET").is_ok();
    if !quiet {
        eprintln!("[Oura] v{} ready (stdio)", env!("CARGO_PKG_VERSION"));
    }
    check_updates();

    let engine = LoopEngine::new(config.loop_engine.max_iterations, config.loop_engine.convergence_threshold);
    let mut server = McpServer::new(engine);
    server.run()?;

    eprintln!("[Oura] Server shut down gracefully");
    Ok(())
}
