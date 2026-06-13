use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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
                        eprintln!(
                            "[Oura] Warning: failed to parse config at {}: {}",
                            path.display(),
                            e
                        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.loop_engine.max_iterations, 20);
        assert_eq!(config.loop_engine.convergence_threshold, 90.0);
        assert!(!config.github.enabled);
        assert!(config.synapsis.enabled);
    }

    #[test]
    fn test_general_config_default() {
        let config = GeneralConfig::default();
        assert_eq!(config.profile, "default");
        assert!(config.data_dir.is_none());
    }

    #[test]
    fn test_loop_engine_config_default() {
        let config = LoopEngineConfig::default();
        assert_eq!(config.max_iterations, 20);
        assert_eq!(config.convergence_threshold, 90.0);
        assert_eq!(config.feedback_sources, vec!["test", "lint"]);
    }

    #[test]
    fn test_github_config_default() {
        let config = GitHubConfig::default();
        assert!(!config.enabled);
        assert!(!config.auto_commit);
        assert!(!config.auto_pr);
        assert_eq!(config.pr_title_prefix, "[Oura] ");
    }

    #[test]
    fn test_synapsis_config_default() {
        let config = SynapsisConfig::default();
        assert!(config.enabled);
        assert_eq!(config.endpoint, "http://localhost:7438");
    }

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, "info");
        assert_eq!(config.format, "text");
        assert_eq!(config.output, "stderr");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(!toml_str.is_empty());

        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            deserialized.loop_engine.max_iterations,
            config.loop_engine.max_iterations
        );
    }

    #[test]
    fn test_apply_env_overrides() {
        std::env::set_var("OURA_MAX_ITERATIONS", "50");
        std::env::set_var("OURA_CONVERGENCE_THRESHOLD", "95.0");

        let config = Config::default();
        let config = Config::apply_env_overrides(config);

        assert_eq!(config.loop_engine.max_iterations, 50);
        assert_eq!(config.loop_engine.convergence_threshold, 95.0);

        std::env::remove_var("OURA_MAX_ITERATIONS");
        std::env::remove_var("OURA_CONVERGENCE_THRESHOLD");
    }
}
