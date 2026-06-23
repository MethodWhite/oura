pub mod handlers;
pub mod resources;

use crate::engine::LoopEngine;
use crate::types::*;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct McpServer {
    pub(super) engine: LoopEngine,
}

impl McpServer {
    pub fn new(engine: LoopEngine) -> Self {
        Self { engine }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);

        loop {
            let mut line = String::new();
            match tokio::time::timeout(std::time::Duration::from_secs(300), reader.read_line(&mut line)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(_)) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() { continue; }

                    let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                        Ok(req) => req,
                        Err(e) => {
                            tracing::error!(error = %e, message = %trimmed, "Parse error");
                            let err_resp = json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("Parse error: {}", e)}});
                            if let Ok(output) = serde_json::to_string(&err_resp) {
                                let _ = stdout.write_all(output.as_bytes()).await;
                                let _ = stdout.write_all(b"\n").await;
                                let _ = stdout.flush().await;
                            }
                            continue;
                        }
                    };

                    let response = self.handle_request(&request).await;
                    if response.id.is_null() { continue; }

                    if let Ok(output) = serde_json::to_string(&response) {
                        let _ = stdout.write_all(output.as_bytes()).await;
                        let _ = stdout.write_all(b"\n").await;
                        let _ = stdout.flush().await;
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "Failed to read line");
                    break;
                }
                Err(_) => {
                    tracing::trace!("stdin idle timeout");
                    continue;
                }
            }
        }
        Ok(())
    }

    async fn handle_request(&mut self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone();
        match request.method.as_str() {
            "initialize" => self.handle_initialize(id, request.params.as_ref()),
            "initialized" => JsonRpcResponse { jsonrpc: "2.0".into(), id, result: None, error: None },
            "tools/list" => self.handle_tools_list(id),
            "tools/call" => self.handle_tools_call(id, request.params.as_ref()).await,
            "resources/list" => self.handle_resources_list(id),
            "resources/read" => self.handle_resource_read(id, request.params.as_ref()),
            "prompts/list" => self.handle_prompts_list(id),
            "prompts/get" => self.handle_prompt_get(id, request.params.as_ref()),
            "ping" => self.ok(id, json!({})),
            _ => JsonRpcResponse {
                jsonrpc: "2.0".into(), id,
                result: None,
                error: Some(JsonRpcError { code: -32601, message: format!("Method not found: {}", request.method), data: None }),
            },
        }
    }

    fn ok(&self, id: Value, result: Value) -> JsonRpcResponse {
        JsonRpcResponse { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }

    fn err(&self, id: Value, code: i32, message: String) -> JsonRpcResponse {
        JsonRpcResponse { jsonrpc: "2.0".into(), id, result: None, error: Some(JsonRpcError { code, message, data: None }) }
    }

    fn text_content(text: String) -> Value {
        json!([{ "type": "text", "text": text }])
    }

    const SUPPORTED_PROTOCOLS: &'static [&'static str] = &["2024-11-05", "2025-03-26"];

    fn handle_initialize(&self, id: Value, params: Option<&Value>) -> JsonRpcResponse {
        let client_version = params.and_then(|p| p["protocolVersion"].as_str()).unwrap_or("unknown");
        if !Self::SUPPORTED_PROTOCOLS.contains(&client_version) {
            tracing::warn!(client = client_version, "MCP client using unsupported protocol version");
        }
        self.ok(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": { "listChanged": true }, "resources": { "listChanged": true }, "prompts": { "listChanged": true } },
            "serverInfo": { "name": "oura", "version": env!("CARGO_PKG_VERSION") }
        }))
    }

    fn handle_tools_list(&self, id: Value) -> JsonRpcResponse {
        let tools = vec![
            McpToolDefinition {
                name: "oura_start_loop".into(),
                description: "Start a new iteration loop to achieve a specific goal. Use manual=true to drive iterations yourself via oura_iterate.".into(),
                input_schema: json!({"type": "object", "properties": {"goal": { "type": "string", "description": "The goal to achieve" }, "maxIterations": { "type": "integer", "description": "Maximum iterations (default: 20)", "default": 20 }, "manual": { "type": "boolean", "description": "Manual mode: you call oura_iterate for each step (default: false)", "default": false } }, "required": ["goal"]}),
            },
            McpToolDefinition {
                name: "oura_iterate".into(),
                description: "Execute a single iteration step manually (use after oura_start_loop with manual=true)".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            McpToolDefinition {
                name: "oura_loop_status".into(),
                description: "Get current status of the active loop".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            McpToolDefinition {
                name: "oura_loop_stop".into(),
                description: "Stop the currently running loop".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            McpToolDefinition {
                name: "oura_results".into(),
                description: "Get accumulated results from all iterations".into(),
                input_schema: json!({"type": "object", "properties": {"iteration": { "type": "number", "description": "Specific iteration to view (omit or 0 for all)" }}}),
            },
            McpToolDefinition {
                name: "oura_configure".into(),
                description: "Update Oura engine configuration".into(),
                input_schema: json!({"type": "object", "properties": {"maxIterations": { "type": "number", "description": "Maximum iterations" }, "convergenceThreshold": { "type": "number", "description": "Score threshold (0-100)" }, "workingDirectory": { "type": "string", "description": "Working directory" }}}),
            },
            McpToolDefinition {
                name: "oura_plugin_load".into(),
                description: "[EXPERIMENTAL — not yet implemented] Load a plugin from a directory path".into(),
                input_schema: json!({"type": "object", "properties": {"path": { "type": "string", "description": "Path to plugin directory" }}, "required": ["path"]}),
            },
            McpToolDefinition {
                name: "oura_plugin_list".into(),
                description: "[EXPERIMENTAL — not yet implemented] List all loaded plugins".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            McpToolDefinition {
                name: "oura_analyze_security".into(),
                description: "Run security audit on specified files (pass at least one file, or you get an empty report)".into(),
                input_schema: json!({"type": "object", "properties": {"files": { "type": "array", "items": { "type": "string" }, "description": "Files to scan (pass at least one)" }}}),
            },
            McpToolDefinition {
                name: "oura_analyze_code".into(),
                description: "Analyze code for clean code violations (pass at least one file)".into(),
                input_schema: json!({"type": "object", "properties": {"files": { "type": "array", "items": { "type": "string" }, "description": "Files to analyze (pass at least one)" }}}),
            },
            McpToolDefinition {
                name: "oura_check_integrity".into(),
                description: "Check code integrity — always passes (baseline comparison not yet implemented)".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            McpToolDefinition {
                name: "oura_guard_destructive".into(),
                description: "[PLACEHOLDER] Acknowledge destructive operation guard (actual scanning not yet implemented)".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            McpToolDefinition {
                name: "oura_analyze_project".into(),
                description: "Scan project directory structure, docs, configs and sizes. Token-efficient project analysis for organization decisions.".into(),
                input_schema: json!({"type": "object", "properties": {"path": { "type": "string", "description": "Project root path (default: current dir)" }, "depth": { "type": "number", "description": "Max directory depth (default: 4, max: 10)", "default": 4 }}}),
            },
            McpToolDefinition {
                name: "oura_cleanup".into(),
                description: "Scan and clean temp files, build artifacts, caches and old logs. Safe dry-run by default.".into(),
                input_schema: json!({"type": "object", "properties": {"path": { "type": "string", "description": "Root path to scan (default: current dir)" }, "dry_run": { "type": "boolean", "description": "Only report, don't delete (default: true)", "default": true }, "confirm": { "type": "boolean", "description": "Must be true when dry_run=false to actually delete", "default": false }, "max_depth": { "type": "number", "description": "Max directory depth (default: 20)", "default": 20 }, "older_than_days": { "type": "number", "description": "Only touch files older than N days (default: 30)", "default": 30 }, "patterns": { "type": "array", "items": { "type": "string" }, "description": "File patterns to match (default: *.tmp, *.log, *.bak, __pycache__, etc)", "default": ["*.tmp", "*.temp", "*.log", "*.bak", "*.swp", "*.swo", "*.pyc", "__pycache__", ".DS_Store", "Thumbs.db"] }, "dir_patterns": { "type": "array", "items": { "type": "string" }, "description": "Directory names to remove entirely (default: node_modules, .next, .turbo, dist, build, .cache)", "default": ["node_modules", ".next", ".turbo", "dist", "build", ".cache"] }}}),
            },
            McpToolDefinition {
                name: "mcp_call".into(),
                description: "Call a tool on another MCP server via HTTP. Note: private/local addresses (localhost, 127.0.0.1, private IPs) are blocked for security.".into(),
                input_schema: json!({"type": "object", "properties": {"server_url": { "type": "string", "description": "Base URL of target MCP server (e.g. http://example.com:7438)" }, "tool_name": { "type": "string", "description": "Name of the tool to call" }, "arguments": { "type": "object", "description": "Arguments for the tool", "default": {} }, "endpoint": { "type": "string", "description": "Custom endpoint path (default: /message)", "default": "/message" } }, "required": ["server_url", "tool_name"]}),
            },
            McpToolDefinition {
                name: "oura_version".into(),
                description: "Show Oura version and build info.".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            McpToolDefinition {
                name: "oura_update".into(),
                description: "Check for updates and optionally rebuild Oura from source via git pull + cargo build.".into(),
                input_schema: json!({"type": "object", "properties": {"apply": { "type": "boolean", "description": "Actually pull and rebuild (default: false = dry-run check)", "default": false }}}),
            },
            McpToolDefinition {
                name: "oura_profile".into(),
                description: "Detect project profile: indie, studio, enterprise, game-dev, ai-ml, etc. Lightweight analysis, no telemetry.".into(),
                input_schema: json!({"type": "object", "properties": {"path": { "type": "string", "description": "Project path (default: current dir)" }}}),
            },
            McpToolDefinition {
                name: "oura_verify".into(),
                description: "Verify dependency licenses, versions, and project profile. Checks Cargo.toml, package.json, Cargo.lock.".into(),
                input_schema: json!({"type": "object", "properties": {"path": { "type": "string", "description": "Project path (default: current dir)" }, "check_licenses": { "type": "boolean", "description": "Check dependency licenses (default: true)", "default": true }, "check_versions": { "type": "boolean", "description": "Check version stability (default: true)", "default": true }}}),
            },
            McpToolDefinition {
                name: "oura_working_dir".into(),
                description: "Set the process working directory (affects all subsequent Oura commands and file operations)".into(),
                input_schema: json!({"type": "object", "properties": {"directory": { "type": "string", "description": "Absolute path to the working directory" }}, "required": ["directory"]}),
            },
            McpToolDefinition {
                name: "oura_connector".into(),
                description: "Call a tool on an external MCP server (e.g. Synapsis) via HTTP or QUIC. Used for cross-server synergy — call this manually or configure [connector] in config for auto-call during iteration.".into(),
                input_schema: json!({"type": "object", "properties": {"tool": { "type": "string", "description": "Name of the tool to call on the external server" }, "transport": { "type": "string", "description": "Transport protocol: 'http' (default) or 'quic'", "default": "http" }, "server_url": { "type": "string", "description": "HTTP URL (only for transport=http)", "default": "http://localhost:7438" }, "host": { "type": "string", "description": "QUIC host (only for transport=quic)", "default": "127.0.0.1" }, "port": { "type": "number", "description": "QUIC port (only for transport=quic)", "default": 7439 }, "arguments": { "type": "object", "description": "Arguments for the tool" }, "endpoint": { "type": "string", "description": "HTTP endpoint path (default: /message)", "default": "/message" }}, "required": ["tool"]}),
            },
        ];
        self.ok(id, json!({ "tools": tools }))
    }

    async fn handle_tools_call(&mut self, id: Value, params: Option<&Value>) -> JsonRpcResponse {
        let params = match params { Some(p) => p, None => return self.err(id, -32602, "Missing params".into()) };
        let name = params["name"].as_str().unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(json!({}));

        match name {
            "oura_start_loop" => self.cmd_start_loop(id, &args).await,
            "oura_iterate" => self.cmd_iterate(id).await,
            "oura_loop_status" => self.cmd_loop_status(id),
            "oura_loop_stop" => self.cmd_loop_stop(id),
            "oura_results" => self.cmd_results(id, &args),
            "oura_configure" => self.cmd_configure(id, &args),
            "oura_plugin_load" => self.cmd_plugin_load(id, &args),
            "oura_plugin_list" => self.cmd_plugin_list(id),
            "oura_analyze_security" => self.cmd_analyze_security(id, &args),
            "oura_analyze_code" => self.cmd_analyze_code(id, &args),
            "oura_check_integrity" => self.cmd_check_integrity(id),
            "oura_guard_destructive" => self.cmd_guard_destructive(id, &args),
            "oura_analyze_project" => self.cmd_analyze_project(id, &args),
            "oura_cleanup" => self.cmd_cleanup(id, &args),
            "mcp_call" => self.cmd_mcp_call(id, &args),
            "oura_version" => self.cmd_version(id),
            "oura_update" => self.cmd_update(id, &args).await,
            "oura_profile" => self.cmd_profile(id, &args),
            "oura_verify" => self.cmd_verify(id, &args),
            "oura_connector" => self.cmd_connector(id, &args).await,
            "oura_working_dir" => self.cmd_working_dir(id, &args),
            _ => self.err(id, -32602, format!("Unknown tool: {}", name)),
        }
    }
}
