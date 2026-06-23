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
    pub logging: LoggingConfig,
    #[serde(default)]
    pub connector: ConnectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralConfig {
    #[serde(default)]
    pub data_dir: Option<String>,
    #[serde(default = "default_profile")]
    pub profile: String,
}

fn default_profile() -> String {
    "default".into()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            profile: "default".into(),
        }
    }
}

fn default_threshold() -> f64 { 90.0 }
fn default_feedback_sources() -> Vec<String> { vec!["test".into(), "lint".into()] }
fn default_max_runtime() -> u64 { 3600 }
fn default_max_iterations() -> u32 { 20 }
fn default_pr_prefix() -> String { "[Oura] ".into() }
fn default_logging_level() -> String { "info".into() }
fn default_logging_format() -> String { "text".into() }
fn default_logging_output() -> String { "stderr".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopEngineConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default = "default_threshold")]
    pub convergence_threshold: f64,
    #[serde(default = "default_feedback_sources")]
    pub feedback_sources: Vec<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default = "default_max_runtime")]
    pub max_runtime_secs: u64,
}

impl Default for LoopEngineConfig {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            convergence_threshold: 90.0,
            feedback_sources: vec!["test".into(), "lint".into()],
            working_directory: None,
            max_runtime_secs: 3600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub default_owner: String,
    #[serde(default)]
    pub default_repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default)]
    pub auto_commit: bool,
    #[serde(default)]
    pub auto_pr: bool,
    #[serde(default = "default_pr_prefix")]
    pub pr_title_prefix: String,
    #[serde(default)]
    pub workflows_enabled: bool,
}

#[allow(dead_code)]
impl GitHubConfig {
    pub fn masked_token(&self) -> Option<String> {
        self.token.as_ref().map(|t| {
            if t.len() > 8 {
                format!("{}…{}", &t[..4], &t[t.len()-4..])
            } else {
                "********".into()
            }
        })
    }
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    #[serde(default = "default_logging_level")]
    pub level: String,
    #[serde(default = "default_logging_format")]
    pub format: String,
    #[serde(default = "default_logging_output")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub server_url: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub auto_call: bool,
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            transport: "http".into(),
            server_url: "http://localhost:7438".into(),
            host: "127.0.0.1".into(),
            port: 7439,
            endpoint: "/message".into(),
            tools: vec![],
            auto_call: false,
        }
    }
}

fn quiet_eprint(msg: &str) {
    if std::env::var("OURA_QUIET").is_err() {
        eprintln!("{}", msg);
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
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        quiet_eprint(&format!("[Oura] Warning: couldn't read config at {}: {}", path.display(), e));
                        quiet_eprint("[Oura] Using default configuration");
                        return Self::apply_env_overrides(Config::default());
                    }
                };
                match toml::from_str(&content) {
                    Ok(config) => {
                        quiet_eprint(&format!("[Oura] Loaded config from: {}", path.display()));
                        return Self::apply_env_overrides(config);
                    }
                    Err(e) => {
                        quiet_eprint(&format!(
                            "[Oura] Warning: failed to parse config at {}: {}",
                            path.display(),
                            e
                        ));
                    }
                }
            }
        }

        quiet_eprint("[Oura] Using default configuration");
        Self::apply_env_overrides(Config::default())
    }

    fn apply_env_overrides(mut config: Self) -> Self {
        if let Ok(val) = std::env::var("OURA_MAX_ITERATIONS") {
            if let Ok(n) = val.parse() {
                config.loop_engine.max_iterations = n;
            }
        }
        if let Ok(val) = std::env::var("OURA_CONVERGENCE_THRESHOLD") {
            if let Ok(n) = val.parse::<f64>() {
                if n.is_finite() && (0.0..=100.0).contains(&n) {
                    config.loop_engine.convergence_threshold = n;
                }
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
        quiet_eprint(&format!("[Oura] Default config written to: {}", path.display()));
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
        assert_eq!(config.max_runtime_secs, 3600);
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
    fn test_config_partial_section() {
        let toml_str = "[loop_engine]\nmax_iterations = 5\n".to_string();
        let config: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.loop_engine.max_iterations, 5);
        assert_eq!(config.loop_engine.convergence_threshold, 90.0);
        assert_eq!(config.loop_engine.max_runtime_secs, 3600);
    }

    #[test]
    fn test_config_unknown_key_rejected() {
        let toml_str = "[loop_engine]\nunknown_key = true\nmax_iterations = 5\n".to_string();
        let result: Result<Config, _> = toml::from_str(&toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_convergence_threshold_env_nan_rejected() {
        std::env::set_var("OURA_CONVERGENCE_THRESHOLD", "nan");
        let config = Config::apply_env_overrides(Config::default());
        // nan should be rejected, default remains
        assert_eq!(config.loop_engine.convergence_threshold, 90.0);
        std::env::remove_var("OURA_CONVERGENCE_THRESHOLD");
    }

    #[test]
    fn test_convergence_threshold_env_out_of_range_rejected() {
        std::env::set_var("OURA_CONVERGENCE_THRESHOLD", "200");
        let config = Config::apply_env_overrides(Config::default());
        assert_eq!(config.loop_engine.convergence_threshold, 90.0);
        std::env::remove_var("OURA_CONVERGENCE_THRESHOLD");
    }

    #[test]
    fn test_convergence_threshold_env_valid() {
        std::env::set_var("OURA_CONVERGENCE_THRESHOLD", "85.5");
        let config = Config::apply_env_overrides(Config::default());
        assert_eq!(config.loop_engine.convergence_threshold, 85.5);
        std::env::remove_var("OURA_CONVERGENCE_THRESHOLD");
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
