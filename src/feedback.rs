use crate::traits::{CommandRunner, FeedbackCollector};
use crate::types::FeedbackEntry;
use async_trait::async_trait;

pub struct TestFeedbackCollector {
    command_runner: Box<dyn CommandRunner>,
    test_command: String,
}

impl TestFeedbackCollector {
    pub fn new(command_runner: Box<dyn CommandRunner>, test_command: String) -> Self {
        Self {
            command_runner,
            test_command,
        }
    }
}

#[async_trait]
impl FeedbackCollector for TestFeedbackCollector {
    async fn collect(&self) -> Vec<FeedbackEntry> {
        let mut entries = vec![];

        match self.command_runner.run(&self.test_command).await {
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
    clippy_command: String,
}

impl ClippyFeedbackCollector {
    pub fn new(command_runner: Box<dyn CommandRunner>, clippy_command: String) -> Self {
        Self {
            command_runner,
            clippy_command,
        }
    }
}

#[async_trait]
impl FeedbackCollector for ClippyFeedbackCollector {
    async fn collect(&self) -> Vec<FeedbackEntry> {
        let mut entries = vec![];

        match self.command_runner.run(&self.clippy_command).await {
            Ok(output) => {
                let warnings = output.matches("warning").count();
                let errors = output.matches("error").count();
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

pub struct ProfileFeedbackCollector;

#[async_trait]
impl FeedbackCollector for ProfileFeedbackCollector {
    async fn collect(&self) -> Vec<FeedbackEntry> {
        let mut entries = vec![];
        let cwd = std::env::current_dir().unwrap_or_default();
        let profile_result = crate::profile::ProjectProfile::detect(&cwd);

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

        let verify_result = crate::profile::verify_dependencies(&cwd);
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
        regex::Regex::new(&format!(r"(?m)^test result:.*?(\d+)\s+{}", label)).unwrap(),
        regex::Regex::new(&format!(r"(\d+)\s+{}\b", label)).unwrap(),
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
