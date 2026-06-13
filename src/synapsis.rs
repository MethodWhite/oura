use anyhow::Result;

pub struct SynapsisBridge {
    endpoint: String,
    enabled: bool,
}

impl SynapsisBridge {
    pub fn new(enabled: bool) -> Self {
        Self {
            endpoint: std::env::var("SYNAPSIS_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:7438".into()),
            enabled,
        }
    }

    pub async fn save_memory(&self, title: &str, content: &str, project: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "title": title,
            "content": content,
            "project": project,
        });

        let _ = client
            .post(&format!("{}/memory/save", self.endpoint))
            .json(&payload)
            .send()
            .await;

        Ok(())
    }

    pub async fn health(&self) -> bool {
        if !self.enabled {
            return false;
        }

        reqwest::get(&self.endpoint).await.is_ok()
    }
}
