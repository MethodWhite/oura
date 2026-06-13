use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub loop_engine: LoopEngineConfig,
    #[serde(default)]
    pub github: GitHubConfig,
    #[serde(default)]
    pub synapsis: SynapsisConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub data_dir: Option<String>,
    pub profile: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            profile: "default".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopEngineConfig {
    pub max_iterations: u32,
    pub convergence_threshold: f64,
    pub feedback_sources: Vec<String>,
    pub working_directory: Option<String>,
}

impl Default for LoopEngineConfig {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            convergence_threshold: 90.0,
            feedback_sources: vec!["test".into(), "lint".into()],
            working_directory: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub enabled: bool,
    pub default_owner: String,
    pub default_repo: String,
    pub token: Option<String>,
    pub auto_commit: bool,
    pub auto_pr: bool,
    pub pr_title_prefix: String,
    pub workflows_enabled: bool,
    pub repos: Vec<RepoConfig>,
}

impl Default for GitHubConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_owner: String::new(),
            default_repo: String::new(),
            token: None,
            auto_commit: false,
            auto_pr: false,
            pr_title_prefix: "[Oura] ".into(),
            workflows_enabled: true,
            repos: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub base_branch: String,
    pub auto_sync: bool,
    pub workflows: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapsisConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub mcp_command: String,
}

impl Default for SynapsisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: "http://localhost:7438".into(),
            mcp_command: "synapsis-mcp".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub output: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            format: "text".into(),
            output: "stderr".into(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_paths = vec![
            std::env::var("OURA_CONFIG").ok().map(PathBuf::from),
            dirs::config_dir().map(|p| p.join("oura").join("config.toml")),
            dirs::home_dir().map(|p| p.join(".oura").join("config.toml")),
        ];

        for path in config_paths.into_iter().flatten() {
            if path.exists() {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                match toml::from_str(&content) {
                    Ok(config) => {
                        if std::env::var("OURA_QUIET").is_err() && std::env::var("QUIET").is_err() {
                            eprintln!("[Oura] Loaded config from: {}", path.display());
                        }
                        return Self::apply_env_overrides(config);
                    }
                    Err(e) => {
                        eprintln!("[Oura] Warning: failed to parse config at {}: {}", path.display(), e);
                    }
                }
            }
        }

        eprintln!("[Oura] Using default configuration");
        Self::apply_env_overrides(Config::default())
    }

    fn apply_env_overrides(mut config: Self) -> Self {
        if let Ok(val) = std::env::var("OURA_MAX_ITERATIONS") {
            if let Ok(n) = val.parse() {
                config.loop_engine.max_iterations = n;
            }
        }
        if let Ok(val) = std::env::var("OURA_CONVERGENCE_THRESHOLD") {
            if let Ok(n) = val.parse() {
                config.loop_engine.convergence_threshold = n;
            }
        }
        if let Ok(val) = std::env::var("OURA_GITHUB_TOKEN") {
            config.github.token = Some(val);
        }
        if let Ok(val) = std::env::var("OURA_GITHUB_OWNER") {
            config.github.default_owner = val;
        }
        if let Ok(val) = std::env::var("OURA_GITHUB_REPO") {
            config.github.default_repo = val;
        }
        if let Ok(val) = std::env::var("OURA_GITHUB_ENABLED") {
            config.github.enabled = val == "true" || val == "1";
        }
        if let Ok(val) = std::env::var("OURA_SYNAPSIS_ENDPOINT") {
            config.synapsis.endpoint = val;
        }
        if let Ok(val) = std::env::var("OURA_WORKING_DIR") {
            config.loop_engine.working_directory = Some(val);
        }
        config
    }

    pub fn save_default(path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config)?;
        std::fs::write(path, toml_str)?;
        eprintln!("[Oura] Default config written to: {}", path.display());
        Ok(())
    }

    pub fn init() -> anyhow::Result<()> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("oura");
        let config_path = config_dir.join("config.toml");

        if !config_path.exists() {
            Self::save_default(&config_path)?;
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            loop_engine: LoopEngineConfig::default(),
            github: GitHubConfig::default(),
            synapsis: SynapsisConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}
