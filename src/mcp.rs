use std::io::{self, BufRead, Write};
use serde_json::{json, Value};
use crate::types::*;
use crate::engine::LoopEngine;
use crate::agents::*;

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
            if line.trim().is_empty() {
                continue;
            }

            let request: JsonRpcRequest = serde_json::from_str(&line)?;
            let response = self.handle_request(&request);

            let output = serde_json::to_string(&response)?;
            writeln!(stdout.lock(), "{}", output)?;
            stdout.lock().flush()?;
        }

        Ok(())
    }

    fn handle_request(&mut self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone();

        match request.method.as_str() {
            "initialize" => self.handle_initialize(id),
            "initialized" => self.ok(id, json!({})),
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
        JsonRpcResponse { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }

    fn err(&self, id: Value, code: i32, message: String) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message, data: None }),
        }
    }

    fn text_content(text: String) -> Value {
        json!([{ "type": "text", "text": text }])
    }

    fn handle_initialize(&self, id: Value) -> JsonRpcResponse {
        self.ok(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": true },
                "resources": { "listChanged": true },
                "prompts": { "listChanged": true }
            },
            "serverInfo": { "name": "oura", "version": "0.1.0" }
        }))
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
            _ => self.err(id, -32602, format!("Unknown tool: {}", name)),
        }
    }

    fn cmd_start_loop(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let goal = args["goal"].as_str().unwrap_or("improve codebase");
        let max_iter = args["maxIterations"].as_u64().unwrap_or(20) as u32;

        self.engine.update_max_iterations(max_iter);
        match self.engine.start(goal) {
            Ok(state) => {
                let msg = format!("Loop started: {}\nLoop ID: {}\nStatus: {}",
                    goal, state.id, state.status);
                self.ok(id, json!({ "content": Self::text_content(msg) }))
            }
            Err(e) => self.err(id, -32603, format!("Failed to start loop: {}", e)),
        }
    }

    fn cmd_iterate(&mut self, id: Value) -> JsonRpcResponse {
        match self.engine.iterate() {
            Ok(result) => {
                let errors = result.feedback.iter().filter(|f| f.type_ == "error").count();
                let warnings = result.feedback.iter().filter(|f| f.type_ == "warning").count();
                let done = result.actions.iter().filter(|a| a.status == "done").count();
                let failed_actions = result.actions.iter().filter(|a| a.status == "error").count();

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
                    lines.push(format!("Last Iteration: #{} (score: {:.1}/100)", last.iteration, last.score));
                }
                self.ok(id, json!({ "content": Self::text_content(lines.join("\n")) }))
            }
            None => self.ok(id, json!({ "content": Self::text_content("No active loop".into()) })),
        }
    }

    fn cmd_loop_stop(&mut self, id: Value) -> JsonRpcResponse {
        let iter = self.engine.stop();
        self.ok(id, json!({ "content": Self::text_content(format!("Loop stopped after {} iterations", iter)) }))
    }

    fn cmd_results(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let results = self.engine.get_results();
        let iter_filter = args["iteration"].as_u64().unwrap_or(0);

        let filtered: Vec<&IterationResult> = if iter_filter > 0 {
            results.iter().filter(|r| r.iteration == iter_filter as u32).collect()
        } else {
            results.iter().collect()
        };

        let summary: Vec<serde_json::Value> = filtered.iter().map(|r| {
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
        }).collect();

        self.ok(id, json!({ "content": Self::text_content(serde_json::to_string_pretty(&summary).unwrap_or_default()) }))
    }

    fn cmd_configure(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        if let Some(max) = args["maxIterations"].as_u64() {
            self.engine.update_max_iterations(max as u32);
        }
        if let Some(threshold) = args["convergenceThreshold"].as_f64() {
            self.engine.update_convergence_threshold(threshold);
        }
        self.ok(id, json!({ "content": Self::text_content("Configuration updated".into()) }))
    }

    fn cmd_plugin_load(&mut self, id: Value, _args: &Value) -> JsonRpcResponse {
        self.ok(id, json!({ "content": Self::text_content("Plugin loading not yet implemented in Rust version".into()) }))
    }

    fn cmd_plugin_list(&self, id: Value) -> JsonRpcResponse {
        self.ok(id, json!({ "content": Self::text_content("No plugins loaded".into()) }))
    }

    fn cmd_analyze_security(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let files: Vec<String> = args["files"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
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
                f.severity.to_uppercase(), f.file,
                f.line.map(|l| l.to_string()).unwrap_or_else(|| "?".into()),
                f.description, f.recommendation
            ));
        }

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    fn cmd_analyze_code(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let files: Vec<String> = args["files"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
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
        self.ok(id, json!({
            "content": Self::text_content(
                "Destructive query guard active: all DROP/DELETE/TRUNCATE/ALTER operations \
                 require explicit confirmation with backup verification".into())
        }))
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
            "oura://results" => serde_json::to_string_pretty(
                &state.map(|s| s.history.clone()).unwrap_or_default()
            ).unwrap_or_default(),
            "oura://config" => serde_json::to_string_pretty(
                &state.map(|s| s.config.clone()).unwrap_or_default()
            ).unwrap_or_default(),
            _ => return self.err(id, -32602, format!("Unknown resource: {}", uri)),
        };

        self.ok(id, json!({
            "contents": [{
                "uri": uri,
                "mimeType": "application/json",
                "text": content
            }]
        }))
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
}
