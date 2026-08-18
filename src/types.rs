use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OuraConfig {
    pub max_iterations: u32,
    pub convergence_threshold: f64,
    pub max_runtime_secs: u64,
    pub working_directory: Option<String>,
}

impl Default for OuraConfig {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            convergence_threshold: 90.0,
            max_runtime_secs: 3600,
            working_directory: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationResult {
    pub iteration: u32,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub actions: Vec<ActionLog>,
    pub feedback: Vec<FeedbackEntry>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionLog {
    pub id: String,
    pub agent: String,
    pub type_: String,
    pub description: String,
    pub target: String,
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackEntry {
    pub source: String,
    pub type_: String,
    pub message: String,
    pub details: Option<String>,
    pub metric: Option<f64>,
    pub threshold: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    pub id: String,
    pub goal: String,
    pub config: OuraConfig,
    pub current_iteration: u32,
    pub history: Vec<IterationResult>,
    pub status: String,
    pub start_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditEntry {
    pub type_: String,
    pub severity: String,
    pub file: String,
    pub line: Option<usize>,
    pub description: String,
    pub recommendation: String,
}

// JSON-RPC types for MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default = "default_id")]
    pub id: serde_json::Value,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

fn default_id() -> serde_json::Value {
    serde_json::Value::Null
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_serialization() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::Value::Number(1.into()),
            method: "tools/list".into(),
            params: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"tools/list\""));
    }

    #[test]
    fn test_json_rpc_response_error() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: serde_json::Value::Null,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "Method not found".into(),
                data: None,
            }),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"code\":-32601"));
        assert!(json.contains("\"message\":\"Method not found\""));
    }

    #[test]
    fn test_json_rpc_response_success() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: serde_json::Value::Number(1.into()),
            result: Some(serde_json::json!({"tools": []})),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_mcp_tool_definition() {
        let tool = McpToolDefinition {
            name: "test_tool".into(),
            description: "A test tool".into(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        };
        assert_eq!(tool.name, "test_tool");
    }

    #[test]
    fn test_iteration_result_defaults() {
        let result = IterationResult {
            iteration: 1,
            status: "running".into(),
            started_at: "2024-01-01".into(),
            completed_at: None,
            actions: vec![],
            feedback: vec![],
            score: 100.0,
        };
        assert_eq!(result.iteration, 1);
        assert!(result.actions.is_empty());
    }
}
