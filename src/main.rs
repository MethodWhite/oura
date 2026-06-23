mod agents;
mod config;
mod connector;
mod engine;
mod error;
mod events;
mod feedback;
mod fs_utils;
mod mcp;
mod profile;
mod traits;
mod types;

use config::Config;
use engine::LoopEngine;
use events::EventLogger;
use mcp::McpServer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn spawn_update_checker() {
    let project = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if !project.join(".git").exists() {
        return;
    }
    tokio::spawn(async move {
        let head_ref = match tokio::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&project)
            .output().await
        {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(_) => return,
        };
        if head_ref.is_empty() || head_ref == "HEAD" { return; }

        let _ = tokio::process::Command::new("git")
            .args(["fetch", "--quiet"])
            .current_dir(&project)
            .output().await;

        let remote_ref = format!("HEAD..origin/{}", head_ref);
        let behind = match tokio::process::Command::new("git")
            .args(["rev-list", "--count", &remote_ref])
            .current_dir(&project)
            .output().await
        {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().parse::<i32>().unwrap_or(0),
            Err(_) => 0,
        };

        if behind > 0 {
            tracing::warn!(commits_behind = behind, branch = head_ref, "Update available. Run oura_update to upgrade.");
        }
    });
}

fn handle_cli_args() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        return Ok(());
    }
    match args[1].as_str() {
        "--version" | "-v" => {
            eprintln!("Oura v{}", env!("CARGO_PKG_VERSION"));
            if let Ok(head) = std::env::var("GIT_HEAD") {
                eprintln!("commit: {}", head);
            }
            if let Ok(ts) = std::env::var("BUILD_TIME") {
                eprintln!("build: {}", ts);
            }
            std::process::exit(0);
        }
        "--init" => {
            Config::init()?;
            std::process::exit(0);
        }
        "--help" | "-h" => {
            eprintln!("Oura v{}", env!("CARGO_PKG_VERSION"));
            eprintln!("Usage: oura [--version | --init | --help]");
            eprintln!();
            eprintln!("Without arguments, runs the MCP server on stdio.");
            eprintln!("  --version, -v  Print version and exit");
            eprintln!("  --init         Generate default config and exit");
            eprintln!("  --help, -h     Show this help and exit");
            std::process::exit(0);
        }
        _ => {
            eprintln!("Unknown argument: {}", args[1]);
            eprintln!("Usage: oura [--version | --init | --help]");
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    handle_cli_args()?;

    Config::init().ok();
    let config = Config::load();

    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| {
        format!("oura={}", config.logging.level)
    });
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_new(&filter)
                .unwrap_or_else(|_| "oura=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let quiet = std::env::var("OURA_QUIET").is_ok();
    if !quiet {
        tracing::info!(version = env!("CARGO_PKG_VERSION"), "Oura ready (stdio)");
    }
    spawn_update_checker();

    let engine = LoopEngine::new(&config.loop_engine, &config.connector);

    let event_logger = EventLogger::new(engine.event_bus());
    let _logger_handle = tokio::spawn(async move {
        event_logger.run().await;
    });

    let mut server = McpServer::new(engine);
    server.run().await?;

    // engine.shutdown().await;  // McpServer owns engine, cleanup on drop
    tracing::info!("Oura server shut down gracefully");
    Ok(())
}
