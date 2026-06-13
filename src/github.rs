use std::process::Command;
use crate::config::{Config, RepoConfig};

#[derive(Debug, Clone)]
pub struct GitHubRepo {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub base_branch: String,
    pub local_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: String,
}

#[derive(Debug, Clone)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
    pub html_url: String,
    pub state: String,
}

pub struct GitHubClient {
    config: Config,
    token: String,
    client: reqwest::Client,
}

impl GitHubClient {
    pub fn new(config: &Config) -> Option<Self> {
        let token = config.github.token.clone()
            .or_else(|| std::env::var("GITHUB_TOKEN").ok())
            .or_else(|| std::env::var("GH_TOKEN").ok())?;

        if !config.github.enabled {
            return None;
        }

        Some(Self {
            config: config.clone(),
            token,
            client: reqwest::Client::new(),
        })
    }

    pub fn repos(&self) -> Vec<GitHubRepo> {
        let mut repos: Vec<GitHubRepo> = self.config.github.repos.iter().map(|r| GitHubRepo {
            owner: r.owner.clone(),
            repo: r.repo.clone(),
            branch: r.branch.clone(),
            base_branch: r.base_branch.clone(),
            local_path: None,
        }).collect();

        if !self.config.github.default_owner.is_empty() && !self.config.github.default_repo.is_empty() {
            let exists = repos.iter().any(|r|
                r.owner == self.config.github.default_owner && r.repo == self.config.github.default_repo);
            if !exists {
                repos.push(GitHubRepo {
                    owner: self.config.github.default_owner.clone(),
                    repo: self.config.github.default_repo.clone(),
                    branch: "main".into(),
                    base_branch: "main".into(),
                    local_path: None,
                });
            }
        }

        repos
    }

    pub async fn create_pr(&self, repo: &RepoConfig, title: &str, body: &str, head: &str, base: &str) -> anyhow::Result<PullRequest> {
        let url = format!("https://api.github.com/repos/{}/{}/pulls", repo.owner, repo.repo);
        let payload = serde_json::json!({
            "title": format!("{}{}", self.config.github.pr_title_prefix, title),
            "body": body,
            "head": head,
            "base": base,
        });

        let resp = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "oura")
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error: {}", text);
        }

        let data: serde_json::Value = resp.json().await?;
        Ok(PullRequest {
            number: data["number"].as_u64().unwrap_or(0),
            title: data["title"].as_str().unwrap_or("").into(),
            body: data["body"].as_str().unwrap_or("").into(),
            head: data["head"]["ref"].as_str().unwrap_or("").into(),
            base: data["base"]["ref"].as_str().unwrap_or("").into(),
            html_url: data["html_url"].as_str().unwrap_or("").into(),
            state: data["state"].as_str().unwrap_or("").into(),
        })
    }

    pub async fn list_workflow_runs(&self, owner: &str, repo: &str, branch: Option<&str>) -> anyhow::Result<Vec<WorkflowRun>> {
        let mut url = format!("https://api.github.com/repos/{}/{}/actions/runs", owner, repo);
        if let Some(b) = branch {
            url.push_str(&format!("?branch={}", b));
        }

        let resp = self.client.get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "oura")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let data: serde_json::Value = resp.json().await?;
        let runs = data["workflow_runs"].as_array()
            .map(|arr| arr.iter().map(|r| WorkflowRun {
                id: r["id"].as_u64().unwrap_or(0),
                name: r["name"].as_str().unwrap_or("").into(),
                status: r["status"].as_str().unwrap_or("").into(),
                conclusion: r["conclusion"].as_str().map(String::from),
                html_url: r["html_url"].as_str().unwrap_or("").into(),
            }).collect())
            .unwrap_or_default();

        Ok(runs)
    }

    pub async fn wait_for_workflow(&self, owner: &str, repo: &str, run_id: u64, poll_secs: u64, max_polls: u32) -> anyhow::Result<Option<String>> {
        let url = format!("https://api.github.com/repos/{}/{}/actions/runs/{}", owner, repo, run_id);

        for i in 0..max_polls {
            tokio::time::sleep(tokio::time::Duration::from_secs(poll_secs)).await;

            let resp = self.client.get(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("User-Agent", "oura")
                .send()
                .await?;

            if !resp.status().is_success() {
                continue;
            }

            let data: serde_json::Value = resp.json().await?;
            let status = data["status"].as_str().unwrap_or("unknown");
            let conclusion = data["conclusion"].as_str();

            if status == "completed" {
                return Ok(conclusion.map(String::from));
            }

            eprintln!("[Oura:GitHub] Workflow {}/{} run {}: {}/{} (poll {}/{})",
                owner, repo, run_id, status, conclusion.unwrap_or("in_progress"), i + 1, max_polls);
        }

        anyhow::bail!("Workflow did not complete within {} polls", max_polls)
    }

    pub async fn dispatch_workflow(&self, owner: &str, repo: &str, workflow: &str, r#ref: &str, inputs: serde_json::Value) -> anyhow::Result<()> {
        let url = format!("https://api.github.com/repos/{}/{}/actions/workflows/{}/dispatches", owner, repo, workflow);

        let payload = serde_json::json!({
            "ref": r#ref,
            "inputs": inputs,
        });

        let resp = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "oura")
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Workflow dispatch failed: {}", text);
        }

        Ok(())
    }

    pub async fn auto_commit_and_push(&self, repo: &RepoConfig, message: &str, files: &[String]) -> anyhow::Result<()> {
        let work_dir = std::env::temp_dir().join(format!("oura-{}-{}", repo.owner, repo.repo));

        // Clone if needed
        if !work_dir.join(".git").exists() {
            let clone_url = format!("https://x-access-token:{}@github.com/{}/{}.git", self.token, repo.owner, repo.repo);
            let status = Command::new("git")
                .args(["clone", "--depth=1", &clone_url, work_dir.to_str().unwrap()])
                .status()?;
            if !status.success() {
                anyhow::bail!("Failed to clone repo");
            }
        }

        // Copy files
        for file in files {
            let src = std::path::Path::new(file);
            let dest = work_dir.join(src);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(src, &dest)?;
        }

        // Commit and push
        let output = Command::new("git")
            .args(["-C", work_dir.to_str().unwrap(), "add", "."])
            .output()?;
        if !output.status.success() {
            anyhow::bail!("git add failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let output = Command::new("git")
            .args(["-C", work_dir.to_str().unwrap(), "commit", "-m", message])
            .output()?;

        // If there's nothing to commit, that's fine
        if output.status.success() || String::from_utf8_lossy(&output.stderr).contains("nothing to commit") {
            let output = Command::new("git")
                .args(["-C", work_dir.to_str().unwrap(), "push", "origin", &repo.branch])
                .output()?;
            if !output.status.success() {
                anyhow::bail!("git push failed: {}", String::from_utf8_lossy(&output.stderr));
            }
        }

        Ok(())
    }
}
