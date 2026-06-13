mod agents;
mod config;
mod engine;
mod github;
mod mcp;
mod synapsis;
mod types;

use config::Config;
use engine::LoopEngine;
use mcp::McpServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Config::init().ok();
    let config = Config::load();

    eprintln!("[Oura] MCP server starting...");
    eprintln!("[Oura] Config: max_iterations={}, convergence_threshold={}",
        config.loop_engine.max_iterations, config.loop_engine.convergence_threshold);
    eprintln!("[Oura] GitHub integration: {}", if config.github.enabled { "enabled" } else { "disabled" });
    eprintln!("[Oura] Synapsis integration: {}", if config.synapsis.enabled { "enabled" } else { "disabled" });

    let _github = match github::GitHubClient::new(&config) {
        Some(client) => {
            let owner = client.repos().first().map(|r| r.owner.clone()).unwrap_or_else(|| "?".into());
            let repo = client.repos().first().map(|r| r.repo.clone()).unwrap_or_else(|| "?".into());
            eprintln!("[Oura] GitHub client initialized for: {}/{}", owner, repo);
            Some(client)
        }
        None => {
            eprintln!("[Oura] GitHub client: not configured (set OURA_GITHUB_TOKEN or config)");
            None
        }
    };

    let engine = LoopEngine::new(config.loop_engine.max_iterations, config.loop_engine.convergence_threshold);
    let mut server = McpServer::new(engine);
    server.run()?;

    Ok(())
}
