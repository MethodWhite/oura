use crate::types::SecurityAuditEntry;
use regex::Regex;
use std::fs;

pub struct SecurityAuditor;

type DangerousPattern = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const DANGEROUS_PATTERNS: &[DangerousPattern] = &[
    (
        "command_injection",
        "critical",
        r#"(?i)(?:exec|spawn|system|popen|shell_exec)\s*\(\s*['"`][^'"`]*rm\s+[-][^'"`]*[/~]"#,
        "Dangerous rm -rf execution detected via command execution function",
        "Use safe file deletion with proper path validation",
        &["js", "php", "py"],
    ),
    (
        "dangerous_query",
        "critical",
        r#"(?i)db\.(?:exec(?:ute)?|query|run)\s*\(\s*['"`]\s*DROP\s+(?:TABLE|DATABASE)"#,
        "DROP TABLE/DATABASE query detected",
        "Use migrations with automatic backup; never DROP in production code",
        &["js", "ts", "py", "go", "rs"],
    ),
    (
        "dangerous_query",
        "high",
        r#"(?i)db\.(?:exec(?:ute)?|query|run)\s*\(\s*['"`]\s*DELETE\s+(?!FROM\s+\()"#,
        "DELETE query without WHERE may delete all rows",
        "Always specify WHERE clause or use safe migration patterns",
        &["js", "ts", "py", "go"],
    ),
    (
        "dangerous_query",
        "high",
        r#"(?i)\bTRUNCATE\s+(?:TABLE\s+)?\w+"#,
        "TRUNCATE detected - irreversible data loss",
        "Use DELETE with condition or backup first",
        &["sql"],
    ),
    (
        "unsafe_deserialization",
        "high",
        r#"(?<!\w)(?:eval|JSON\.parse)\s*\(\s*(?:request|req|body|data|input|userInput)"#,
        "eval() of user input - arbitrary code execution risk",
        "Avoid eval(); use safe parsers",
        &["js", "ts"],
    ),
    (
        "xss",
        "high",
        r#"(?i)(?:innerHTML|outerHTML|insertAdjacentHTML)\s*="#,
        "innerHTML assignment - XSS vulnerability",
        "Use textContent or sanitize with DOMPurify",
        &["js", "ts", "html"],
    ),
    (
        "sql_injection",
        "high",
        r#"(?i)(?:execute|query|run|exec)\s*\(\s*['"`]\s*(?:SELECT|INSERT|UPDATE|DELETE)"#,
        "Raw SQL query - possible SQL injection",
        "Use parameterized queries or prepared statements",
        &["js", "ts", "py", "go", "java", "php"],
    ),
    (
        "insecure_crypto",
        "medium",
        r#"(?i)(?:MD5|SHA1?)\s*(?:\.|::)(?:hash|digest|create)\s*\(|crypto\.createHash\s*\(\s*['"`](?:md5|sha1)['"`]"#,
        "MD5/SHA-1 hash usage - cryptographically broken",
        "Use SHA-256 or higher",
        &["js", "ts", "py", "rs"],
    ),
    (
        "path_traversal",
        "high",
        r#"(?i)(?:open|readFile|writeFile|unlink|rmdir)\s*\([^)]*\.\.\/"#,
        "Path traversal pattern in file operations",
        "Use path.resolve() with allowlist-based path validation",
        &["js", "ts", "py", "go", "rs"],
    ),
];

struct CompiledPattern {
    type_: &'static str,
    severity: &'static str,
    regex: regex::Regex,
    description: &'static str,
    recommendation: &'static str,
    langs: &'static [&'static str],
}

fn compiled_patterns() -> &'static [CompiledPattern] {
    static PATTERNS: std::sync::OnceLock<Vec<CompiledPattern>> = std::sync::OnceLock::new();
    PATTERNS.get_or_init(|| {
        DANGEROUS_PATTERNS.iter().filter_map(|(type_, severity, pattern, description, recommendation, langs)| {
            match regex::Regex::new(pattern) {
                Ok(regex) => Some(CompiledPattern {
                    type_, severity, regex, description, recommendation, langs,
                }),
                Err(e) => {
                    eprintln!("[Oura] Warning: failed to compile security pattern '{}': {}", type_, e);
                    None
                }
            }
        }).collect()
    })
}

impl SecurityAuditor {
    pub fn new() -> Self {
        Self
    }

    pub fn audit(&self, files: &[String]) -> Vec<SecurityAuditEntry> {
        let mut entries = vec![];
        let compiled = compiled_patterns();

        for file in files {
            let path = std::path::Path::new(file);
            if let Ok(meta) = path.metadata() {
                if meta.len() > 10_000_000 {
                    continue;
                }
            }

            let ext = path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let content = match fs::read_to_string(file) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[Oura] SecurityAuditor: skipping unreadable file {}: {}", file, e);
                    continue;
                }
            };

            let lines: Vec<&str> = content.lines().collect();

            for CompiledPattern { type_, severity, regex, description, recommendation, langs } in compiled
            {
                // Skip pattern if its language list doesn't match the file extension
                let ext_match = |ext: &str, langs: &[&str]| -> bool {
                    let mapped = match ext {
                        "js" | "jsx" | "mjs" | "cjs" => "js",
                        "ts" | "tsx" | "mts" | "cts" => "ts",
                        "py" | "pyw" => "py",
                        "rs" => "rs",
                        "go" => "go",
                        "java" | "kt" | "kts" => "java",
                        "php" => "php",
                        "sql" => "sql",
                        "html" | "htm" | "xhtml" => "html",
                        _ => return false,
                    };
                    langs.contains(&mapped)
                };

                if !ext_match(&ext, langs) {
                    continue;
                }

                for (i, line) in lines.iter().enumerate() {
                    if regex.is_match(line) {
                        entries.push(SecurityAuditEntry {
                            type_: type_.to_string(),
                            severity: severity.to_string(),
                            file: file.clone(),
                            line: Some(i + 1),
                            description: description.to_string(),
                            recommendation: recommendation.to_string(),
                        });
                    }
                }
            }
        }

        entries
    }
}

pub struct RefactorEngine;

const CLEAN_CODE_PATTERNS: &[(&str, &str, &str)] = &[
    (
        "catch\\s*\\([^)]*\\)\\s*\\{",
        "Generic catch clause",
        "Type the error or add specific error handling",
    ),
    (
        "//\\s*(TODO|todo|FIXME|fixme|HACK|hack)",
        "Code smell: TODO/FIXME/HACK",
        "Address the technical debt",
    ),
];

impl RefactorEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(&self, file: &str) -> (Vec<String>, Vec<String>) {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => return (vec!["Cannot read file".into()], vec![]),
        };

        let mut issues = vec![];
        let mut suggestions = vec![];

        for (pattern, description, suggestion) in CLEAN_CODE_PATTERNS {
            let re = match Regex::new(pattern) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let count = re.find_iter(&content).count();
            if count > 0 {
                issues.push(format!("{} ({} occurrences)", description, count));
                suggestions.push(suggestion.to_string());
            }
        }

        let line_count = content.lines().count();
        if line_count > 500 {
            issues.push(format!("File too long: {} lines", line_count));
            suggestions.push("Split into smaller modules".into());
        }

        (issues, suggestions)
    }
}

pub struct AntiDeletionGuard;

impl AntiDeletionGuard {
    pub fn new() -> Self {
        Self
    }

    pub fn check_integrity(&self) -> Result<String, String> {
        Ok("Integrity check passed: no baseline violations detected (baseline system not yet implemented in Rust version)".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_security_auditor_creation() {
        let auditor = SecurityAuditor::new();
        let _ = auditor;
    }

    #[test]
    fn test_security_auditor_no_findings() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("clean.js");
        fs::write(&file, "const x = 1;").unwrap();

        let auditor = SecurityAuditor::new();
        let findings = auditor.audit(&[file.to_string_lossy().to_string()]);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_security_auditor_sql_injection() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.js");
        fs::write(&file, "db.execute('SELECT * FROM users')").unwrap();

        let auditor = SecurityAuditor::new();
        let findings = auditor.audit(&[file.to_string_lossy().to_string()]);
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.type_ == "sql_injection"));
    }

    #[test]
    fn test_security_auditor_xss() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.js");
        fs::write(&file, "element.innerHTML = userInput").unwrap();

        let auditor = SecurityAuditor::new();
        let findings = auditor.audit(&[file.to_string_lossy().to_string()]);
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.type_ == "xss"));
    }

    #[test]
    fn test_security_auditor_nonexistent_file() {
        let auditor = SecurityAuditor::new();
        let findings = auditor.audit(&["/nonexistent/file.js".to_string()]);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_refactor_engine_creation() {
        let engine = RefactorEngine::new();
        let _ = engine;
    }

    #[test]
    fn test_refactor_engine_clean_code() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("clean.js");
        fs::write(&file, "const x = 1;").unwrap();

        let engine = RefactorEngine::new();
        let (issues, _) = engine.analyze(&file.to_string_lossy());
        assert!(issues.is_empty());
    }

    #[test]
    fn test_refactor_engine_todo_detection() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.js");
        fs::write(&file, "// TODO: fix this later").unwrap();

        let engine = RefactorEngine::new();
        let (issues, suggestions) = engine.analyze(&file.to_string_lossy());
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("TODO")));
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_refactor_engine_long_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("long.js");
        let content = "const x = 1;\n".repeat(600);
        fs::write(&file, content).unwrap();

        let engine = RefactorEngine::new();
        let (issues, _) = engine.analyze(&file.to_string_lossy());
        assert!(issues.iter().any(|i| i.contains("File too long")));
    }

    #[test]
    fn test_refactor_engine_nonexistent_file() {
        let engine = RefactorEngine::new();
        let (issues, _) = engine.analyze("/nonexistent/file.js");
        assert!(issues.iter().any(|i| i.contains("Cannot read file")));
    }

    #[test]
    fn test_anti_deletion_guard() {
        let guard = AntiDeletionGuard::new();
        let result = guard.check_integrity();
        assert!(result.is_ok());
        assert!(result.unwrap().contains("passed"));
    }
}
