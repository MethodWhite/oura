use crate::types::FeedbackEntry;
use async_trait::async_trait;
use std::process::Stdio;

#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, program: &str, args: &[&str]) -> Result<String, String>;
    fn working_dir(&self) -> Option<&std::path::Path> { None }
}

#[async_trait]
pub trait FeedbackCollector: Send + Sync {
    async fn collect(&self) -> Vec<FeedbackEntry>;
}

pub struct DefaultCommandRunner {
    working_dir: Option<std::path::PathBuf>,
}

impl DefaultCommandRunner {
    pub fn new() -> Self {
        Self { working_dir: None }
    }
    pub fn new_with_dir(dir: std::path::PathBuf) -> Self {
        Self { working_dir: Some(dir) }
    }
}

impl Default for DefaultCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandRunner for DefaultCommandRunner {
    async fn run(&self, program: &str, args: &[&str]) -> Result<String, String> {
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }
        let output = cmd.output().await
            .map_err(|e| format!("Failed to run command: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Ok(format!("{}\n{}", stdout, stderr))
    }

    fn working_dir(&self) -> Option<&std::path::Path> {
        self.working_dir.as_deref()
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
