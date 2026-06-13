use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Instant;
use chrono::Utc;
use uuid::Uuid;
use crate::types::*;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct LoopEngine {
    state: Arc<Mutex<Option<LoopState>>>,
    max_iterations: Arc<Mutex<u32>>,
    convergence_threshold: Arc<Mutex<f64>>,
    max_runtime_secs: Arc<Mutex<u64>>,
    test_command: Arc<Mutex<String>>,
    clippy_command: Arc<Mutex<String>>,
    stop_flag: Arc<AtomicBool>,
    loop_thread: Option<thread::JoinHandle<()>>,
    loop_start: Arc<Mutex<Option<Instant>>>,
}

impl Drop for LoopEngine {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.loop_thread.take() {
            if !handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

impl LoopEngine {
    pub fn new(max_iterations: u32, convergence_threshold: f64) -> Self {
        Self {
            state: Arc::new(Mutex::new(None)),
            max_iterations: Arc::new(Mutex::new(max_iterations)),
            convergence_threshold: Arc::new(Mutex::new(convergence_threshold)),
            max_runtime_secs: Arc::new(Mutex::new(3600)),
            test_command: Arc::new(Mutex::new("cargo test 2>&1".into())),
            clippy_command: Arc::new(Mutex::new("cargo clippy 2>&1".into())),
            stop_flag: Arc::new(AtomicBool::new(false)),
            loop_thread: None,
            loop_start: Arc::new(Mutex::new(None)),
        }
    }

    pub fn configure(&mut self, max_iter: Option<u32>, threshold: Option<f64>, runtime: Option<u64>, test_cmd: Option<String>, clippy_cmd: Option<String>) {
        if let Some(v) = max_iter { *lock(&self.max_iterations) = v; }
        if let Some(v) = threshold { *lock(&self.convergence_threshold) = v; }
        if let Some(v) = runtime { *lock(&self.max_runtime_secs) = v; }
        if let Some(v) = test_cmd { *lock(&self.test_command) = v; }
        if let Some(v) = clippy_cmd { *lock(&self.clippy_command) = v; }
    }

    pub fn start(&mut self, goal: &str) -> anyhow::Result<LoopState> {
        if let Some(handle) = self.loop_thread.take() {
            if !handle.is_finished() {
                self.stop_flag.store(true, Ordering::SeqCst);
                let _ = handle.join();
            }
        }
        self.stop_flag.store(false, Ordering::SeqCst);

        {
            let state_guard = lock(&self.state);
            if let Some(ref state) = *state_guard {
                if state.status == "running" {
                    anyhow::bail!("Loop already running");
                }
            }
        }

        let initial_state = LoopState {
            id: Uuid::new_v4().to_string(),
            goal: goal.into(),
            config: OuraConfig::default(),
            current_iteration: 0,
            history: vec![],
            status: "running".into(),
            start_time: Utc::now().to_rfc3339(),
        };

        {
            let mut state_guard = lock(&self.state);
            *state_guard = Some(initial_state.clone());
        }

        *lock(&self.loop_start) = Some(Instant::now());

        let state_clone = self.state.clone();
        let max_iter_clone = self.max_iterations.clone();
        let threshold_clone = self.convergence_threshold.clone();
        let runtime_clone = self.max_runtime_secs.clone();
        let stop_flag_clone = self.stop_flag.clone();
        let test_cmd = lock(&self.test_command).clone();
        let clippy_cmd = lock(&self.clippy_command).clone();

        let handle = thread::spawn(move || {
            let loop_start = Instant::now();
            let mut result = IterationResult {
                iteration: 0,
                status: "running".into(),
                started_at: Utc::now().to_rfc3339(),
                completed_at: None,
                actions: vec![],
                feedback: vec![],
                score: 0.0,
            };

            let max_runtime = *lock(&runtime_clone);

            loop {
                if stop_flag_clone.load(Ordering::SeqCst) {
                    let mut state_guard = lock(&state_clone);
                    if let Some(ref mut st) = *state_guard {
                        st.status = "stopped".into();
                    }
                    break;
                }

                if max_runtime > 0 && loop_start.elapsed().as_secs() > max_runtime {
                    let mut state_guard = lock(&state_clone);
                    if let Some(ref mut st) = *state_guard {
                        st.status = "failed".into();
                    }
                    break;
                }

                let iteration_num = {
                    let mut state_guard = lock(&state_clone);
                    let st = match state_guard.as_mut() {
                        Some(s) => s,
                        None => break,
                    };
                    if st.status != "running" {
                        break;
                    }
                    st.current_iteration += 1;
                    st.current_iteration
                };

                let max_iter = *lock(&max_iter_clone);
                let threshold = *lock(&threshold_clone);
                result.iteration = iteration_num;

                let actions = vec![
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
                ];
                result.actions = actions;

                let feedback_entries = collect_feedback_with_commands(&test_cmd, &clippy_cmd);
                result.feedback = feedback_entries;
                result.started_at = Utc::now().to_rfc3339();

                let error_count = result.feedback.iter().filter(|f| f.type_ == "error").count() as f64;
                let warning_count = result.feedback.iter().filter(|f| f.type_ == "warning").count() as f64;

                let mut score = 100.0 - (error_count * 15.0) - (warning_count * 5.0);
                score = score.clamp(0.0, 100.0);
                result.score = score;
                result.completed_at = Some(Utc::now().to_rfc3339());

                let has_feedback = !result.feedback.is_empty();
                let converged = has_feedback && (score >= threshold || (error_count == 0.0 && warning_count == 0.0));

                {
                    let mut state_guard = lock(&state_clone);
                    let st = match state_guard.as_mut() {
                        Some(s) => s,
                        None => break,
                    };
                    if converged {
                        result.status = "converged".into();
                        st.status = "completed".into();
                    } else if iteration_num >= max_iter {
                        result.status = "failed".into();
                        st.status = "failed".into();
                    }
                    st.history.push(result.clone());
                }

                if converged || result.status == "failed" {
                    break;
                }

                thread::sleep(std::time::Duration::from_millis(100));
            }
        });

        self.loop_thread = Some(handle);
        Ok(initial_state)
    }

    pub fn iterate(&mut self) -> anyhow::Result<IterationResult> {
        if let Some(ref handle) = self.loop_thread {
            if !handle.is_finished() {
                anyhow::bail!("A background loop is running. Stop it first with oura_loop_stop, or wait for it to finish.");
            }
        }

        let mut state_guard = lock(&self.state);
        let state = state_guard.as_mut()
            .ok_or_else(|| anyhow::anyhow!("No active loop"))?;

        if state.status != "running" {
            anyhow::bail!("Loop is not running (status: {})", state.status);
        }

        state.current_iteration += 1;
        let iteration_num = state.current_iteration;

        let mut result = IterationResult {
            iteration: iteration_num,
            status: "running".into(),
            started_at: Utc::now().to_rfc3339(),
            completed_at: None,
            actions: vec![],
            feedback: vec![],
            score: 0.0,
        };

        let actions = vec![
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
        ];
        result.actions = actions;

        let max_iter = *lock(&self.max_iterations);
        let threshold = *lock(&self.convergence_threshold);

        let test_cmd = lock(&self.test_command).clone();
        let clippy_cmd = lock(&self.clippy_command).clone();
        let feedback_entries = collect_feedback_with_commands(&test_cmd, &clippy_cmd);
        result.feedback = feedback_entries;

        let error_count = result.feedback.iter().filter(|f| f.type_ == "error").count() as f64;
        let warning_count = result.feedback.iter().filter(|f| f.type_ == "warning").count() as f64;

        let mut score = 100.0 - (error_count * 15.0) - (warning_count * 5.0);
        score = score.clamp(0.0, 100.0);
        result.score = score;
        result.completed_at = Some(Utc::now().to_rfc3339());

        let has_feedback = !result.feedback.is_empty();
        let converged = has_feedback && (score >= threshold || (error_count == 0.0 && warning_count == 0.0));

        if converged {
            result.status = "converged".into();
            state.status = "completed".into();
        } else if iteration_num >= max_iter {
            result.status = "failed".into();
            state.status = "failed".into();
        }
        state.history.push(result.clone());

        Ok(result)
    }

    pub fn stop(&mut self) -> u32 {
        self.stop_flag.store(true, Ordering::SeqCst);
        let state_guard = lock(&self.state);
        let iter = state_guard.as_ref().map(|s| s.current_iteration).unwrap_or(0);
        iter
    }

    pub fn get_state(&self) -> Option<LoopState> {
        lock(&self.state).clone()
    }

    pub fn get_results(&self) -> Vec<IterationResult> {
        lock(&self.state).as_ref().map(|s| s.history.clone()).unwrap_or_default()
    }

    pub fn update_max_iterations(&mut self, max: u32) {
        *lock(&self.max_iterations) = max;
    }

    pub fn update_convergence_threshold(&mut self, threshold: f64) {
        *lock(&self.convergence_threshold) = threshold;
    }

    pub fn max_iterations(&self) -> &Arc<Mutex<u32>> {
        &self.max_iterations
    }

    pub fn convergence_threshold(&self) -> &Arc<Mutex<f64>> {
        &self.convergence_threshold
    }

    pub fn save_results(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let results = self.get_results();
        let json = serde_json::to_string_pretty(&results)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_results(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let json = std::fs::read_to_string(path)?;
        let results: Vec<IterationResult> = serde_json::from_str(&json)?;
        let mut state_guard = lock(&self.state);
        if let Some(ref mut state) = *state_guard {
            state.history = results;
        }
        Ok(())
    }
}

fn collect_feedback_static() -> Vec<FeedbackEntry> {
    collect_feedback_with_commands("cargo test 2>&1", "cargo clippy 2>&1")
}

fn collect_feedback_with_commands(test_cmd: &str, clippy_cmd: &str) -> Vec<FeedbackEntry> {
    let mut entries = vec![];

    if let Ok(output) = run_command_static(test_cmd) {
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

    if let Ok(output) = run_command_static(clippy_cmd) {
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

    let cwd = std::env::current_dir().unwrap_or_default();
    let profile_result = crate::profile::ProjectProfile::detect(&cwd);
    let dep_type = if profile_result.dependency_count > 50 { "error" }
        else if profile_result.dependency_count > 20 { "warning" }
        else { "info" };
    entries.push(FeedbackEntry {
        source: "profile".into(),
        type_: dep_type.into(),
        message: format!("Project: {} | {} deps | {} engine:{}",
            profile_result.user_type, profile_result.dependency_count,
            profile_result.ecosystem,
            if profile_result.has_game_engine { "yes" } else { "no" },
        ),
        details: Some(profile_result.summary()),
        metric: Some(profile_result.confidence * 100.0),
        threshold: None,
    });

    let verify_result = crate::profile::verify_dependencies(&cwd);
    if !verify_result.license_issues.is_empty() {
        entries.push(FeedbackEntry {
            source: "license".into(),
            type_: "warning".into(),
            message: format!("License issues: {} restricted deps", verify_result.license_issues.len()),
            details: Some(verify_result.license_issues.join("\n")),
            metric: Some(100.0 - (verify_result.license_issues.len() as f64 * 10.0).min(100.0)),
            threshold: Some(100.0),
        });
    }
    if !verify_result.version_issues.is_empty() {
        entries.push(FeedbackEntry {
            source: "versioning".into(),
            type_: "warning".into(),
            message: format!("Version issues: {} unstable deps", verify_result.version_issues.len()),
            details: Some(verify_result.version_issues.join("\n")),
            metric: Some(100.0 - (verify_result.version_issues.len() as f64 * 5.0).min(100.0)),
            threshold: Some(100.0),
        });
    }

    entries
}

fn run_command_static(cmd: &str) -> Result<String, String> {
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

fn make_extract_regex(label: &str) -> (regex::Regex, regex::Regex) {
    (
        regex::Regex::new(&format!(r"(?m)^test result:.*?(\d+)\s+{}", label)).unwrap(),
        regex::Regex::new(&format!(r"(\d+)\s+{}\b", label)).unwrap(),
    )
}

fn extract_number(output: &str, label: &str) -> u32 {
    // For the two known labels ("passed", "failed"), cache is effective
    // For other labels, compile on demand
    let (re1, re2) = match label {
        "passed" => {
            static RE: std::sync::OnceLock<(regex::Regex, regex::Regex)> = std::sync::OnceLock::new();
            RE.get_or_init(|| make_extract_regex("passed"))
        }
        "failed" => {
            static RE: std::sync::OnceLock<(regex::Regex, regex::Regex)> = std::sync::OnceLock::new();
            RE.get_or_init(|| make_extract_regex("failed"))
        }
        _ => return 0,
    };
    if let Some(caps) = re1.captures(output) {
        if let Some(m) = caps.get(1) {
            if let Ok(n) = m.as_str().parse() {
                return n;
            }
        }
    }
    if let Some(caps) = re2.captures(output) {
        if let Some(m) = caps.get(1) {
            if let Ok(n) = m.as_str().parse() {
                return n;
            }
        }
    }
    0
}
