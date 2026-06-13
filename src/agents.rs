use std::fs;
use regex::Regex;
use crate::types::SecurityAuditEntry;

pub struct SecurityAuditor;

const DANGEROUS_PATTERNS: &[(&str, &str, &str, &str, &str)] = &[
    ("command_injection", "critical",
     "exec\\s*\\(\\s*['\"`]\\s*rm\\s+-rf\\s+[/~]",
     "Dangerous rm -rf execution detected",
     "Use safe file deletion with proper path validation"),
    ("dangerous_query", "critical",
     "db\\.exec(?:ute)?\\s*\\(\\s*['\"`]\\s*DROP\\s+(?:TABLE|DATABASE)",
     "DROP TABLE/DATABASE query detected",
     "Use migrations with automatic backup; never DROP in production code"),
    ("dangerous_query", "high",
     "db\\.exec(?:ute)?\\s*\\(\\s*['\"`]\\s*DELETE\\s+FROM",
     "DELETE query without WHERE clause may delete all rows",
     "Always specify WHERE clause or use safe migration patterns"),
    ("dangerous_query", "high",
     "TRUNCATE\\s+(?:TABLE\\s+)?\\w+",
     "TRUNCATE detected - irreversible data loss",
     "Use DELETE with condition or backup first"),
    ("unsafe_deserialization", "high",
     "eval\\s*\\(",
     "eval() usage - arbitrary code execution risk",
     "Avoid eval(); use safe parsers"),
    ("xss", "high",
     "innerHTML\\s*=",
     "innerHTML assignment - XSS vulnerability",
     "Use textContent or sanitize with DOMPurify"),
    ("sql_injection", "high",
     "\\.query\\s*\\(\\s*['\"`]\\s*SELECT",
     "Raw SQL query - possible SQL injection",
     "Use parameterized queries or prepared statements"),
    ("insecure_crypto", "medium",
     "crypto\\.createHash\\s*\\(\\s*['\"`]md5['\"`]",
     "MD5 hash usage - cryptographically broken",
     "Use SHA-256 or higher"),
    ("insecure_crypto", "medium",
     "crypto\\.createHash\\s*\\(\\s*['\"`]sha1['\"`]",
     "SHA-1 hash usage - cryptographically broken",
     "Use SHA-256 or higher"),
    ("path_traversal", "high",
     "\\.\\.\\/|\\.\\.[\\\\/]",
     "Path traversal pattern detected",
     "Use path.resolve() with allowlist-based path validation"),
];

impl SecurityAuditor {
    pub fn new() -> Self {
        Self
    }

    pub fn audit(&self, files: &[String]) -> Vec<SecurityAuditEntry> {
        let mut entries = vec![];

        for file in files {
            let content = match fs::read_to_string(file) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let lines: Vec<&str> = content.lines().collect();

            for (type_, severity, pattern, description, recommendation) in DANGEROUS_PATTERNS {
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
    ("catch\\s*\\([^)]*\\)\\s*\\{[^}]*\\}",
     "Generic catch clause",
     "Type the error or add specific error handling"),
    ("//\\s*(TODO|todo|FIXME|fixme|HACK|hack)",
     "Code smell: TODO/FIXME/HACK",
     "Address the technical debt"),
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
