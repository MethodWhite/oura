mod agents;
mod config;
mod engine;
mod error;
mod events;
mod feedback;
mod mcp;
mod profile;
mod traits;
mod types;

use config::Config;
use engine::LoopEngine;
use events::EventLogger;
use mcp::McpServer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn check_updates() {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    if !project.join(".git").exists() {
        return;
    }

    let head_ref = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(project)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            s.trim()
                .trim_start_matches("refs/remotes/origin/")
                .to_string()
        })
        .unwrap_or_else(|| "main".into());

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
        tracing::warn!(
            commits_behind = behind,
            "Update available. Run oura_update to upgrade."
        );
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "oura=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    Config::init().ok();
    let config = Config::load();

    let quiet = std::env::var("OURA_QUIET").is_ok() || std::env::var("QUIET").is_ok();
    if !quiet {
        tracing::info!(version = env!("CARGO_PKG_VERSION"), "Oura ready (stdio)");
    }
    check_updates();

    let mut engine = LoopEngine::new(
        config.loop_engine.max_iterations,
        config.loop_engine.convergence_threshold,
    );

    let event_logger = EventLogger::new(engine.event_bus());
    tokio::spawn(async move {
        event_logger.run().await;
    });

    let mut server = McpServer::new(engine);
    server.run().await?;

    tracing::info!("Oura server shut down gracefully");
    Ok(())
}
