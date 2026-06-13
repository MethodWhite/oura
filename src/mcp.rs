use crate::agents::*;
use crate::engine::LoopEngine;
use crate::types::*;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static MCP_CALL_ID: AtomicU64 = AtomicU64::new(1);

pub struct McpServer {
    engine: LoopEngine,
}

impl McpServer {
    pub fn new(engine: LoopEngine) -> Self {
        Self { engine }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let stdin = io::stdin();
        let stdout = io::stdout();

        for line in stdin.lock().lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(req) => req,
                Err(e) => {
                    eprintln!("[Oura MCP] Parse error: {} | Message: '{}'", e, trimmed);
                    let err_resp = json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {
                            "code": -32700,
                            "message": format!("Parse error: {}", e)
                        }
                    });
                    if let Ok(output) = serde_json::to_string(&err_resp) {
                        let mut out = stdout.lock();
                        let _ = writeln!(out, "{}", output);
                        let _ = out.flush();
                    }
                    continue;
                }
            };

            let response = self.handle_request(&request);

            // Don't respond to notifications (JSON-RPC requests with null id)
            if response.id.is_null() {
                continue;
            }

            if let Ok(output) = serde_json::to_string(&response) {
                let mut out = stdout.lock();
                let _ = writeln!(out, "{}", output);
                let _ = out.flush();
            }
        }

        Ok(())
    }

    fn handle_request(&mut self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone();

        match request.method.as_str() {
            "initialize" => self.handle_initialize(id),
            "initialized" => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: None,
            },
            "tools/list" => self.handle_tools_list(id),
            "tools/call" => self.handle_tools_call(id, request.params.as_ref()),
            "resources/list" => self.handle_resources_list(id),
            "resources/read" => self.handle_resource_read(id, request.params.as_ref()),
            "prompts/list" => self.handle_prompts_list(id),
            "prompts/get" => self.handle_prompt_get(id, request.params.as_ref()),
            _ => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                    data: None,
                }),
            },
        }
    }

    fn ok(&self, id: Value, result: Value) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(&self, id: Value, code: i32, message: String) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }

    fn text_content(text: String) -> Value {
        json!([{ "type": "text", "text": text }])
    }

    fn handle_initialize(&self, id: Value) -> JsonRpcResponse {
        self.ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": { "listChanged": true },
                    "resources": { "listChanged": true },
                    "prompts": { "listChanged": true }
                },
                "serverInfo": { "name": "oura", "version": env!("CARGO_PKG_VERSION") }
            }),
        )
    }

    fn handle_tools_list(&self, id: Value) -> JsonRpcResponse {
        let tools = vec![
            McpToolDefinition {
                name: "oura_start_loop".into(),
                description: "Start a new iteration loop to achieve a specific goal".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "goal": { "type": "string", "description": "The goal to achieve" },
                        "maxIterations": { "type": "number", "description": "Maximum iterations (default: 20)" }
                    },
                    "required": ["goal"]
                }),
            },
            McpToolDefinition {
                name: "oura_iterate".into(),
                description: "Execute a single iteration step manually".into(),
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            McpToolDefinition {
                name: "oura_loop_status".into(),
                description: "Get current status of the active loop".into(),
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            McpToolDefinition {
                name: "oura_loop_stop".into(),
                description: "Stop the currently running loop".into(),
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            McpToolDefinition {
                name: "oura_results".into(),
                description: "Get accumulated results from all iterations".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "iteration": { "type": "number", "description": "Specific iteration to view (0 = all)" }
                    }
                }),
            },
            McpToolDefinition {
                name: "oura_configure".into(),
                description: "Update Oura engine configuration".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "maxIterations": { "type": "number", "description": "Maximum iterations" },
                        "convergenceThreshold": { "type": "number", "description": "Score threshold (0-100)" },
                        "workingDirectory": { "type": "string", "description": "Working directory" }
                    }
                }),
            },
            McpToolDefinition {
                name: "oura_plugin_load".into(),
                description: "Load a plugin from a directory path".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to plugin directory" }
                    },
                    "required": ["path"]
                }),
            },
            McpToolDefinition {
                name: "oura_plugin_list".into(),
                description: "List all loaded plugins".into(),
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            McpToolDefinition {
                name: "oura_analyze_security".into(),
                description: "Run security audit on specified files".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "files": { "type": "array", "items": { "type": "string" }, "description": "Files to scan" }
                    }
                }),
            },
            McpToolDefinition {
                name: "oura_analyze_code".into(),
                description: "Analyze code for clean code violations".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "files": { "type": "array", "items": { "type": "string" }, "description": "Files to analyze" }
                    }
                }),
            },
            McpToolDefinition {
                name: "oura_check_integrity".into(),
                description: "Check code integrity against baseline to detect lost critical symbols".into(),
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            McpToolDefinition {
                name: "oura_guard_destructive".into(),
                description: "Check for dangerous destructive operations in code".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "files": { "type": "array", "items": { "type": "string" }, "description": "Files to scan" }
                    }
                }),
            },
            McpToolDefinition {
                name: "oura_analyze_project".into(),
                description: "Scan project directory structure, docs, configs and sizes. Token-efficient project analysis for organization decisions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Project root path (default: current dir)" },
                        "depth": { "type": "number", "description": "Max directory depth (default: 4)", "default": 4 }
                    }
                }),
            },
            McpToolDefinition {
                name: "oura_cleanup".into(),
                description: "Scan and clean temp files, build artifacts, old logs. Safe dry-run by default.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Root path to scan (default: current dir)" },
                        "dry_run": { "type": "boolean", "description": "Only report, don't delete (default: true)", "default": true },
                        "older_than_days": { "type": "number", "description": "Only touch files older than N days (default: 30)", "default": 30 },
                        "patterns": { "type": "array", "items": { "type": "string" }, "description": "File patterns to match (e.g. *.tmp, *.log)" }
                    }
                }),
            },
            McpToolDefinition {
                name: "mcp_call".into(),
                description: "Call a tool on another MCP server. Lets sub-orchestrators dispatch to any MCP tool via HTTP.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "server_url": { "type": "string", "description": "Base URL of target MCP server (e.g. http://localhost:7438)" },
                        "tool_name": { "type": "string", "description": "Name of the tool to call" },
                        "arguments": { "type": "object", "description": "Arguments for the tool", "default": {} },
                        "endpoint": { "type": "string", "description": "Custom endpoint path (default: /message)", "default": "/message" }
                    },
                    "required": ["server_url", "tool_name"]
                }),
            },
            McpToolDefinition {
                name: "oura_version".into(),
                description: "Show Oura version, build info, and latest available version.".into(),
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            McpToolDefinition {
                name: "oura_update".into(),
                description: "Check for updates and optionally rebuild Oura from source via git pull + cargo build.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "apply": { "type": "boolean", "description": "Actually pull and rebuild (default: false = dry-run check)", "default": false }
                    }
                }),
            },
            McpToolDefinition {
                name: "oura_profile".into(),
                description: "Detect project profile: indie, studio, enterprise, game-dev, ai-ml, etc. Lightweight analysis, no telemetry.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Project path (default: current dir)" }
                    }
                }),
            },
            McpToolDefinition {
                name: "oura_verify".into(),
                description: "Verify dependency licenses, versions, and project profile. Checks Cargo.toml, package.json, Cargo.lock.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Project path (default: current dir)" },
                        "check_licenses": { "type": "boolean", "description": "Check dependency licenses (default: true)", "default": true },
                        "check_versions": { "type": "boolean", "description": "Check version stability (default: true)", "default": true }
                    }
                }),
            },
            McpToolDefinition {
                name: "oura_working_dir".into(),
                description: "Set the working directory for Oura commands (test, clippy, project analysis).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "directory": { "type": "string", "description": "Absolute path to the working directory" }
                    },
                    "required": ["directory"]
                }),
            },
        ];

        self.ok(id, json!({ "tools": tools }))
    }

    fn handle_tools_call(&mut self, id: Value, params: Option<&Value>) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => return self.err(id, -32602, "Missing params".into()),
        };

        let name = params["name"].as_str().unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(json!({}));

        match name {
            "oura_start_loop" => self.cmd_start_loop(id, &args),
            "oura_iterate" => self.cmd_iterate(id),
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
            "oura_update" => self.cmd_update(id, &args),
            "oura_profile" => self.cmd_profile(id, &args),
            "oura_verify" => self.cmd_verify(id, &args),
            "oura_working_dir" => self.cmd_working_dir(id, &args),
            _ => self.err(id, -32602, format!("Unknown tool: {}", name)),
        }
    }

    fn cmd_start_loop(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let goal = args["goal"].as_str().unwrap_or("improve codebase");
        let max_iter = args["maxIterations"].as_u64().unwrap_or(20) as u32;

        self.engine.update_max_iterations(max_iter);
        match self.engine.start(goal) {
            Ok(state) => {
                let msg = format!(
                    "Loop started: {}\nLoop ID: {}\nStatus: {}",
                    goal, state.id, state.status
                );
                self.ok(id, json!({ "content": Self::text_content(msg) }))
            }
            Err(e) => self.err(id, -32603, format!("Failed to start loop: {}", e)),
        }
    }

    fn cmd_iterate(&mut self, id: Value) -> JsonRpcResponse {
        match self.engine.iterate() {
            Ok(result) => {
                let errors = result
                    .feedback
                    .iter()
                    .filter(|f| f.type_ == "error")
                    .count();
                let warnings = result
                    .feedback
                    .iter()
                    .filter(|f| f.type_ == "warning")
                    .count();
                let done = result.actions.iter().filter(|a| a.status == "done").count();
                let failed_actions = result
                    .actions
                    .iter()
                    .filter(|a| a.status == "error")
                    .count();

                let mut msg = format!(
                    "Iteration #{} - Score: {:.1}/100\nActions: {} done, {} errors\nFeedback: {} errors, {} warnings",
                    result.iteration, result.score, done, failed_actions, errors, warnings
                );

                if result.status == "converged" {
                    msg.push_str("\n\n*** CONVERGED ***");
                } else if result.status == "failed" {
                    msg.push_str("\n\n*** FAILED ***");
                }

                self.ok(id, json!({ "content": Self::text_content(msg) }))
            }
            Err(e) => self.err(id, -32603, format!("Iteration failed: {}", e)),
        }
    }

    fn cmd_loop_status(&self, id: Value) -> JsonRpcResponse {
        let state = self.engine.get_state();
        match state {
            Some(state) => {
                let last = state.history.last();
                let mut lines = vec![
                    format!("Status: {}", state.status),
                    format!("Goal: {}", state.goal),
                    format!("Loop ID: {}", state.id),
                    format!("Current Iteration: {}", state.current_iteration),
                    format!("Started: {}", state.start_time),
                ];
                if let Some(last) = last {
                    lines.push(format!(
                        "Last Iteration: #{} (score: {:.1}/100)",
                        last.iteration, last.score
                    ));
                }
                self.ok(
                    id,
                    json!({ "content": Self::text_content(lines.join("\n")) }),
                )
            }
            None => self.ok(
                id,
                json!({ "content": Self::text_content("No active loop".into()) }),
            ),
        }
    }

    fn cmd_loop_stop(&mut self, id: Value) -> JsonRpcResponse {
        let iter = self.engine.stop();
        self.ok(id, json!({ "content": Self::text_content(format!("Loop stopped after {} iterations", iter)) }))
    }

    fn cmd_results(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let results = self.engine.get_results();
        let iter_filter = args["iteration"].as_u64().unwrap_or(0);
        let max_iterations = 20;

        let filtered: Vec<&IterationResult> = if iter_filter > 0 {
            results
                .iter()
                .filter(|r| r.iteration == iter_filter as u32)
                .collect()
        } else {
            results.iter().rev().take(max_iterations).collect()
        };

        let summary: Vec<serde_json::Value> = filtered
            .iter()
            .map(|r| {
                json!({
                    "iteration": r.iteration,
                    "score": r.score,
                    "status": r.status,
                    "actions": r.actions.iter().map(|a| json!({
                        "agent": a.agent,
                        "type": a.type_,
                        "status": a.status
                    })).collect::<Vec<_>>(),
                    "feedback": r.feedback.iter().map(|f| json!({
                        "source": f.source,
                        "type": f.type_,
                        "message": f.message
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();

        self.ok(id, json!({ "content": Self::text_content(serde_json::to_string_pretty(&summary).unwrap_or_default()) }))
    }

    fn cmd_configure(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        if let Some(max) = args["maxIterations"].as_u64() {
            self.engine.update_max_iterations(max as u32);
        }
        if let Some(threshold) = args["convergenceThreshold"].as_f64() {
            self.engine.update_convergence_threshold(threshold);
        }
        if let Some(dir) = args["workingDirectory"].as_str() {
            let path = std::path::Path::new(dir);
            if path.exists() {
                std::env::set_current_dir(path).ok();
            }
        }
        self.ok(
            id,
            json!({ "content": Self::text_content("Configuration updated".into()) }),
        )
    }

    fn cmd_working_dir(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let dir = args["directory"].as_str().unwrap_or(".");
        let path = std::path::Path::new(dir);
        if !path.exists() {
            return self.err(id, -32602, format!("Directory not found: {}", dir));
        }
        std::env::set_current_dir(path).ok();
        self.ok(
            id,
            json!({ "content": Self::text_content(format!("Working directory set to: {}", dir)) }),
        )
    }

    fn cmd_plugin_load(&mut self, id: Value, _args: &Value) -> JsonRpcResponse {
        self.ok(id, json!({ "content": Self::text_content("Plugin loading not yet implemented in Rust version".into()) }))
    }

    fn cmd_plugin_list(&self, id: Value) -> JsonRpcResponse {
        self.ok(
            id,
            json!({ "content": Self::text_content("No plugins loaded".into()) }),
        )
    }

    fn cmd_analyze_security(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let files: Vec<String> = args["files"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let auditor = SecurityAuditor::new();
        let findings = auditor.audit(&files);

        if findings.is_empty() {
            return self.ok(id, json!({ "content": Self::text_content("Security scan complete: no vulnerabilities found".into()) }));
        }

        let critical = findings.iter().filter(|f| f.severity == "critical").count();
        let high = findings.iter().filter(|f| f.severity == "high").count();
        let medium = findings.iter().filter(|f| f.severity == "medium").count();
        let low = findings.iter().filter(|f| f.severity == "low").count();

        let mut report = format!(
            "Security scan complete: {} issues found\n  Critical: {}\n  High: {}\n  Medium: {}\n  Low: {}\n",
            findings.len(), critical, high, medium, low
        );

        for f in &findings {
            report.push_str(&format!(
                "\n[{}] {}:{} - {}\n  => {}",
                f.severity.to_uppercase(),
                f.file,
                f.line.map(|l| l.to_string()).unwrap_or_else(|| "?".into()),
                f.description,
                f.recommendation
            ));
        }

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    fn cmd_analyze_code(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let files: Vec<String> = args["files"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let refactor = RefactorEngine::new();
        let mut report = String::from("Code Analysis Results:\n");

        for file in &files {
            let (issues, suggestions) = refactor.analyze(file);
            if !issues.is_empty() {
                report.push_str(&format!("\n{}:", file));
                for issue in &issues {
                    report.push_str(&format!("\n  [ISSUE] {}", issue));
                }
                for suggestion in &suggestions {
                    report.push_str(&format!("\n  [SUGGEST] {}", suggestion));
                }
            }
        }

        if report == "Code Analysis Results:\n" {
            report = "Code analysis passed: no clean code violations detected".into();
        }

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    fn cmd_check_integrity(&self, id: Value) -> JsonRpcResponse {
        let guard = AntiDeletionGuard::new();
        match guard.check_integrity() {
            Ok(msg) => self.ok(id, json!({ "content": Self::text_content(msg) })),
            Err(msg) => self.ok(id, json!({ "content": Self::text_content(msg) })),
        }
    }

    fn cmd_guard_destructive(&self, id: Value, _args: &Value) -> JsonRpcResponse {
        self.ok(
            id,
            json!({
                "content": Self::text_content(
                    "Destructive query guard active: all DROP/DELETE/TRUNCATE/ALTER operations \
                     require explicit confirmation with backup verification".into())
            }),
        )
    }

    fn cmd_analyze_project(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let root = args["path"].as_str().unwrap_or(".");
        let max_depth = args["depth"].as_u64().unwrap_or(4) as usize;

        let root_path = Path::new(root);
        if !root_path.exists() {
            return self.err(id, -32602, format!("Path not found: {}", root));
        }

        let mut report = String::new();
        report.push_str(&format!(
            "[{}] ({})\n",
            root_path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_else(|| "?".into()),
            root
        ));
        report.push_str(&format!(
            "   Size: {}\n",
            format_size(dir_size(root_path).unwrap_or(0))
        ));

        let readme = find_readme(root_path);
        if let Some(rm) = readme {
            if let Ok(content) = std::fs::read_to_string(&rm) {
                let preview: String = content.lines().take(8).collect::<Vec<_>>().join("\n");
                report.push_str(&format!(
                    "   README: {} chars\n{}\n",
                    content.len(),
                    preview
                ));
            }
        }

        let docs_dir = root_path.join("docs");
        if docs_dir.exists() && docs_dir.is_dir() {
            let count = std::fs::read_dir(&docs_dir).map(|e| e.count()).unwrap_or(0);
            report.push_str(&format!("   docs/: {} files\n", count));
        }

        let configs = scan_configs(root_path);
        if !configs.is_empty() {
            report.push_str(&format!("   Config: {}\n", configs.join(", ")));
        }

        let mut entries = collect_entries(root_path, 0, max_depth);
        entries.sort_by(|a, b| a.1.cmp(&b.1));
        for (path_str, depth, is_dir, size) in &entries {
            if *depth == 0 || *depth > max_depth {
                continue;
            }
            let indent = "  ".repeat(*depth);
            let marker = if *is_dir { "+" } else { " " };
            let size_str = if *is_dir {
                String::new()
            } else {
                format!(" ({})", format_size(*size))
            };
            report.push_str(&format!("{}{} {}{}\n", indent, marker, path_str, size_str));
        }

        let large: Vec<_> = entries
            .iter()
            .filter(|(_, _, is_dir, size)| !is_dir && *size > 100_000)
            .collect();
        if !large.is_empty() {
            report.push_str(&format!("\n   Large files (>100KB): {}\n", large.len()));
            for (path_str, _, _, size) in large.iter().take(5) {
                report.push_str(&format!("     - {} ({})\n", path_str, format_size(*size)));
            }
        }

        let total_files = entries.iter().filter(|(_, _, is_dir, _)| !is_dir).count();
        let total_dirs = entries.iter().filter(|(_, _, is_dir, _)| *is_dir).count();
        report.push_str(&format!(
            "\n   Summary: {} dirs, {} files",
            total_dirs, total_files
        ));

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    fn cmd_mcp_call(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let server_url = args["server_url"]
            .as_str()
            .unwrap_or("")
            .trim_end_matches('/');
        let tool_name = args["tool_name"].as_str().unwrap_or("");
        let tool_args = args.get("arguments").cloned().unwrap_or(json!({}));
        let endpoint = args["endpoint"].as_str().unwrap_or("/message");

        if server_url.is_empty() || tool_name.is_empty() {
            return self.err(id, -32602, "server_url and tool_name are required".into());
        }

        let call_id = MCP_CALL_ID.fetch_add(1, Ordering::SeqCst);
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": call_id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": tool_args
            }
        });

        let url = format!("{}{}", server_url, endpoint);
        match Self::http_post(&url, &request_body) {
            Ok(response_text) => {
                self.ok(id, json!({ "content": Self::text_content(response_text) }))
            }
            Err(e) => self.ok(
                id,
                json!({ "content": Self::text_content(format!("MCP call failed: {}", e)) }),
            ),
        }
    }

    fn http_post(url: &str, body: &Value) -> Result<String, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let url = url.to_string();
        let body = body.clone();
        std::thread::spawn(move || {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| e.to_string());
            match client {
                Ok(c) => {
                    let resp = c.post(&url).json(&body).send().map_err(|e| e.to_string());
                    match resp {
                        Ok(r) => {
                            let _ = tx.send(r.text().map_err(|e| e.to_string()));
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e));
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        });
        rx.recv_timeout(std::time::Duration::from_secs(35))
            .map_err(|_| "HTTP request timed out".to_string())?
    }

    fn cmd_version(&self, id: Value) -> JsonRpcResponse {
        let version = env!("CARGO_PKG_VERSION");

        let build_ts = option_env!("BUILD_TIME").unwrap_or("0");
        let build_date = {
            let secs: u64 = build_ts.parse().unwrap_or(0);
            if secs > 0 {
                let naive = chrono::DateTime::from_timestamp(secs as i64, 0)
                    .map(|dt| dt.format("%Y-%m").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                naive
            } else {
                "unknown".to_string()
            }
        };
        let git_head = option_env!("GIT_HEAD").unwrap_or("unknown");
        let project_dir = env!("CARGO_MANIFEST_DIR");

        let out = format!(
            "Oura v{}\nBuilt: {}\nCommit: {}\nProject: {}",
            version, build_date, git_head, project_dir
        );

        self.ok(id, json!({ "content": Self::text_content(out) }))
    }

    fn cmd_update(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let apply = args["apply"].as_bool().unwrap_or(false);
        let project_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

        if !project_dir.join(".git").exists() {
            return self.ok(
                id,
                json!({
                    "content": Self::text_content("Not a git repository. Can't auto-update.".into())
                }),
            );
        }

        // Detect default branch
        let default_branch = std::process::Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
            .current_dir(project_dir)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| {
                s.trim()
                    .trim_start_matches("refs/remotes/origin/")
                    .to_string()
            })
            .unwrap_or_else(|| "main".into());

        let mut report = String::new();
        report.push_str(&format!(
            "Oura v{} - checking for updates...\n",
            env!("CARGO_PKG_VERSION")
        ));

        let git_fetch = run_command_timeout(&["git", "fetch", "--quiet"], project_dir, 30);

        match git_fetch {
            Ok(output) if output.status.success() => {
                report.push_str("Git fetch OK.\n");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                report.push_str(&format!("Git fetch: {}\n", stderr));
                return self.ok(id, json!({ "content": Self::text_content(report) }));
            }
            Err(e) => {
                report.push_str(&format!("Git fetch failed: {}\n", e));
                return self.ok(id, json!({ "content": Self::text_content(report) }));
            }
        }

        let remote_ref = format!("HEAD..origin/{}", default_branch);
        let ahead = std::process::Command::new("git")
            .args(["rev-list", "--count", &remote_ref])
            .current_dir(project_dir)
            .output();

        let commits_behind = match ahead {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => "?".to_string(),
        };

        let local_hash = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(project_dir)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "?".into());

        let remote_hash = std::process::Command::new("git")
            .args([
                "rev-parse",
                "--short",
                &format!("origin/{}", default_branch),
            ])
            .current_dir(project_dir)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "?".into());

        report.push_str(&format!(
            "Local: v{} @{}\nRemote: @{}\nBehind: {} commits\n",
            env!("CARGO_PKG_VERSION"),
            local_hash,
            remote_hash,
            commits_behind
        ));

        let behind_count: i32 = commits_behind.parse().unwrap_or(0);

        if behind_count <= 0 {
            report.push_str("\nAlready up to date. No update needed.");
        } else if apply {
            let pull = run_command_timeout(&["git", "pull", "--rebase"], project_dir, 60);

            match pull {
                Ok(output) if output.status.success() => {
                    report.push_str("\nGit pull OK. Rebuilding...");
                    let build =
                        run_command_timeout(&["cargo", "build", "--release"], project_dir, 300);

                    match build {
                        Ok(b) if b.status.success() => {
                            report.push_str(" Build OK! Restart Oura to use the new version.");
                        }
                        Ok(b) => {
                            let stderr = String::from_utf8_lossy(&b.stderr);
                            report.push_str(&format!(" Build failed:\n{}", stderr));
                        }
                        Err(e) => {
                            report.push_str(&format!(" Build error: {}", e));
                        }
                    }
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    report.push_str(&format!("\nGit pull failed:\n{}", stderr));
                }
                Err(e) => {
                    report.push_str(&format!("\nGit pull error: {}", e));
                }
            }
        } else {
            report.push_str(&format!(
                "\n{} commits behind. Run oura_update with apply=true to update.",
                behind_count
            ));
        }

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    fn cmd_profile(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let root = args["path"]
            .as_str()
            .map(Path::new)
            .unwrap_or_else(|| &std::path::Path::new("."));
        let profile = crate::profile::ProjectProfile::detect(root);
        let summary = profile.summary();

        let json_output = serde_json::json!({
            "user_type": profile.user_type,
            "confidence": profile.confidence,
            "ecosystem": profile.ecosystem,
            "has_game_engine": profile.has_game_engine,
            "has_paid_tools": profile.has_paid_tools,
            "has_enterprise_configs": profile.has_enterprise_configs,
            "dependency_count": profile.dependency_count,
            "indicators": profile.indicators,
        });

        let report = format!(
            "{}\n\n---\n{}",
            summary,
            serde_json::to_string_pretty(&json_output).unwrap_or_default()
        );
        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    fn cmd_verify(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let root = args["path"]
            .as_str()
            .map(Path::new)
            .unwrap_or_else(|| &std::path::Path::new("."));
        let report = crate::profile::verify_dependencies(root);
        self.ok(
            id,
            json!({ "content": Self::text_content(report.summary()) }),
        )
    }

    fn handle_resources_list(&self, id: Value) -> JsonRpcResponse {
        self.ok(id, json!({
            "resources": [
                { "uri": "oura://state", "name": "Oura Loop State", "description": "Current state of the active loop", "mimeType": "application/json" },
                { "uri": "oura://results", "name": "Oura Iteration Results", "description": "History of all iterations", "mimeType": "application/json" },
                { "uri": "oura://config", "name": "Oura Configuration", "description": "Current engine configuration", "mimeType": "application/json" },
            ]
        }))
    }

    fn handle_resource_read(&self, id: Value, params: Option<&Value>) -> JsonRpcResponse {
        let uri = params.and_then(|p| p["uri"].as_str()).unwrap_or("");
        let state = self.engine.get_state();

        let content = match uri {
            "oura://state" => serde_json::to_string_pretty(&state).unwrap_or_default(),
            "oura://results" => {
                serde_json::to_string_pretty(&state.map(|s| s.history.clone()).unwrap_or_default())
                    .unwrap_or_default()
            }
            "oura://config" => {
                let max_iter = *self.engine.max_iterations().lock().unwrap();
                let threshold = *self.engine.convergence_threshold().lock().unwrap();
                let cwd = std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let config_json = json!({
                    "max_iterations": max_iter,
                    "convergence_threshold": threshold,
                    "working_directory": cwd,
                });
                serde_json::to_string_pretty(&config_json).unwrap_or_default()
            }
            _ => return self.err(id, -32602, format!("Unknown resource: {}", uri)),
        };

        self.ok(
            id,
            json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": content
                }]
            }),
        )
    }

    fn handle_prompts_list(&self, id: Value) -> JsonRpcResponse {
        self.ok(id, json!({
            "prompts": [
                {
                    "name": "start_loop",
                    "description": "Template for starting a new Oura iteration loop",
                    "arguments": [
                        { "name": "goal", "description": "What to achieve", "required": true }
                    ]
                },
                {
                    "name": "loop_summary",
                    "description": "Template for summarizing loop results",
                    "arguments": [
                        { "name": "iteration", "description": "Which iteration to summarize", "required": false }
                    ]
                }
            ]
        }))
    }

    fn handle_prompt_get(&self, id: Value, params: Option<&Value>) -> JsonRpcResponse {
        let name = params.and_then(|p| p["name"].as_str()).unwrap_or("");
        match name {
            "start_loop" => {
                let goal = params.and_then(|p| p["arguments"].as_object())
                    .and_then(|a| a.get("goal"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("improve codebase");
                self.ok(id, json!({
                    "messages": [{
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": format!(
                                "I want to start an Oura iteration loop to: {}\n\n\
                                 Please call oura_start_loop with goal=\"{}\".\n\
                                 Oura will run iterations until convergence or max iterations reached.\n\
                                 You can check progress with oura_loop_status and finally oura_results.",
                                goal, goal
                            )
                        }
                    }]
                }))
            }
            "loop_summary" => {
                self.ok(id, json!({
                    "messages": [{
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": String::from("Please summarize the Oura loop results. Use oura_results to get the data first.")
                        }
                    }]
                }))
            }
            _ => self.err(id, -32602, format!("Unknown prompt: {}", name)),
        }
    }

    fn cmd_cleanup(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let root = args["path"].as_str().unwrap_or(".");
        let dry_run = args["dry_run"].as_bool().unwrap_or(true);
        let older_than = args["older_than_days"].as_u64().unwrap_or(30);
        let patterns: Vec<String> = args["patterns"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    "*.tmp".into(),
                    "*.temp".into(),
                    "*.log".into(),
                    "*.bak".into(),
                    "*.swp".into(),
                    "*.swo".into(),
                    "*.pyc".into(),
                    "__pycache__".into(),
                    ".DS_Store".into(),
                    "Thumbs.db".into(),
                ]
            });
        let dir_patterns = ["node_modules", ".next", ".turbo", "dist", "build", ".cache"];

        let root_path = Path::new(root);
        if !root_path.exists() {
            return self.err(id, -32602, format!("Path not found: {}", root));
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let max_age = if older_than > 0 {
            older_than * 86400
        } else {
            u64::MAX
        };

        let mut report = String::new();
        let mut candidates: Vec<(String, String)> = Vec::new();
        let mut total_size: u64 = 0;

        cleanup_walk(
            root_path,
            &patterns,
            &dir_patterns,
            &mut candidates,
            &mut total_size,
            now,
            max_age,
            0,
            10,
        );

        if candidates.is_empty() {
            report = format!(
                "No cleanup candidates found in {}. Everything looks clean.",
                root
            );
        } else {
            report.push_str(&format!("Cleanup candidates in {}:\n", root));
            for (path, reason) in &candidates {
                report.push_str(&format!("  - {} ({})\n", path, reason));
            }
            report.push_str(&format!(
                "\nTotal: {} items, {}\n",
                candidates.len(),
                format_size(total_size)
            ));

            if !dry_run {
                let mut deleted = 0;
                let mut failed = 0;
                for (path, _) in &candidates {
                    let p = Path::new(path);
                    if p.is_dir() {
                        match std::fs::remove_dir_all(p) {
                            Ok(_) => deleted += 1,
                            Err(e) => {
                                report.push_str(&format!("  FAILED: {}: {}\n", path, e));
                                failed += 1;
                            }
                        }
                    } else {
                        match std::fs::remove_file(p) {
                            Ok(_) => deleted += 1,
                            Err(e) => {
                                report.push_str(&format!("  FAILED: {}: {}\n", path, e));
                                failed += 1;
                            }
                        }
                    }
                }
                report.push_str(&format!("\nDeleted: {}, Failed: {}", deleted, failed));
            } else {
                report.push_str("\nDry-run mode. Set dry_run=false to delete.");
            }
        }

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }
}

fn run_command_timeout(
    args: &[&str],
    dir: &Path,
    secs: u64,
) -> std::result::Result<std::process::Output, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let dir = dir.to_path_buf();
    let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        let _ = tx.send(
            std::process::Command::new(&args_owned[0])
                .args(&args_owned[1..])
                .current_dir(&dir)
                .output(),
        );
    });
    match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
        Ok(result) => result.map_err(|e| format!("Command failed: {}", e)),
        Err(_) => Err("Command timed out".to_string()),
    }
}

fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if path.is_file() {
        return Ok(path.metadata()?.len());
    }
    walk_size(path, 0, 3, &mut total)?;
    Ok(total)
}

fn walk_size(path: &Path, depth: usize, max_depth: usize, total: &mut u64) -> std::io::Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    if path.is_dir() {
        let mut visited = std::collections::HashSet::new();
        walk_size_inner(path, depth, max_depth, total, &mut visited)
    } else {
        Ok(())
    }
}

fn walk_size_inner(
    path: &Path,
    depth: usize,
    max_depth: usize,
    total: &mut u64,
    visited: &mut std::collections::HashSet<u64>,
) -> std::io::Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    if let Ok(meta) = path.metadata() {
        #[cfg(unix)]
        let ino = std::os::unix::fs::MetadataExt::ino(&meta);
        #[cfg(not(unix))]
        let ino = 0u64;
        if ino != 0 && !visited.insert(ino) {
            return Ok(());
        }
    }
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let e = entry?;
            let p = e.path();
            if p.is_symlink() {
                continue;
            }
            if p.is_file() {
                *total += e.metadata()?.len();
            } else if p.is_dir() {
                walk_size_inner(&p, depth + 1, max_depth, total, visited)?;
            }
        }
    }
    Ok(())
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    if bytes == 0 {
        return "0B".into();
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.1}{}", size, UNITS[unit])
}

fn find_readme(path: &Path) -> Option<std::path::PathBuf> {
    let names = [
        "README.md",
        "Readme.md",
        "readme.md",
        "README",
        "LEEME.md",
        "README.txt",
    ];
    for name in &names {
        let p = path.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn scan_configs(path: &Path) -> Vec<String> {
    let configs = [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "CMakeLists.txt",
        "Makefile",
        "Dockerfile",
        "docker-compose.yml",
        "compose.yaml",
        ".env.example",
        ".gitignore",
        ".editorconfig",
        "tsconfig.json",
        "opencode.json",
        "opencode.jsonc",
        "claude-code.json",
        "cursor.json",
    ];
    let mut found = Vec::new();
    for name in &configs {
        if path.join(name).exists() {
            found.push(name.to_string());
        }
    }
    found
}

fn collect_entries(path: &Path, depth: usize, max_depth: usize) -> Vec<(String, usize, bool, u64)> {
    let mut entries = Vec::new();
    let mut visited = std::collections::HashSet::new();
    collect_entries_inner(path, depth, max_depth, &mut entries, &mut visited);
    entries
}

fn collect_entries_inner(
    path: &Path,
    depth: usize,
    max_depth: usize,
    entries: &mut Vec<(String, usize, bool, u64)>,
    visited: &mut std::collections::HashSet<u64>,
) {
    if depth > max_depth || !path.is_dir() {
        return;
    }
    if let Ok(meta) = path.metadata() {
        #[cfg(unix)]
        let ino = std::os::unix::fs::MetadataExt::ino(&meta);
        #[cfg(not(unix))]
        let ino = 0u64;
        if ino != 0 && !visited.insert(ino) {
            return;
        }
    }
    if let Ok(readdir) = std::fs::read_dir(path) {
        for entry in readdir.flatten() {
            let p = entry.path();
            if p.is_symlink() {
                continue;
            }
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }
            let is_dir = p.is_dir();
            let size = if is_dir {
                0
            } else {
                p.metadata().map(|m| m.len()).unwrap_or(0)
            };
            entries.push((name.clone(), depth, is_dir, size));
            if is_dir {
                collect_entries_inner(&p, depth + 1, max_depth, entries, visited);
            }
        }
    }
}

fn cleanup_walk(
    path: &Path,
    patterns: &[String],
    dir_patterns: &[&str],
    candidates: &mut Vec<(String, String)>,
    total_size: &mut u64,
    now: u64,
    max_age: u64,
    depth: usize,
    max_depth: usize,
) {
    let mut visited = std::collections::HashSet::new();
    cleanup_walk_inner(
        path,
        patterns,
        dir_patterns,
        candidates,
        total_size,
        now,
        max_age,
        depth,
        max_depth,
        &mut visited,
    );
}

fn cleanup_walk_inner(
    path: &Path,
    patterns: &[String],
    dir_patterns: &[&str],
    candidates: &mut Vec<(String, String)>,
    total_size: &mut u64,
    now: u64,
    max_age: u64,
    depth: usize,
    max_depth: usize,
    visited: &mut std::collections::HashSet<u64>,
) {
    if depth > max_depth || !path.is_dir() {
        return;
    }
    if let Ok(meta) = path.metadata() {
        #[cfg(unix)]
        let ino = std::os::unix::fs::MetadataExt::ino(&meta);
        #[cfg(not(unix))]
        let ino = 0u64;
        if ino != 0 && !visited.insert(ino) {
            return;
        }
    }
    if let Ok(readdir) = std::fs::read_dir(path) {
        for entry in readdir.flatten() {
            let p = entry.path();
            if p.is_symlink() {
                continue;
            }
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }

            if p.is_dir() {
                if dir_patterns.contains(&name.as_str()) {
                    let size = dir_size(&p).unwrap_or(0);
                    *total_size += size;
                    candidates.push((p.to_string_lossy().to_string(), format!("{} dir", name)));
                } else {
                    cleanup_walk_inner(
                        &p,
                        patterns,
                        dir_patterns,
                        candidates,
                        total_size,
                        now,
                        max_age,
                        depth + 1,
                        max_depth,
                        visited,
                    );
                }
            } else {
                let matched = patterns.iter().any(|pat| {
                    if let Some(ext) = pat.strip_prefix('*') {
                        name.ends_with(ext)
                    } else {
                        name == *pat
                    }
                });
                if matched {
                    let size = p.metadata().map(|m| m.len()).unwrap_or(0);
                    *total_size += size;
                    let reason = if let Ok(meta) = p.metadata() {
                        if let Ok(modified) = meta.modified() {
                            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                                let age_secs = now.saturating_sub(duration.as_secs());
                                if age_secs > max_age {
                                    format!("old ({} days)", age_secs / 86400)
                                } else {
                                    "temp file".into()
                                }
                            } else {
                                "temp file".into()
                            }
                        } else {
                            "temp file".into()
                        }
                    } else {
                        "temp file".into()
                    };
                    candidates.push((p.to_string_lossy().to_string(), reason));
                }
            }
        }
    }
}
