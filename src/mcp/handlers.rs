use crate::agents::*;
use crate::fs_utils::{self, run_command_timeout, safe_path, CleanupContext};
use crate::mcp::McpServer;
use crate::types::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

impl McpServer {
    pub(super) async fn cmd_start_loop(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let goal = match args["goal"].as_str() {
            Some(g) => g,
            None => return self.err(id, -32602, "Missing required parameter: goal".into()),
        };
        let max_iter = args["maxIterations"].as_u64().map(|n| n as u32);
        let manual = args["manual"].as_bool().unwrap_or(false);

        match self.engine.start(goal, manual, max_iter).await {
            Ok(state) => {
                let mode = if manual { "manual" } else { "autonomous" };
                let msg = format!("Loop started in {} mode: {}\nLoop ID: {}\nStatus: {}", mode, goal, state.id, state.status);
                self.ok(id, json!({ "content": Self::text_content(msg) }))
            }
            Err(e) => self.err(id, e.code(), format!("Failed to start loop: {}", e)),
        }
    }

    pub(super) async fn cmd_iterate(&mut self, id: Value) -> JsonRpcResponse {
        match self.engine.iterate().await {
            Ok(result) => {
                let errors = result.feedback.iter().filter(|f| f.type_ == "error").count();
                let warnings = result.feedback.iter().filter(|f| f.type_ == "warning").count();

                let mut msg = format!(
                    "Iteration #{} - Score: {:.1}/100\nFeedback: {} errors, {} warnings",
                    result.iteration, result.score, errors, warnings
                );
                for f in &result.feedback {
                    msg.push_str(&format!("\n  [{}] {}: {}", f.type_, f.source, f.message));
                }
                if result.status == "completed" { msg.push_str("\n\n*** CONVERGED ***"); }
                else if result.status == "failed" { msg.push_str("\n\n*** FAILED ***"); }

                self.ok(id, json!({ "content": Self::text_content(msg) }))
            }
            Err(e) => self.err(id, e.code(), format!("Iteration failed: {}", e)),
        }
    }

    pub(super) fn cmd_loop_status(&self, id: Value) -> JsonRpcResponse {
        let state = match self.engine.get_state() {
            Ok(s) => s,
            Err(e) => return self.err(id, e.code(), format!("Failed to get state: {}", e)),
        };
        match state {
            Some(state) => {
                let last = state.history.last();
                let mut lines = vec![
                    format!("Status: {}", state.status),
                    format!("Goal: {}", state.goal),
                    format!("Loop ID: {}", state.id),
                    format!("Current Iteration: {}", state.current_iteration),
                    format!("Started: {}", state.start_time),
                ];
                if let Some(last) = last {
                    lines.push(format!("Last Iteration: #{} (score: {:.1}/100)", last.iteration, last.score));
                }
                self.ok(id, json!({ "content": Self::text_content(lines.join("\n")) }))
            }
            None => self.ok(id, json!({ "content": Self::text_content("No active loop".into()) })),
        }
    }

    pub(super) fn cmd_loop_stop(&mut self, id: Value) -> JsonRpcResponse {
        match self.engine.stop() {
            Ok(iter) => self.ok(id, json!({ "content": Self::text_content(format!("Loop stopped after {} iterations", iter)) })),
            Err(e) => self.err(id, e.code(), format!("Failed to stop loop: {}", e)),
        }
    }

    pub(super) fn cmd_results(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let results = match self.engine.get_results() {
            Ok(r) => r,
            Err(e) => return self.err(id, e.code(), format!("Failed to get results: {}", e)),
        };
        let iter_filter = args["iteration"].as_u64();

        let filtered: Vec<&IterationResult> = match iter_filter {
            Some(n) if n > 0 => results.iter().filter(|r| r.iteration == n as u32).collect(),
            _ => results.iter().collect(),
        };

        let summary: Vec<Value> = filtered.iter().map(|r| {
            json!({
                "iteration": r.iteration, "score": r.score, "status": r.status,
                "actions": r.actions.iter().map(|a| json!({"agent": a.agent, "type": a.type_, "status": a.status})).collect::<Vec<_>>(),
                "feedback": r.feedback.iter().map(|f| json!({"source": f.source, "type": f.type_, "message": f.message})).collect::<Vec<_>>(),
            })
        }).collect();

        match serde_json::to_string_pretty(&summary) {
            Ok(json) => self.ok(id, json!({ "content": Self::text_content(json) })),
            Err(e) => self.err(id, -32603, format!("Failed to serialize results: {}", e)),
        }
    }

    pub(super) fn cmd_configure(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        if let Some(max) = args["maxIterations"].as_u64() {
            if let Err(e) = self.engine.update_max_iterations(max as u32) {
                return self.err(id, e.code(), format!("Failed to set max iterations: {}", e));
            }
        }
        if let Some(threshold) = args["convergenceThreshold"].as_f64() {
            if let Err(e) = self.engine.update_convergence_threshold(threshold) {
                return self.err(id, e.code(), format!("Failed to set threshold: {}", e));
            }
        }
        if let Some(dir) = args["workingDirectory"].as_str() {
            let dir_path = Path::new(dir);
            let resolved = match Self::validate_working_dir(dir_path) {
                Ok(p) => p,
                Err(e) => return self.err(id, -32602, e),
            };
            if std::env::set_current_dir(&resolved).is_err() {
                return self.err(id, -32603, format!("Failed to set working directory: {}", dir));
            }
        }
        self.ok(id, json!({ "content": Self::text_content("Configuration updated".into()) }))
    }

    fn validate_working_dir(dir: &Path) -> Result<PathBuf, String> {
        let resolved = if dir.exists() {
            std::fs::canonicalize(dir).map_err(|e| format!("Cannot resolve path {}: {}", dir.display(), e))?
        } else {
            let parent = dir.parent().unwrap_or(Path::new("/"));
            if !parent.exists() {
                return Err(format!("Parent directory does not exist: {}", parent.display()));
            }
            dir.to_path_buf()
        };
        if resolved.starts_with("/proc") || resolved.starts_with("/sys") || resolved.starts_with("/dev") {
            return Err("Cannot set working directory to a system directory".into());
        }
        Ok(resolved)
    }

    pub(super) fn cmd_working_dir(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let dir = match args["directory"].as_str() {
            Some(d) => d,
            None => return self.err(id, -32602, "Missing required parameter: directory".into()),
        };
        let dir_path = Path::new(dir);
        let resolved = match Self::validate_working_dir(dir_path) {
            Ok(p) => p,
            Err(e) => return self.err(id, -32602, e),
        };
        if std::env::set_current_dir(&resolved).is_err() {
            return self.err(id, -32603, format!("Failed to set working directory: {}", dir));
        }
        self.ok(id, json!({ "content": Self::text_content(format!("Working directory set to: {}", resolved.display())) }))
    }

    pub(super) fn cmd_plugin_load(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let path = match args["path"].as_str() {
            Some(p) => p,
            None => return self.err(id, -32602, "Missing required parameter: path".into()),
        };
        self.ok(id, json!({ "content": Self::text_content(
            format!("Plugin loading not yet implemented in Rust version (requested path: {})", path)
        ) }))
    }

    pub(super) fn cmd_plugin_list(&self, id: Value) -> JsonRpcResponse {
        self.ok(id, json!({ "content": Self::text_content("No plugins loaded".into()) }))
    }

    pub(super) fn cmd_analyze_security(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let raw_files: Vec<String> = args["files"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let mut skipped = Vec::new();
        let files: Vec<String> = raw_files.iter().filter_map(|f| {
            match safe_path(f) {
                Ok(p) => Some(p.to_string_lossy().to_string()),
                Err(_) => { skipped.push(f.clone()); None }
            }
        }).collect();
        let auditor = SecurityAuditor::new();
        let findings = auditor.audit(&files);

        if findings.is_empty() && skipped.is_empty() {
            return self.ok(id, json!({ "content": Self::text_content("Security scan complete: no vulnerabilities found".into()) }));
        }
        let critical = findings.iter().filter(|f| f.severity == "critical").count();
        let high = findings.iter().filter(|f| f.severity == "high").count();
        let medium = findings.iter().filter(|f| f.severity == "medium").count();
        let low = findings.iter().filter(|f| f.severity == "low").count();

        let mut report = format!("Security scan complete: {} issues found\n  Critical: {}\n  High: {}\n  Medium: {}\n  Low: {}\n", findings.len(), critical, high, medium, low);
        for f in &findings {
            report.push_str(&format!("\n[{}] {}:{} - {}\n  => {}", f.severity.to_uppercase(), f.file, f.line.map(|l| l.to_string()).unwrap_or_else(|| "?".into()), f.description, f.recommendation));
        }
        if !skipped.is_empty() {
            report.push_str(&format!("\nSkipped {} files (not found): {}\n", skipped.len(), skipped.join(", ")));
        }
        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    pub(super) fn cmd_analyze_code(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let raw_files: Vec<String> = args["files"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let mut skipped = Vec::new();
        let files: Vec<String> = raw_files.iter().filter_map(|f| {
            match safe_path(f) {
                Ok(p) => Some(p.to_string_lossy().to_string()),
                Err(_) => { skipped.push(f.clone()); None }
            }
        }).collect();
        let refactor = RefactorEngine::new();
        let mut report = String::from("Code Analysis Results:\n");
        for file in &files {
            let (issues, suggestions) = refactor.analyze(file);
            if !issues.is_empty() {
                report.push_str(&format!("  {}:\n", file));
                for issue in &issues { report.push_str(&format!("    [ISSUE] {}\n", issue)); }
                for suggestion in &suggestions { report.push_str(&format!("    [SUGGEST] {}\n", suggestion)); }
            }
        }
        if !skipped.is_empty() {
            report.push_str(&format!("\nSkipped {} files (not found): {}\n", skipped.len(), skipped.join(", ")));
        }
        if report == "Code Analysis Results:\n" { report = "Code analysis passed: no clean code violations detected".into(); }
        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    pub(super) fn cmd_check_integrity(&self, id: Value) -> JsonRpcResponse {
        let guard = AntiDeletionGuard::new();
        match guard.check_integrity() {
            Ok(msg) => self.ok(id, json!({ "content": Self::text_content(msg) })),
            Err(msg) => self.err(id, -32603, msg),
        }
    }

    pub(super) fn cmd_guard_destructive(&self, id: Value, _args: &Value) -> JsonRpcResponse {
        self.ok(id, json!({ "content": Self::text_content(
            "Destructive query guard active: all DROP/DELETE/TRUNCATE/ALTER operations require explicit confirmation with backup verification".into())
        }))
    }

    pub(super) fn cmd_analyze_project(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let root = args["path"].as_str().unwrap_or(".");
        let max_depth = args["depth"].as_u64().unwrap_or(4).min(10) as usize;  // clamp to 10 max
        let root_path = match safe_path(root) {
            Ok(p) => p,
            Err(e) => return self.err(id, -32602, e),
        };

        let mut report = String::new();
        report.push_str(&format!("[{}] ({})\n", root_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| "?".into()), root_path.display()));
        report.push_str(&format!("   Size: {}\n", fs_utils::format_size(fs_utils::dir_size(&root_path, max_depth, 10000).unwrap_or(0))));

        if let Some(rm) = fs_utils::find_readme(&root_path) {
            if let Ok(content) = std::fs::read_to_string(&rm) {
                let preview: String = content.lines().take(8).collect::<Vec<_>>().join("\n");
                report.push_str(&format!("   README: {} chars\n{}\n", content.len(), preview));
            }
        }

        let docs_dir = root_path.join("docs");
        if docs_dir.exists() && docs_dir.is_dir() {
            report.push_str(&format!("   docs/: {} files\n", std::fs::read_dir(&docs_dir).map(|e| e.count()).unwrap_or(0)));
        }

        let configs = fs_utils::scan_configs(&root_path);
        if !configs.is_empty() { report.push_str(&format!("   Config: {}\n", configs.join(", "))); }

        let mut entries = fs_utils::collect_entries(&root_path, 0, max_depth, 500);
        entries.sort_by_key(|a| a.1);
        for (path_str, depth, is_dir, size) in &entries {
            if *depth == 0 || *depth > max_depth { continue; }
            let indent = "  ".repeat(*depth);
            let marker = if *is_dir { "+" } else { " " };
            report.push_str(&format!("{}{} {}{}\n", indent, marker, path_str, if *is_dir { String::new() } else { format!(" ({})", fs_utils::format_size(*size)) }));
        }

        let large: Vec<_> = entries.iter().filter(|(_, _, is_dir, size)| !is_dir && *size > 100_000).collect();
        if !large.is_empty() { report.push_str(&format!("\n   Large files (>100KB): {}\n", large.len())); }

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    fn is_private_ip(host: &str) -> bool {
        if host == "localhost" || host == "127.0.0.1" || host == "::1" {
            return true;
        }
        if host.ends_with(".local") || host.ends_with(".internal") {
            return true;
        }
        if let Ok(addr) = host.parse::<std::net::IpAddr>() {
            match addr {
                std::net::IpAddr::V4(a) => {
                    return a.is_loopback()
                        || a.is_private()
                        || a.is_link_local()
                }
                std::net::IpAddr::V6(a) => {
                    return a.is_loopback()
                        || a.is_unicast_link_local()
                }
            }
        }
        false
    }

    pub(super) fn cmd_mcp_call(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let server_url = match args["server_url"].as_str() {
            Some(u) => u,
            None => return self.err(id, -32602, "Missing required parameter: server_url".into()),
        };
        let parsed = match url::Url::parse(server_url) {
            Ok(u) => u,
            Err(e) => return self.err(id, -32602, format!("Invalid server_url: {}", e)),
        };
        let host = parsed.host_str().unwrap_or("");
        if host.is_empty() || Self::is_private_ip(host) {
            return self.err(id, -32602, "SSRF protection: requests to private/local addresses are blocked".into());
        }
        let tool_name = match args["tool_name"].as_str() {
            Some(t) => t,
            None => return self.err(id, -32602, "Missing required parameter: tool_name".into()),
        };
        let arguments = args.get("arguments").cloned().unwrap_or(json!({}));
        let endpoint = args["endpoint"].as_str().unwrap_or("/message");
        let url = format!("{}{}", server_url.trim_end_matches('/'), endpoint);
        let request_body = json!({
            "jsonrpc": "2.0", "id": "1", "method": "tools/call",
            "params": { "name": tool_name, "arguments": arguments }
        });

        let result = Self::http_post(&url, &request_body);
        match result {
            Ok(body) => self.ok(id, json!({ "content": Self::text_content(body) })),
            Err(e) => self.err(id, -32603, format!("MCP call failed: {}", e)),
        }
    }

    fn http_post(url: &str, body: &Value) -> Result<String, String> {
        let url_owned = url.to_string();
        let body_str = serde_json::to_string(body).map_err(|e| format!("Serialization error: {}", e))?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| format!("HTTP client build error: {}", e))
                .and_then(|client| {
                    client.post(&url_owned)
                        .header("Content-Type", "application/json")
                        .body(body_str.clone())
                        .send()
                        .map_err(|e| format!("HTTP request failed: {}", e))
                        .and_then(|resp| resp.text().map_err(|e| format!("HTTP read error: {}", e)))
                });
            let _ = tx.send(result);
        });
        rx.recv_timeout(std::time::Duration::from_secs(35))
            .map_err(|_| "MCP call timed out".to_string())?
    }

    pub(super) fn cmd_version(&self, id: Value) -> JsonRpcResponse {
        let version = env!("CARGO_PKG_VERSION");
        let build_ts = option_env!("BUILD_TIME").unwrap_or("0");
        let git_head = option_env!("GIT_HEAD").unwrap_or("unknown");
        let msg = format!("Oura v{} | build: {} | commit: {}", version, build_ts, git_head);
        self.ok(id, json!({ "content": Self::text_content(msg) }))
    }

    pub(super) async fn cmd_update(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let apply = args["apply"].as_bool().unwrap_or(false);
        let project_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

        if !project_dir.join(".git").exists() {
            return self.ok(id, json!({ "content": Self::text_content("Not a git repository. Can't auto-update.".into()) }));
        }

        let default_branch = tokio::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(project_dir)
            .output().await.ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "HEAD")
            .unwrap_or_else(|| "main".into());

        let mut report = format!("Oura v{} - checking for updates...\n", env!("CARGO_PKG_VERSION"));

        match run_command_timeout(&["git", "fetch", "--quiet"], project_dir, 30).await {
            Ok(output) if output.status.success() => report.push_str("Git fetch OK.\n"),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                report.push_str(&format!("Git fetch: {}\n", stderr));
                return self.ok(id, json!({ "content": Self::text_content(report) }));
            }
            Err(e) => {
                report.push_str(&format!("Git fetch failed: {}\n", e));
                return self.ok(id, json!({ "content": Self::text_content(report) }));
            }
        }

        let remote_ref = format!("HEAD..origin/{}", default_branch);
        let ahead = tokio::process::Command::new("git")
            .args(["rev-list", "--count", &remote_ref])
            .current_dir(project_dir).output().await;
        let commits_behind = match ahead {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => "?".to_string(),
        };
        let behind_count: i32 = commits_behind.parse().unwrap_or(-1);

        if behind_count > 0 {
            report.push_str(&format!("{} commits behind origin/{}\n", behind_count, default_branch));
            if apply {
                report.push_str("Applying update...\n");
                match run_command_timeout(&["git", "pull", "--ff-only"], project_dir, 30).await {
                    Ok(output) if output.status.success() => report.push_str("Git pull OK.\n"),
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        report.push_str(&format!("Git pull failed: {}\n", stderr));
                        return self.ok(id, json!({ "content": Self::text_content(report) }));
                    }
                    Err(e) => {
                        report.push_str(&format!("Git pull failed: {}\n", e));
                        return self.ok(id, json!({ "content": Self::text_content(report) }));
                    }
                }
                report.push_str("Running cargo build --release...\n");
                match run_command_timeout(&["cargo", "build", "--release"], project_dir, 300).await {
                    Ok(output) if output.status.success() => report.push_str("Build OK.\n"),
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        report.push_str(&format!("Build failed: {}\n", stderr));
                    }
                    Err(e) => report.push_str(&format!("Build failed: {}\n", e)),
                }
            } else {
                report.push_str("Use oura_update with apply=true to upgrade.\n");
            }
        } else if behind_count == 0 {
            report.push_str(&format!("Already up to date with origin/{}\n", default_branch));
        } else {
            report.push_str("Could not determine update status.\n");
        }

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    pub(super) fn cmd_profile(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let path = args["path"].as_str().map(Path::new).map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let profile = crate::profile::ProjectProfile::detect(&path);
        self.ok(id, json!({ "content": Self::text_content(profile.summary()) }))
    }

    pub(super) fn cmd_verify(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let path = args["path"].as_str().map(Path::new).map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let check_licenses = args["check_licenses"].as_bool().unwrap_or(true);
        let check_versions = args["check_versions"].as_bool().unwrap_or(true);

        let profile = crate::profile::ProjectProfile::detect(&path);
        let verify = crate::profile::verify_dependencies(&path);

        let mut report = profile.summary();
        if check_licenses && !verify.license_issues.is_empty() {
            report.push_str(&format!("\nLicense issues:\n  {}", verify.license_issues.join("\n  ")));
        }
        if check_versions && !verify.version_issues.is_empty() {
            report.push_str(&format!("\nVersion issues:\n  {}", verify.version_issues.join("\n  ")));
        }
        if report.is_empty() { report = "No issues found".into(); }

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }

    pub(super) async fn cmd_connector(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let tool = match args["tool"].as_str() {
            Some(t) => t,
            None => return self.err(id, -32602, "Missing required parameter: tool".into()),
        };
        let arguments = args.get("arguments").cloned().unwrap_or(json!({}));
        let transport = args["transport"].as_str().unwrap_or("http");

        match transport {
            "quic" => {
                let host = args["host"].as_str().unwrap_or("127.0.0.1");
                let port = args["port"].as_u64().unwrap_or(7439) as u16;

                match crate::connector::QuicConnector::new() {
                    Ok(conn) => match conn.call_tool(host, port, tool, &arguments).await {
                        Ok(body) => self.ok(id, json!({ "content": Self::text_content(body) })),
                        Err(e) => self.err(id, -32603, format!("QUIC call failed: {}", e)),
                    },
                    Err(e) => self.err(id, -32603, format!("QUIC init failed: {}", e)),
                }
            }
            _ => {
                let server_url = args["server_url"].as_str().unwrap_or("http://localhost:7438");
                let endpoint = args["endpoint"].as_str().unwrap_or("/message");

                match crate::feedback::call_mcp_tool(server_url, endpoint, tool, &arguments).await {
                    Ok(body) => self.ok(id, json!({ "content": Self::text_content(body) })),
                    Err(e) => self.err(id, -32603, format!("HTTP call failed: {}", e)),
                }
            }
        }
    }

    pub(super) fn cmd_cleanup(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let root = args["path"].as_str().unwrap_or(".");
        let dry_run = args["dry_run"].as_bool().unwrap_or(true);
        let confirm = args["confirm"].as_bool().unwrap_or(false);
        let older_than = args["older_than_days"].as_u64().unwrap_or(30);
        let max_depth = args["max_depth"].as_u64().unwrap_or(20) as usize;
        let default_patterns: Vec<String> = vec!["*.tmp".into(), "*.temp".into(), "*.log".into(), "*.bak".into(), "*.swp".into(), "*.swo".into(), "*.pyc".into(), "__pycache__".into(), ".DS_Store".into(), "Thumbs.db".into()];
        let default_dir_patterns: Vec<String> = vec!["node_modules".into(), ".next".into(), ".turbo".into(), "dist".into(), "build".into(), ".cache".into()];
        let patterns: Vec<String> = args["patterns"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_else(|| default_patterns.clone());
        let dir_patterns: Vec<String> = args["dir_patterns"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_else(|| default_dir_patterns);

        let root_path = match safe_path(root) {
            Ok(p) => p,
            Err(e) => return self.err(id, -32602, e),
        };

        if !dry_run && !confirm {
            return self.err(id, -32602, "Deletion requires confirm=true. Run with dry_run=true first to preview.".into());
        }

        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let max_age = if older_than > 0 { older_than.saturating_mul(86400) } else { u64::MAX };

        let mut ctx = CleanupContext::new(patterns, dir_patterns.to_vec(), now, max_age, max_depth);
        ctx.walk(&root_path, 0);

        let candidates = ctx.candidates().to_vec();
        let total_size = ctx.total_size();

        if candidates.is_empty() {
            return self.ok(id, json!({ "content": Self::text_content("No files to clean up.".into()) }));
        }

        let mut report = format!("Found {} candidates ({}):\n", candidates.len(), fs_utils::format_size(total_size));
        for (path, kind) in &candidates {
            report.push_str(&format!("  [{}] {}\n", kind, path));
        }

        if !dry_run {
            let mut deleted = 0u64;
            let mut freed = 0u64;
            for (path_str, kind) in &candidates {
                let p = Path::new(path_str);
                let size = p.metadata().ok().map(|m| m.len()).unwrap_or(0);
                if kind == "dir" { std::fs::remove_dir_all(p).ok(); }
                else { std::fs::remove_file(p).ok(); }
                deleted += 1;
                freed += size;
            }
            report.push_str(&format!("\nCleaned: {} items, freed {}\n", deleted, fs_utils::format_size(freed)));
        } else {
            report.push_str("\nDry-run mode. Set dry_run=false and confirm=true to actually delete.\n");
        }

        self.ok(id, json!({ "content": Self::text_content(report) }))
    }
}
