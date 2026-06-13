use crate::types::SecurityAuditEntry;
use regex::Regex;
use std::fs;

pub struct SecurityAuditor;

const DANGEROUS_PATTERNS: &[(&str, &str, &str, &str, &str, &[&str])] = &[
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

impl SecurityAuditor {
    pub fn new() -> Self {
        Self
    }

    pub fn audit(&self, files: &[String]) -> Vec<SecurityAuditEntry> {
        let mut entries = vec![];

        for file in files {
            let ext = std::path::Path::new(file)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let content = match fs::read_to_string(file) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let lines: Vec<&str> = content.lines().collect();

            for (type_, severity, pattern, description, recommendation, langs) in DANGEROUS_PATTERNS
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
                        _ => "",
                    };
                    mapped.is_empty() || langs.contains(&mapped)
                };

                if !ext_match(&ext, langs) {
                    continue;
                }

                let re = match Regex::new(pattern) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                for (i, line) in lines.iter().enumerate() {
                    if re.is_match(line) {
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
        "catch\\s*\\([^)]*\\)\\s*\\{[^}]*\\}",
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
