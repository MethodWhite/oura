use crate::config::ConnectorConfig;
use crate::traits::{CommandRunner, FeedbackCollector};
use crate::types::FeedbackEntry;
use async_trait::async_trait;

pub struct TestFeedbackCollector {
    command_runner: Box<dyn CommandRunner>,
}

impl TestFeedbackCollector {
    pub fn new(command_runner: Box<dyn CommandRunner>) -> Self {
        Self {
            command_runner,
        }
    }
}

#[async_trait]
impl FeedbackCollector for TestFeedbackCollector {
    async fn collect(&self) -> Vec<FeedbackEntry> {
        let mut entries = vec![];

        match self.command_runner.run("cargo", &["test", "--workspace"]).await {
            Ok(output) => {
                let passed = extract_number(&output, "passed");
                let failed = extract_number(&output, "failed");

                entries.push(FeedbackEntry {
                    source: "tests".into(),
                    type_: if failed > 0 {
                        "error".into()
                    } else {
                        "success".into()
                    },
                    message: if failed > 0 {
                        format!("{} tests failed, {} passed", failed, passed)
                    } else {
                        format!("All {} tests passed", passed)
                    },
                    details: if failed > 0 { Some(output) } else { None },
                    metric: Some(if passed + failed > 0 {
                        (passed as f64 / (passed + failed) as f64) * 100.0
                    } else {
                        0.0
                    }),
                    threshold: Some(100.0),
                });
            }
            Err(e) => {
                entries.push(FeedbackEntry {
                    source: "tests".into(),
                    type_: "error".into(),
                    message: format!("Failed to run tests: {}", e),
                    details: None,
                    metric: Some(0.0),
                    threshold: Some(100.0),
                });
            }
        }

        entries
    }
}

pub struct ClippyFeedbackCollector {
    command_runner: Box<dyn CommandRunner>,
}

impl ClippyFeedbackCollector {
    pub fn new(command_runner: Box<dyn CommandRunner>) -> Self {
        Self {
            command_runner,
        }
    }
}

#[async_trait]
impl FeedbackCollector for ClippyFeedbackCollector {
    async fn collect(&self) -> Vec<FeedbackEntry> {
        let mut entries = vec![];

        match self.command_runner.run("cargo", &["clippy", "--workspace"]).await {
            Ok(output) => {
                let warnings = output.lines().filter(|l| l.starts_with("warning:") || l.contains("warning: ")).count();
                let errors = output.lines().filter(|l| l.starts_with("error:") || l.contains("error: ")).count();
                if warnings > 0 || errors > 0 {
                    entries.push(FeedbackEntry {
                        source: "clippy".into(),
                        type_: if errors > 0 {
                            "error".into()
                        } else {
                            "warning".into()
                        },
                        message: format!("Clippy: {} warnings, {} errors", warnings, errors),
                        details: Some(output),
                        metric: Some(if errors > 0 { 0.0 } else { 100.0 }),
                        threshold: Some(100.0),
                    });
                }
            }
            Err(e) => {
                entries.push(FeedbackEntry {
                    source: "clippy".into(),
                    type_: "error".into(),
                    message: format!("Failed to run clippy: {}", e),
                    details: None,
                    metric: Some(0.0),
                    threshold: Some(100.0),
                });
            }
        }

        entries
    }
}

pub struct ProfileFeedbackCollector {
    working_dir: Option<std::path::PathBuf>,
}

impl ProfileFeedbackCollector {
    pub fn new() -> Self {
        Self { working_dir: None }
    }
    pub fn new_with_dir(dir: std::path::PathBuf) -> Self {
        Self { working_dir: Some(dir) }
    }
}

impl Default for ProfileFeedbackCollector {
    fn default() -> Self {
        Self::new()
    }
}

static PROFILE_CACHE: std::sync::OnceLock<std::sync::Mutex<(String, crate::profile::ProjectProfile, crate::profile::VerifyReport)>> = std::sync::OnceLock::new();

fn get_cached_profile(cwd: &std::path::Path) -> (crate::profile::ProjectProfile, crate::profile::VerifyReport) {
    let cache = PROFILE_CACHE.get_or_init(|| std::sync::Mutex::new((String::new(), crate::profile::ProjectProfile::detect(cwd), crate::profile::verify_dependencies(cwd))));
    if let Ok(mut guard) = cache.lock() {
        let cwd_str = cwd.to_string_lossy().to_string();
        if guard.0 != cwd_str {
            guard.0 = cwd_str;
            guard.1 = crate::profile::ProjectProfile::detect(cwd);
            guard.2 = crate::profile::verify_dependencies(cwd);
        }
        return (guard.1.clone(), guard.2.clone());
    }
    (crate::profile::ProjectProfile::detect(cwd), crate::profile::verify_dependencies(cwd))
}

#[async_trait]
impl FeedbackCollector for ProfileFeedbackCollector {
    async fn collect(&self) -> Vec<FeedbackEntry> {
        let mut entries = vec![];
        let cwd = self.working_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let (profile_result, verify_result) = tokio::task::spawn_blocking({
            let cwd = cwd.clone();
            move || get_cached_profile(&cwd)
        }).await.unwrap_or_else(|_| (crate::profile::ProjectProfile::detect(&cwd), crate::profile::verify_dependencies(&cwd)));

        let dep_type = if profile_result.dependency_count > 50 {
            "error"
        } else if profile_result.dependency_count > 20 {
            "warning"
        } else {
            "info"
        };

        entries.push(FeedbackEntry {
            source: "profile".into(),
            type_: dep_type.into(),
            message: format!(
                "Project: {} | {} deps | {} engine:{}",
                profile_result.user_type,
                profile_result.dependency_count,
                profile_result.ecosystem,
                if profile_result.has_game_engine {
                    "yes"
                } else {
                    "no"
                },
            ),
            details: Some(profile_result.summary()),
            metric: Some(profile_result.confidence * 100.0),
            threshold: None,
        });

        if !verify_result.license_issues.is_empty() {
            entries.push(FeedbackEntry {
                source: "license".into(),
                type_: "warning".into(),
                message: format!(
                    "License issues: {} restricted deps",
                    verify_result.license_issues.len()
                ),
                details: Some(verify_result.license_issues.join("\n")),
                metric: Some(100.0 - (verify_result.license_issues.len() as f64 * 10.0).min(100.0)),
                threshold: Some(100.0),
            });
        }
        if !verify_result.version_issues.is_empty() {
            entries.push(FeedbackEntry {
                source: "versioning".into(),
                type_: "warning".into(),
                message: format!(
                    "Version issues: {} unstable deps",
                    verify_result.version_issues.len()
                ),
                details: Some(verify_result.version_issues.join("\n")),
                metric: Some(100.0 - (verify_result.version_issues.len() as f64 * 5.0).min(100.0)),
                threshold: Some(100.0),
            });
        }

        entries
    }
}

fn make_extract_regex(label: &str) -> (regex::Regex, regex::Regex) {
    (
        regex::Regex::new(&format!(r"(?m)^test result:.*?(\d+)\s+{}", label))
            .expect("Static regex pattern is always valid"),
        regex::Regex::new(&format!(r"(\d+)\s+{}\b", label))
            .expect("Static regex pattern is always valid"),
    )
}

fn extract_number(output: &str, label: &str) -> u32 {
    let (re1, re2) = match label {
        "passed" => {
            static RE: std::sync::OnceLock<(regex::Regex, regex::Regex)> =
                std::sync::OnceLock::new();
            RE.get_or_init(|| make_extract_regex("passed"))
        }
        "failed" => {
            static RE: std::sync::OnceLock<(regex::Regex, regex::Regex)> =
                std::sync::OnceLock::new();
            RE.get_or_init(|| make_extract_regex("failed"))
        }
        _ => return 0,
    };
    if let Some(caps) = re1.captures(output) {
        if let Some(m) = caps.get(1) {
            if let Ok(n) = m.as_str().parse() {
                return n;
            }
        }
    }
    if let Some(caps) = re2.captures(output) {
        if let Some(m) = caps.get(1) {
            if let Ok(n) = m.as_str().parse() {
                return n;
            }
        }
    }
    0
}

pub async fn call_mcp_tool(server_url: &str, endpoint: &str, tool_name: &str, arguments: &serde_json::Value) -> Result<String, String> {
    let url = format!("{}{}", server_url.trim_end_matches('/'), endpoint);
    let request_body = serde_json::json!({
        "jsonrpc": "2.0", "id": "1", "method": "tools/call",
        "params": { "name": tool_name, "arguments": arguments }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client build error: {}", e))?;

    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    resp.text().await.map_err(|e| format!("HTTP read error: {}", e))
}

pub struct ConnectorFeedbackCollector {
    config: ConnectorConfig,
}

impl ConnectorFeedbackCollector {
    pub fn new(config: ConnectorConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl FeedbackCollector for ConnectorFeedbackCollector {
    async fn collect(&self) -> Vec<FeedbackEntry> {
        if !self.config.enabled || self.config.tools.is_empty() {
            return vec![];
        }

        let mut entries = vec![];
        let empty_args = serde_json::json!({});

        for tool_name in &self.config.tools {
            let result = if self.config.transport == "quic" {
                match crate::connector::QuicConnector::new() {
                    Ok(conn) => conn.call_tool(
                        &self.config.host,
                        self.config.port,
                        tool_name,
                        &empty_args,
                    ).await.map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                }
            } else {
                call_mcp_tool(&self.config.server_url, &self.config.endpoint, tool_name, &empty_args).await
            };

            match result {
                Ok(response) => {
                    let preview: String = response.chars().take(300).collect();
                    entries.push(FeedbackEntry {
                        source: format!("connector:{}", tool_name),
                        type_: "info".into(),
                        message: format!("Connector {} returned ({} chars)", tool_name, response.len()),
                        details: Some(preview),
                        metric: None,
                        threshold: None,
                    });
                }
                Err(e) => {
                    entries.push(FeedbackEntry {
                        source: format!("connector:{}", tool_name),
                        type_: "error".into(),
                        message: format!("Connector {} failed: {}", tool_name, e),
                        details: None,
                        metric: Some(0.0),
                        threshold: None,
                    });
                }
            }
        }
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_number_passed() {
        let output = "test result: ok. 5 passed; 0 failed; 0 ignored";
        assert_eq!(extract_number(output, "passed"), 5);
    }

    #[test]
    fn test_extract_number_failed() {
        let output = "test result: FAILED. 3 passed; 2 failed; 0 ignored";
        assert_eq!(extract_number(output, "failed"), 2);
    }

    #[test]
    fn test_extract_number_unknown_label() {
        let output = "test result: ok. 5 passed; 0 failed";
        assert_eq!(extract_number(output, "unknown"), 0);
    }
}
