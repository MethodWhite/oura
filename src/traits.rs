use async_trait::async_trait;
use crate::types::FeedbackEntry;

#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, cmd: &str) -> Result<String, String>;
}

#[async_trait]
pub trait FeedbackCollector: Send + Sync {
    async fn collect(&self) -> Vec<FeedbackEntry>;
}

#[async_trait]
pub trait SecurityScanner: Send + Sync {
    async fn scan(&self, files: &[String]) -> Vec<crate::types::SecurityAuditEntry>;
}

#[async_trait]
pub trait CodeAnalyzer: Send + Sync {
    async fn analyze(&self, file: &str) -> (Vec<String>, Vec<String>);
}

#[async_trait]
pub trait IntegrityChecker: Send + Sync {
    async fn check(&self) -> Result<String, String>;
}

pub struct DefaultCommandRunner;

#[async_trait]
impl CommandRunner for DefaultCommandRunner {
    async fn run(&self, cmd: &str) -> Result<String, String> {
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .await
            .map_err(|e| format!("Failed to run command: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Ok(format!("{}\n{}", stdout, stderr))
    }
}

pub struct CompositeFeedbackCollector {
    collectors: Vec<Box<dyn FeedbackCollector>>,
}

impl CompositeFeedbackCollector {
    pub fn new() -> Self {
        Self {
            collectors: Vec::new(),
        }
    }

    pub fn add(&mut self, collector: Box<dyn FeedbackCollector>) {
        self.collectors.push(collector);
    }
}

impl Default for CompositeFeedbackCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FeedbackCollector for CompositeFeedbackCollector {
    async fn collect(&self) -> Vec<FeedbackEntry> {
        let mut entries = Vec::new();
        for collector in &self.collectors {
            entries.extend(collector.collect().await);
        }
        entries
    }
}
