use crate::mcp::McpServer;
use crate::types::JsonRpcResponse;
use serde_json::{json, Value};

impl McpServer {
    pub(super) fn handle_resources_list(&self, id: Value) -> JsonRpcResponse {
        self.ok(id, json!({
            "resources": [
                { "uri": "oura://state", "name": "Loop State", "mimeType": "application/json" },
                { "uri": "oura://results", "name": "Loop Results", "mimeType": "application/json" },
                { "uri": "oura://config", "name": "Oura Config", "mimeType": "application/json" },
            ]
        }))
    }

    pub(super) fn handle_resource_read(&self, id: Value, params: Option<&Value>) -> JsonRpcResponse {
        let uri = params.and_then(|p| p["uri"].as_str()).unwrap_or("");

        let content = match uri {
            "oura://state" => {
                match self.engine.get_state() {
                    Ok(Some(state)) => serde_json::to_string_pretty(&state).unwrap_or_else(|e| format!("Serialization error: {}", e)),
                    Ok(None) => "No active loop".into(),
                    Err(e) => return self.err(id, e.code(), format!("Failed to get state: {}", e)),
                }
            }
            "oura://results" => {
                let results = match self.engine.get_results() {
                    Ok(r) => r,
                    Err(e) => return self.err(id, e.code(), format!("Failed to get results: {}", e)),
                };
                serde_json::to_string_pretty(&results).unwrap_or_else(|e| format!("Serialization error: {}", e))
            }
            "oura://config" => {
                let max_iter = self.engine.max_iterations().unwrap_or(20);
                let threshold = self.engine.convergence_threshold().unwrap_or(90.0);
                let cwd = std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                let config_json = json!({
                    "max_iterations": max_iter, "convergence_threshold": threshold, "working_directory": cwd,
                });
                serde_json::to_string_pretty(&config_json).unwrap_or_else(|e| format!("Serialization error: {}", e))
            }
            _ => return self.err(id, -32602, format!("Unknown resource: {}", uri)),
        };

        self.ok(id, json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": content }] }))
    }

    pub(super) fn handle_prompts_list(&self, id: Value) -> JsonRpcResponse {
        self.ok(id, json!({
            "prompts": [
                { "name": "start_loop", "description": "Template for starting a new Oura iteration loop", "arguments": [{ "name": "goal", "description": "What to achieve", "required": true }] },
                { "name": "loop_summary", "description": "Template for summarizing loop results", "arguments": [{ "name": "iteration", "description": "Which iteration to summarize", "required": false }] },
            ]
        }))
    }

    pub(super) fn handle_prompt_get(&self, id: Value, params: Option<&Value>) -> JsonRpcResponse {
        let name = params.and_then(|p| p["name"].as_str()).unwrap_or("");
        match name {
            "start_loop" => {
                let goal = params.and_then(|p| p["arguments"].as_object())
                    .and_then(|a| a.get("goal")).and_then(|v| v.as_str()).unwrap_or("improve codebase");
                self.ok(id, json!({
                    "messages": [{
                        "role": "user", "content": { "type": "text", "text": format!(
                            "I want to start an Oura iteration loop to: {}\n\nPlease call oura_start_loop with goal=\"{}\".\nOura will run iterations until convergence or max iterations reached.\nYou can check progress with oura_loop_status and finally oura_results.", goal, goal
                        )}
                    }]
                }))
            }
            "loop_summary" => {
                self.ok(id, json!({
                    "messages": [{ "role": "user", "content": { "type": "text", "text": "Please summarize the Oura loop results. Use oura_results to get the data first." } }]
                }))
            }
            _ => self.err(id, -32602, format!("Unknown prompt: {}", name)),
        }
    }
}
