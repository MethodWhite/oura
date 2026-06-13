use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub type ProfileType = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProfile {
    pub user_type: ProfileType,
    pub confidence: f64,
    pub indicators: Vec<String>,
    pub ecosystem: String,
    pub has_game_engine: bool,
    pub has_paid_tools: bool,
    pub has_enterprise_configs: bool,
    pub dependency_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub is_outdated: bool,
    pub latest_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub dependencies: Vec<DependencyInfo>,
    pub license_issues: Vec<String>,
    pub version_issues: Vec<String>,
    pub profile: ProjectProfile,
}

impl ProjectProfile {
    pub fn detect(root: &Path) -> Self {
        let mut indicators: Vec<String> = Vec::new();
        let mut has_game_engine = false;
        let mut has_paid_tools = false;
        let mut has_enterprise_configs = false;
        let mut ecosystem = "unknown".to_string();
        let mut dep_count = 0;

        if root.join("Cargo.toml").exists() {
            ecosystem = "rust".to_string();
            dep_count = count_cargo_deps(root);
            if let Ok(content) = std::fs::read_to_string(root.join("Cargo.toml")) {
                if content.contains("unreal")
                    || content.contains("godot")
                    || content.contains("bevy")
                    || content.contains("amethyst")
                {
                    has_game_engine = true;
                    indicators.push("game-engine: rust".to_string());
                }
            }
        }
        if root.join("package.json").exists() {
            ecosystem = if ecosystem == "unknown" {
                "node".to_string()
            } else {
                format!("{}+node", ecosystem)
            };
            dep_count += count_json_deps(root, "package.json");
            if let Ok(content) = std::fs::read_to_string(root.join("package.json")) {
                if content.contains("\"three\"")
                    || content.contains("\"babylon\"")
                    || content.contains("\"phaser\"")
                    || content.contains("\"pixi.js\"")
                {
                    has_game_engine = true;
                    indicators.push("game-engine: web".to_string());
                }
            }
        }
        if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
            ecosystem = if ecosystem == "unknown" {
                "python".to_string()
            } else {
                format!("{}+python", ecosystem)
            };
        }
        if root.join("CMakeLists.txt").exists() {
            ecosystem = if ecosystem == "unknown" {
                "cpp".to_string()
            } else {
                format!("{}+cpp", ecosystem)
            };
        }
        if root.join("go.mod").exists() {
            ecosystem = if ecosystem == "unknown" {
                "go".to_string()
            } else {
                format!("{}+go", ecosystem)
            };
        }

        let enterprise_patterns = [
            "saml",
            "ldap",
            "okta",
            "keycloak",
            "oauth2",
            "enterprise",
            "single-sign-on",
            "sso",
            "active-directory",
            "adfs",
            "compliance",
            "gdpr",
            "hipaa",
            "sox",
            "pci-dss",
            "audit-log",
            "rbac",
            "abac",
            "tenant",
            "multi-tenant",
        ];
        let paid_tool_patterns = [
            "unreal",
            "unity",
            "UnrealEngine",
            "UnityEngine",
            "photosh",
            "adobe",
            "maya",
            "blender",
            " Substance ",
            "houdini",
            "nuke",
            "fusion360",
            "autocad",
            "solidworks",
            "matlab",
            "simulink",
            "ansys",
            "comsol",
            "datadog",
            "newrelic",
            "sentry",
            "databricks",
            "snowflake",
        ];
        let studio_patterns = [
            "gamestudio",
            "game-studio",
            "entertainment",
            "interactive",
            "arts",
            "creative",
            "media",
            "production",
        ];

        let config_files = [
            ".saml",
            "SAML",
            "ldap",
            "keycloak.json",
            "okta.json",
            "enterprise.json",
            "enterprise.yml",
            "corporate.json",
            "license.key",
            "LICENSE.key",
            "unity.alf",
            "UnrealLicense",
        ];

        for file in &config_files {
            if root.join(file).exists() {
                has_enterprise_configs = true;
                indicators.push(format!("enterprise-config: {}", file));
            }
        }

        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                for pat in &enterprise_patterns {
                    if name.contains(pat) {
                        has_enterprise_configs = true;
                        indicators.push(format!("enterprise-pattern: {}", pat));
                        break;
                    }
                }
                for pat in &paid_tool_patterns {
                    if name.contains(&pat.to_lowercase()) {
                        has_paid_tools = true;
                        indicators.push(format!("paid-tool: {}", pat));
                        break;
                    }
                }
                for pat in &studio_patterns {
                    if name.contains(pat) {
                        indicators.push(format!("studio-pattern: {}", pat));
                        break;
                    }
                }
            }
        }

        if let Ok(content) = read_first_n_chars(root.join("README.md"), 2048)
            .or_else(|_| read_first_n_chars(root.join("README"), 2048))
        {
            let lower = content.to_lowercase();
            for pat in &enterprise_patterns {
                if lower.contains(pat) {
                    has_enterprise_configs = true;
                    indicators.push(format!("readme-enterprise: {}", pat));
                    break;
                }
            }
            for pat in &studio_patterns {
                if lower.contains(pat) {
                    indicators.push(format!("readme-studio: {}", pat));
                    break;
                }
            }
        }

        let user_type = classify(has_enterprise_configs, has_paid_tools, &indicators);

        let confidence = if indicators.len() >= 3 {
            0.9
        } else if indicators.len() >= 2 {
            0.7
        } else if !indicators.is_empty() {
            0.5
        } else {
            0.3
        };

        Self {
            user_type,
            confidence,
            indicators,
            ecosystem,
            has_game_engine,
            has_paid_tools,
            has_enterprise_configs,
            dependency_count: dep_count,
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "Profile: {} (conf: {:.0}%)\nEcosystem: {}\nGame Engine: {}\nPaid Tools: {}\nEnterprise: {}\nDependencies: {}\nIndicators: {}",
            self.user_type, self.confidence * 100.0,
            self.ecosystem,
            if self.has_game_engine { "yes" } else { "no" },
            if self.has_paid_tools { "yes" } else { "no" },
            if self.has_enterprise_configs { "yes" } else { "no" },
            self.dependency_count,
            self.indicators.join(", "),
        )
    }
}

fn classify(enterprise: bool, paid_tools: bool, indicators: &[String]) -> ProfileType {
    let profile_rules: Vec<(&[&str], &str)> = vec![
        (
            &[
                "enterprise",
                "saml",
                "ldap",
                "okta",
                "keycloak",
                "active-directory",
                "compliance",
                "hipaa",
                "sox",
                "pci-dss",
                "rbac",
                "abac",
                "multi-tenant",
            ],
            "enterprise",
        ),
        (
            &[
                "gamestudio",
                "game-studio",
                "entertainment",
                "interactive",
                "production",
                "media",
                "creative",
                "arts",
                "film",
                "vfx",
            ],
            "studio",
        ),
        (
            &[
                "unreal", "unity", "godot", "bevy", "amethyst", "three.js", "babylon",
            ],
            "game-dev",
        ),
        (
            &[
                "photosh",
                "adobe",
                "maya",
                "blender",
                "substance",
                "houdini",
                "nuke",
                "fusion360",
                "autocad",
                "solidworks",
            ],
            "3d-artist",
        ),
        (
            &["matlab", "simulink", "ansys", "comsol", "wolfram", "maple"],
            "research",
        ),
        (
            &[
                "datadog",
                "newrelic",
                "databricks",
                "snowflake",
                "sentry",
                "elastic",
                "grafana",
            ],
            "devops",
        ),
        (
            &[
                "startup",
                "saas",
                "b2b",
                "b2c",
                "mobile",
                "ios",
                "android",
                "react-native",
                "flutter",
            ],
            "startup",
        ),
        (
            &[
                "education",
                "edtech",
                "course",
                "tutorial",
                "learning",
                "academy",
            ],
            "education",
        ),
        (
            &[
                "opensource",
                "open-source",
                "open_source",
                "oss",
                "mit",
                "apache-2.0",
                "gpl",
                "lgpl",
            ],
            "open-source",
        ),
        (
            &[
                "blockchain",
                "web3",
                "ethereum",
                "solana",
                "near",
                "cosmos",
                "crypto",
                "nft",
                "defi",
            ],
            "blockchain",
        ),
        (
            &[
                "ai",
                "ml",
                "deep-learning",
                "machine-learning",
                "neural",
                "llm",
                "gpt",
                "transformer",
                "pytorch",
                "tensorflow",
            ],
            "ai-ml",
        ),
        (
            &[
                "robotics",
                "ros",
                "automation",
                "industrial",
                "iot",
                "embedded",
            ],
            "industrial",
        ),
        (
            &[
                "government",
                "public-sector",
                "ministry",
                "federal",
                "state",
                "agency",
            ],
            "government",
        ),
        (
            &[
                "e-commerce",
                "ecommerce",
                "shopify",
                "woocommerce",
                "magento",
                "retail",
            ],
            "ecommerce",
        ),
        (
            &[
                "health",
                "healthcare",
                "medtech",
                "bio",
                "pharma",
                "clinical",
                "hospital",
            ],
            "healthcare",
        ),
        (
            &[
                "fintech",
                "banking",
                "finance",
                "payment",
                "insurance",
                "investment",
            ],
            "fintech",
        ),
    ];

    let mut scores: HashMap<&str, usize> = HashMap::new();
    for (pats, label) in &profile_rules {
        for pat in *pats {
            if indicators.iter().any(|i| i.contains(pat)) {
                *scores.entry(label).or_insert(0) += 1;
            }
        }
    }
    if enterprise {
        *scores.entry("enterprise").or_insert(0) += 1;
    }
    if paid_tools {
        *scores.entry("studio").or_insert(0) += 1;
        *scores.entry("3d-artist").or_insert(0) += 1;
    }

    scores
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(label, _)| label.to_string())
        .unwrap_or_else(|| {
            if indicators.is_empty() {
                "unknown".to_string()
            } else {
                "indie".to_string()
            }
        })
}

fn count_cargo_deps(root: &Path) -> usize {
    if let Ok(content) = std::fs::read_to_string(root.join("Cargo.toml")) {
        let mut count = 0;
        let mut in_deps = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_deps = trimmed.starts_with("[dependencies]")
                    || trimmed.starts_with("[dev-dependencies]")
                    || trimmed.starts_with("[build-dependencies]");
                continue;
            }
            if in_deps && trimmed.contains('=') && !trimmed.starts_with('#') {
                count += 1;
            }
        }
        return count;
    }
    0
}

fn count_json_deps(root: &Path, file: &str) -> usize {
    if let Ok(content) = std::fs::read_to_string(root.join(file)) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            let mut count = 0;
            if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
                count += deps.len();
            }
            if let Some(dev) = json.get("devDependencies").and_then(|d| d.as_object()) {
                count += dev.len();
            }
            return count;
        }
    }
    0
}

pub fn verify_dependencies(root: &Path) -> VerifyReport {
    let profile = ProjectProfile::detect(root);
    let mut dependencies = Vec::new();
    let mut license_issues = Vec::new();
    let mut version_issues = Vec::new();

    if root.join("Cargo.toml").exists() {
        let result = check_cargo_licenses(root);
        dependencies.extend(result.deps);
        license_issues.extend(result.license_issues);
        version_issues.extend(result.version_issues);
    }
    if root.join("package.json").exists() {
        let result = check_node_licenses(root);
        dependencies.extend(result.deps);
        license_issues.extend(result.license_issues);
        version_issues.extend(result.version_issues);
    }

    VerifyReport {
        dependencies,
        license_issues,
        version_issues,
        profile,
    }
}

struct LicenseCheckResult {
    deps: Vec<DependencyInfo>,
    license_issues: Vec<String>,
    version_issues: Vec<String>,
}

fn check_cargo_licenses(root: &Path) -> LicenseCheckResult {
    let mut deps = Vec::new();
    let mut license_issues = Vec::new();
    let version_issues = Vec::new();

    let restricted = ["BUSL-1.1", "BSL-1.1", "AGPL-3.0", "SSPL-1.0", "Elastic-2.0"];

    let cargo_lock = root.join("Cargo.lock");
    if let Ok(content) = std::fs::read_to_string(&cargo_lock) {
        let mut current_pkg = HashMap::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "[[package]]" {
                if let Some(name) = current_pkg.get("name") {
                    let version: String = current_pkg.get("version").cloned().unwrap_or_default();
                    let license = current_pkg.get("license").cloned();
                    let info = DependencyInfo {
                        name: name.clone(),
                        version,
                        license: license.clone(),
                        is_outdated: false,
                        latest_version: None,
                    };
                    deps.push(info);
                    if let Some(ref lic) = license {
                        if restricted.contains(&lic.as_str()) {
                            license_issues.push(format!("{}: restricted license ({})", name, lic));
                        }
                    }
                }
                current_pkg.clear();
                continue;
            }
            if let Some(val) = trimmed.strip_prefix("name = \"") {
                if let Some(name) = val.strip_suffix('"') {
                    current_pkg.insert("name".to_string(), name.to_string());
                }
            }
            if let Some(val) = trimmed.strip_prefix("version = \"") {
                if let Some(ver) = val.strip_suffix('"') {
                    current_pkg.insert("version".to_string(), ver.to_string());
                }
            }
            if let Some(val) = trimmed.strip_prefix("license = \"") {
                if let Some(lic) = val.strip_suffix('"') {
                    current_pkg.insert("license".to_string(), lic.to_string());
                }
            }
        }
        if let Some(name) = current_pkg.get("name") {
            let version = current_pkg.get("version").cloned().unwrap_or_default();
            let license = current_pkg.get("license").cloned();
            deps.push(DependencyInfo {
                name: name.clone(),
                version,
                license,
                is_outdated: false,
                latest_version: None,
            });
        }
    }

    LicenseCheckResult {
        deps,
        license_issues,
        version_issues,
    }
}

fn check_node_licenses(root: &Path) -> LicenseCheckResult {
    let mut deps = Vec::new();
    let mut license_issues = Vec::new();
    let mut version_issues = Vec::new();

    let restricted = ["BUSL-1.1", "BSL-1.1", "AGPL-3.0", "SSPL-1.0", "Elastic-2.0"];

    // Check package.json for name/version, try node_modules for license files
    if let Ok(content) = std::fs::read_to_string(root.join("package.json")) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(deps_obj) = json.get("dependencies").and_then(|d| d.as_object()) {
                for (name, ver) in deps_obj {
                    let license = check_node_dep_license(root, name);
                    if let Some(ref lic) = license {
                        if restricted.contains(&lic.as_str()) {
                            license_issues.push(format!("{}: restricted license ({})", name, lic));
                        }
                    }
                    deps.push(DependencyInfo {
                        name: name.clone(),
                        version: ver.as_str().unwrap_or("?").to_string(),
                        license,
                        is_outdated: false,
                        latest_version: None,
                    });
                    if ver
                        .as_str()
                        .is_some_and(|v| v.starts_with("0.") || v == "*")
                    {
                        version_issues.push(format!("{}: unstable version ({})", name, ver));
                    }
                }
            }
        }
    }

    LicenseCheckResult {
        deps,
        license_issues,
        version_issues,
    }
}

fn check_node_dep_license(root: &Path, name: &str) -> Option<String> {
    // Try node_modules/<name>/package.json
    let pkg_path = root.join("node_modules").join(name).join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(lic) = json.get("license").and_then(|l| l.as_str()) {
                return Some(lic.to_string());
            }
            // Some packages use "licenses" (plural, array)
            if let Some(lics) = json.get("licenses").and_then(|l| l.as_array()) {
                if let Some(first) = lics
                    .first()
                    .and_then(|l| l.get("type").and_then(|t| t.as_str()))
                {
                    return Some(first.to_string());
                }
            }
        }
    }
    None
}

impl VerifyReport {
    pub fn summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Profile:\n{}\n\n", self.profile.summary()));

        out.push_str(&format!("Dependencies: {}\n", self.dependencies.len()));
        if !self.license_issues.is_empty() {
            out.push_str(&format!(
                "\nLicense Issues ({}):\n",
                self.license_issues.len()
            ));
            for issue in &self.license_issues {
                out.push_str(&format!("  ! {}\n", issue));
            }
        }
        if !self.version_issues.is_empty() {
            out.push_str(&format!(
                "\nVersion Issues ({}):\n",
                self.version_issues.len()
            ));
            for issue in &self.version_issues {
                out.push_str(&format!("  ~ {}\n", issue));
            }
        }
        if self.license_issues.is_empty() && self.version_issues.is_empty() {
            out.push_str("No license or version issues found.\n");
        }

        out
    }
}

fn read_first_n_chars(path: std::path::PathBuf, n: usize) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; n];
    let bytes_read = file.read(&mut buf)?;
    buf.truncate(bytes_read);
    Ok(String::from_utf8_lossy(&buf).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_rust_project() {
        let temp = TempDir::new().unwrap();
        let cargo_toml = temp.path().join("Cargo.toml");
        fs::write(&cargo_toml, "[package]\nname = \"test\"\n[dependencies]\nserde = \"1.0\"").unwrap();
        
        let profile = ProjectProfile::detect(temp.path());
        assert_eq!(profile.ecosystem, "rust");
        assert_eq!(profile.dependency_count, 1);
    }

    #[test]
    fn test_detect_node_project() {
        let temp = TempDir::new().unwrap();
        let package_json = temp.path().join("package.json");
        fs::write(&package_json, r#"{"dependencies": {"express": "4.18.0"}}"#).unwrap();
        
        let profile = ProjectProfile::detect(temp.path());
        assert_eq!(profile.ecosystem, "node");
        assert_eq!(profile.dependency_count, 1);
    }

    #[test]
    fn test_detect_game_engine() {
        let temp = TempDir::new().unwrap();
        let cargo_toml = temp.path().join("Cargo.toml");
        fs::write(&cargo_toml, "[dependencies]\nbevy = \"0.12\"").unwrap();
        
        let profile = ProjectProfile::detect(temp.path());
        assert!(profile.has_game_engine);
        assert!(profile.indicators.iter().any(|i| i.contains("game-engine")));
    }

    #[test]
    fn test_classify_empty() {
        let result = classify(false, false, &[]);
        assert_eq!(result, "unknown");
    }

    #[test]
    fn test_classify_enterprise() {
        let indicators = vec!["enterprise-config: saml".to_string()];
        let result = classify(true, false, &indicators);
        assert_eq!(result, "enterprise");
    }

    #[test]
    fn test_classify_game_dev() {
        let indicators = vec!["game-engine: rust".to_string()];
        let result = classify(false, false, &indicators);
        assert_eq!(result, "indie");
    }

    #[test]
    fn test_count_cargo_deps() {
        let temp = TempDir::new().unwrap();
        let cargo_toml = temp.path().join("Cargo.toml");
        fs::write(&cargo_toml, "[dependencies]\nserde = \"1.0\"\ntokio = \"1.0\"\n[dev-dependencies]\ntempfile = \"3.0\"").unwrap();
        
        let count = count_cargo_deps(temp.path());
        assert_eq!(count, 3);
    }

    #[test]
    fn test_count_json_deps() {
        let temp = TempDir::new().unwrap();
        let package_json = temp.path().join("package.json");
        fs::write(&package_json, r#"{"dependencies": {"express": "4.18.0"}, "devDependencies": {"jest": "29.0.0"}}"#).unwrap();
        
        let count = count_json_deps(temp.path(), "package.json");
        assert_eq!(count, 2);
    }

    #[test]
    fn test_verify_dependencies_empty() {
        let temp = TempDir::new().unwrap();
        let report = verify_dependencies(temp.path());
        assert!(report.license_issues.is_empty());
        assert!(report.version_issues.is_empty());
    }

    #[test]
    fn test_profile_summary() {
        let profile = ProjectProfile {
            user_type: "indie".to_string(),
            confidence: 0.8,
            indicators: vec!["test".to_string()],
            ecosystem: "rust".to_string(),
            has_game_engine: false,
            has_paid_tools: false,
            has_enterprise_configs: false,
            dependency_count: 5,
        };
        
        let summary = profile.summary();
        assert!(summary.contains("indie"));
        assert!(summary.contains("rust"));
    }
}
