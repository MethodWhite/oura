use std::collections::HashMap;
use chrono::Utc;
use uuid::Uuid;
use crate::types::*;

pub struct LoopEngine {
    state: Option<LoopState>,
    max_iterations: u32,
    convergence_threshold: f64,
}

impl LoopEngine {
    pub fn new(max_iterations: u32, convergence_threshold: f64) -> Self {
        Self {
            state: None,
            max_iterations,
            convergence_threshold,
        }
    }

    pub fn start(&mut self, goal: &str) -> anyhow::Result<LoopState> {
        if let Some(ref state) = self.state {
            if state.status == "running" {
                anyhow::bail!("Loop already running");
            }
        }

        let state = LoopState {
            id: Uuid::new_v4().to_string(),
            goal: goal.into(),
            config: OuraConfig::default(),
            current_iteration: 0,
            history: vec![],
            status: "running".into(),
            start_time: Utc::now().to_rfc3339(),
        };

        self.state = Some(state);

        // Run the loop automatically
        self.run_loop()?;
        Ok(self.state.clone().unwrap())
    }

    fn run_loop(&mut self) -> anyhow::Result<()> {
        loop {
            let should_continue = match self.state {
                Some(ref state) => state.status == "running",
                None => break,
            };
            if !should_continue {
                break;
            }

            let result = self.iterate()?;
            if result.status == "converged" || result.status == "failed" {
                break;
            }
        }
        Ok(())
    }

    pub fn iterate(&mut self) -> anyhow::Result<IterationResult> {
        let (iteration_num, config) = {
            let state = self.state.as_mut()
                .ok_or_else(|| anyhow::anyhow!("No active loop"))?;

            if state.status != "running" {
                anyhow::bail!("Loop is not running (status: {})", state.status);
            }

            state.current_iteration += 1;
            (state.current_iteration, state.config.clone())
        };

        let mut result = IterationResult {
            iteration: iteration_num,
            status: "running".into(),
            started_at: Utc::now().to_rfc3339(),
            completed_at: None,
            actions: vec![],
            feedback: vec![],
            score: 0.0,
        };

        let actions = self.collect_actions();
        for action in actions {
            result.actions.push(action);
        }

        let feedback_entries = self.collect_feedback();
        result.feedback = feedback_entries;

        let report = self.calculate_convergence(&result, &config);
        result.score = report.score;
        result.completed_at = Some(Utc::now().to_rfc3339());

        if let Some(ref mut state) = self.state.as_mut() {
            if report.converged {
                result.status = "converged".into();
                state.status = "completed".into();
            } else if state.current_iteration >= self.max_iterations {
                result.status = "failed".into();
                state.status = "failed".into();
            }
            state.history.push(result.clone());
        }

        Ok(result)
    }

    pub fn stop(&mut self) -> u32 {
        let iter = self.state.as_ref().map(|s| s.current_iteration).unwrap_or(0);
        if let Some(ref mut state) = self.state {
            state.status = "stopped".into();
        }
        iter
    }

    pub fn get_state(&self) -> Option<LoopState> {
        self.state.clone()
    }

    pub fn get_results(&self) -> Vec<IterationResult> {
        self.state.as_ref().map(|s| s.history.clone()).unwrap_or_default()
    }

    pub fn update_max_iterations(&mut self, max: u32) {
        self.max_iterations = max;
    }

    pub fn update_convergence_threshold(&mut self, threshold: f64) {
        self.convergence_threshold = threshold;
    }

    fn collect_actions(&self) -> Vec<ActionLog> {
        vec![
            ActionLog {
                id: Uuid::new_v4().to_string(),
                agent: "test-warrior".into(),
                type_: "run_tests".into(),
                description: "Run test suite to check current state".into(),
                target: "tests".into(),
                status: "pending".into(),
                result: None,
                error: None,
                timestamp: Utc::now().to_rfc3339(),
            },
            ActionLog {
                id: Uuid::new_v4().to_string(),
                agent: "security-auditor".into(),
                type_: "security_scan".into(),
                description: "Scan for security vulnerabilities".into(),
                target: "codebase".into(),
                status: "pending".into(),
                result: None,
                error: None,
                timestamp: Utc::now().to_rfc3339(),
            },
            ActionLog {
                id: Uuid::new_v4().to_string(),
                agent: "anti-deletion".into(),
                type_: "check_integrity".into(),
                description: "Check code integrity and prevent critical function loss".into(),
                target: "critical_paths".into(),
                status: "pending".into(),
                result: None,
                error: None,
                timestamp: Utc::now().to_rfc3339(),
            },
        ]
    }

    fn collect_feedback(&self) -> Vec<FeedbackEntry> {
        let mut entries = vec![];

        // Run test feedback
        if let Ok(output) = self.run_command("cargo test 2>&1") {
            let passed = extract_number(&output, "passed");
            let failed = extract_number(&output, "failed");

            entries.push(FeedbackEntry {
                source: "tests".into(),
                type_: if failed > 0 { "error".into() } else { "success".into() },
                message: if failed > 0 {
                    format!("{} tests failed, {} passed", failed, passed)
                } else {
                    format!("All {} tests passed", passed)
                },
                details: if failed > 0 { Some(output) } else { None },
                metric: Some(if passed + failed > 0 {
                    (passed as f64 / (passed + failed) as f64) * 100.0
                } else {
                    0.0
                }),
                threshold: Some(100.0),
            });
        }

        // Run clippy feedback
        if let Ok(output) = self.run_command("cargo clippy 2>&1") {
            let warnings = output.matches("warning").count();
            let errors = output.matches("error").count();
            if warnings > 0 || errors > 0 {
                entries.push(FeedbackEntry {
                    source: "clippy".into(),
                    type_: if errors > 0 { "error".into() } else { "warning".into() },
                    message: format!("Clippy: {} warnings, {} errors", warnings, errors),
                    details: Some(output),
                    metric: Some(if errors > 0 { 0.0 } else { 100.0 }),
                    threshold: Some(100.0),
                });
            }
        }

        entries
    }

    fn run_command(&self, cmd: &str) -> Result<String, String> {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| format!("Failed to run command: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = format!("{}\n{}", stdout, stderr);

        Ok(combined)
    }

    fn calculate_convergence(&self, current: &IterationResult, config: &OuraConfig) -> ConvergenceReport {
        let mut metrics = HashMap::new();

        let error_count = current.feedback.iter().filter(|f| f.type_ == "error").count() as f64;
        let warning_count = current.feedback.iter().filter(|f| f.type_ == "warning").count() as f64;

        metrics.insert("current_score".into(), current.score);
        metrics.insert("total_errors".into(), error_count);
        metrics.insert("total_warnings".into(), warning_count);
        metrics.insert("actions_executed".into(), current.actions.len() as f64);

        let mut score = 100.0 - (error_count * 15.0) - (warning_count * 5.0);
        score = score.clamp(0.0, 100.0);

        let mut reasons = vec![];

        if let Some(state) = &self.state {
            if state.history.len() >= 3 {
                let recent: Vec<f64> = state.history.iter()
                    .rev().take(3).map(|r| r.score).collect();
                if recent.iter().all(|&s| (s - recent[0]).abs() < 0.01) {
                    reasons.push("Plateau detected: no score improvement for 3 iterations".into());
                }
            }
        }

        if score >= config.convergence_threshold {
            reasons.push(format!("Convergence score {:.1} >= threshold {:.1}", score, config.convergence_threshold));
        }

        if error_count == 0.0 && warning_count == 0.0 {
            reasons.push("No errors or warnings".into());
        }

        let converged = score >= config.convergence_threshold || (error_count == 0.0 && warning_count == 0.0);

        ConvergenceReport { converged, score, reasons, metrics }
    }
}

fn extract_number(output: &str, label: &str) -> u32 {
    let re = regex::Regex::new(&format!(r"(\d+)\s+{}", label)).ok();
    match re {
        Some(re) => re.captures(output)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0),
        None => 0,
    }
}
