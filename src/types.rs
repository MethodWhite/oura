use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OuraConfig {
    pub max_iterations: u32,
    pub convergence_threshold: f64,
    pub feedback_sources: Vec<FeedbackSource>,
    pub sync_to_synapsis: bool,
    pub working_directory: String,
}

impl Default for OuraConfig {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            convergence_threshold: 90.0,
            feedback_sources: vec![
                FeedbackSource {
                    type_: "test".into(),
                    command: Some("cargo test".into()),
                    enabled: true,
                },
                FeedbackSource {
                    type_: "lint".into(),
                    command: Some("cargo clippy".into()),
                    enabled: true,
                },
                FeedbackSource {
                    type_: "typecheck".into(),
                    command: None,
                    enabled: false,
                },
            ],
            sync_to_synapsis: true,
            working_directory: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackSource {
    #[serde(rename = "type")]
    pub type_: String,
    pub command: Option<String>,
    pub enabled: bool,
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
    pub input_schema: serde_json::Value,
}
