use crate::config::{ConnectorConfig, LoopEngineConfig};
use crate::error::{OuraError, Result};
use crate::events::{EventBus, OuraEvent};
use crate::feedback::{ClippyFeedbackCollector, ConnectorFeedbackCollector, ProfileFeedbackCollector, TestFeedbackCollector};
use crate::traits::{CommandRunner, CompositeFeedbackCollector, DefaultCommandRunner, FeedbackCollector};
use crate::types::*;
use chrono::Utc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::Notify;
use uuid::Uuid;

fn lock_state(state: &Arc<Mutex<Option<LoopState>>>) -> Result<std::sync::MutexGuard<'_, Option<LoopState>>> {
    state.lock().map_err(|e| OuraError::Internal(format!("Lock poisoned: {}", e)))
}

fn lock_value<T>(mutex: &Arc<Mutex<T>>) -> Result<std::sync::MutexGuard<'_, T>> {
    mutex.lock().map_err(|e| OuraError::Internal(format!("Lock poisoned: {}", e)))
}

pub struct LoopEngine {
    state: Arc<Mutex<Option<LoopState>>>,
    max_iterations: Arc<Mutex<u32>>,
    convergence_threshold: Arc<Mutex<f64>>,
    max_runtime_secs: Arc<Mutex<u64>>,
    stop_flag: Arc<AtomicBool>,
    stop_notify: Arc<Notify>,
    feedback_collector: Arc<dyn FeedbackCollector>,
    event_bus: EventBus,
    loop_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for LoopEngine {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.stop_notify.notify_waiters();
    }
}

impl LoopEngine {
    pub async fn shutdown(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.stop_notify.notify_waiters();
        if let Some(handle) = self.loop_handle.take() {
            tokio::select! {
                _ = handle => {},
                _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {},
            }
        }
    }
}

impl LoopEngine {
    pub fn new(config: &LoopEngineConfig, connector_config: &ConnectorConfig) -> Self {
        let mut composite = CompositeFeedbackCollector::new();
        let make_runner = |dir: &Option<String>| -> Box<dyn CommandRunner> {
            match dir {
                Some(d) => Box::new(DefaultCommandRunner::new_with_dir(std::path::PathBuf::from(d))),
                None => Box::new(DefaultCommandRunner::new()),
            }
        };
        let runner = make_runner(&config.working_directory);
        if config.feedback_sources.contains(&"test".to_string()) {
            composite.add(Box::new(TestFeedbackCollector::new(runner)));
        }
        let runner = make_runner(&config.working_directory);
        if config.feedback_sources.contains(&"lint".to_string()) {
            composite.add(Box::new(ClippyFeedbackCollector::new(runner)));
        }
        if config.feedback_sources.contains(&"profile".to_string()) {
            let collector = match &config.working_directory {
                Some(d) => ProfileFeedbackCollector::new_with_dir(std::path::PathBuf::from(d)),
                None => ProfileFeedbackCollector::new(),
            };
            composite.add(Box::new(collector));
        }
        if connector_config.enabled && !connector_config.server_url.is_empty() && !connector_config.tools.is_empty() {
            composite.add(Box::new(ConnectorFeedbackCollector::new(connector_config.clone())));
        }

        Self {
            state: Arc::new(Mutex::new(None)),
            max_iterations: Arc::new(Mutex::new(config.max_iterations)),
            convergence_threshold: Arc::new(Mutex::new(config.convergence_threshold)),
            max_runtime_secs: Arc::new(Mutex::new(config.max_runtime_secs)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            stop_notify: Arc::new(Notify::new()),
            feedback_collector: Arc::new(composite),
            event_bus: EventBus::new(),
            loop_handle: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_feedback_collector(
        max_iterations: u32,
        convergence_threshold: f64,
        feedback_collector: Arc<dyn FeedbackCollector>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(None)),
            max_iterations: Arc::new(Mutex::new(max_iterations)),
            convergence_threshold: Arc::new(Mutex::new(convergence_threshold)),
            max_runtime_secs: Arc::new(Mutex::new(3600)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            stop_notify: Arc::new(Notify::new()),
            feedback_collector,
            event_bus: EventBus::new(),
            loop_handle: None,
        }
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub async fn start(&mut self, goal: &str, manual: bool, max_iterations: Option<u32>) -> Result<LoopState> {
        if let Some(ref handle) = self.loop_handle {
            if !handle.is_finished() {
                return Err(OuraError::LoopAlreadyRunning);
            }
        }

        self.stop_flag.store(false, Ordering::SeqCst);

        {
            let state_guard = lock_state(&self.state)?;
            if let Some(ref state) = *state_guard {
                if state.status == "running" {
                    return Err(OuraError::LoopAlreadyRunning);
                }
            }
        }

        let max_iter = max_iterations.unwrap_or(*lock_value(&self.max_iterations)?);
        let threshold = *lock_value(&self.convergence_threshold)?;
        let runtime = *lock_value(&self.max_runtime_secs)?;
        let initial_state = LoopState {
            id: Uuid::new_v4().to_string(),
            goal: goal.into(),
            config: OuraConfig {
                max_iterations: max_iter,
                convergence_threshold: threshold,
                max_runtime_secs: runtime,
                ..OuraConfig::default()
            },
            current_iteration: 0,
            history: vec![],
            status: "running".into(),
            start_time: Utc::now().to_rfc3339(),
        };

        {
            let mut state_guard = lock_state(&self.state)?;
            *state_guard = Some(initial_state.clone());
        }

        self.event_bus.publish(OuraEvent::LoopStarted {
            loop_id: initial_state.id.clone(),
            goal: goal.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        });

        if manual {
            return Ok(initial_state);
        }

        let state_clone = self.state.clone();
        let max_iter_limit = max_iter;
        let threshold_clone = self.convergence_threshold.clone();
        let runtime_clone = self.max_runtime_secs.clone();
        let stop_flag_clone = self.stop_flag.clone();
        let stop_notify_clone = self.stop_notify.clone();
        let feedback_collector = self.feedback_collector.clone();
        let event_bus = self.event_bus.clone();
        let loop_id = initial_state.id.clone();

        let handle = tokio::spawn(async move {
            let loop_start = std::time::Instant::now();
            let max_runtime = match runtime_clone.lock() {
                Ok(guard) => *guard,
                Err(_) => {
                    tracing::error!("Lock poisoned in runtime");
                    return;
                }
            };

            loop {
                if stop_flag_clone.load(Ordering::SeqCst) {
                    let mut state_guard = match state_clone.lock() {
                        Ok(g) => g,
                        Err(_) => { tracing::error!("Lock poisoned"); return; }
                    };
                    if let Some(ref mut st) = *state_guard {
                        st.status = "stopped".into();
                    }
                    event_bus.publish(OuraEvent::LoopStopped {
                        loop_id: loop_id.clone(),
                        iterations: state_guard
                            .as_ref()
                            .map(|s| s.current_iteration)
                            .unwrap_or(0),
                        timestamp: Utc::now().to_rfc3339(),
                    });
                    break;
                }

                if max_runtime > 0 && loop_start.elapsed().as_secs() > max_runtime {
                    let mut state_guard = match state_clone.lock() {
                        Ok(g) => g,
                        Err(_) => { tracing::error!("Lock poisoned"); return; }
                    };
                    if let Some(ref mut st) = *state_guard {
                        st.status = "failed".into();
                    }
                    event_bus.publish(OuraEvent::Error {
                        loop_id: Some(loop_id.clone()),
                        message: "Max runtime exceeded".to_string(),
                        timestamp: Utc::now().to_rfc3339(),
                    });
                    break;
                }

                let iteration_num = {
                    let mut state_guard = match state_clone.lock() {
                        Ok(g) => g,
                        Err(_) => { tracing::error!("Lock poisoned"); return; }
                    };
                    let st = match state_guard.as_mut() {
                        Some(s) => s,
                        None => break,
                    };
                    if st.status != "running" {
                        break;
                    }
                    st.current_iteration = st.current_iteration.saturating_add(1);
                    st.current_iteration
                };

                event_bus.publish(OuraEvent::IterationStarted {
                    loop_id: loop_id.clone(),
                    iteration: iteration_num,
                    timestamp: Utc::now().to_rfc3339(),
                });

                let threshold = match threshold_clone.lock() {
                    Ok(g) => *g,
                    Err(_) => { tracing::error!("Lock poisoned in threshold"); return; }
                };

                let feedback_entries = {
                    let collect = feedback_collector.collect();
                    tokio::select! {
                        biased;
                        _ = stop_notify_clone.notified() => {
                            let mut state_guard = match state_clone.lock() {
                                Ok(g) => g,
                                Err(_) => { tracing::error!("Lock poisoned"); return; }
                            };
                            if let Some(ref mut st) = *state_guard {
                                st.status = "stopped".into();
                            }
                            event_bus.publish(OuraEvent::LoopStopped {
                                loop_id: loop_id.clone(),
                                iterations: iteration_num,
                                timestamp: Utc::now().to_rfc3339(),
                            });
                            break;
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_secs(600)) => {
                            tracing::error!("Feedback collection timed out after 600s");
                            event_bus.publish(OuraEvent::Error {
                                loop_id: Some(loop_id.clone()),
                                message: "Feedback collection timed out".to_string(),
                                timestamp: Utc::now().to_rfc3339(),
                            });
                            Vec::new()
                        }
                        result = collect => result,
                    }
                };

                event_bus.publish(OuraEvent::FeedbackCollected {
                    loop_id: loop_id.clone(),
                    iteration: iteration_num,
                    entry_count: feedback_entries.len(),
                    timestamp: Utc::now().to_rfc3339(),
                });

                let error_count = feedback_entries
                    .iter()
                    .filter(|f| f.type_ == "error")
                    .count() as f64;
                let warning_count = feedback_entries
                    .iter()
                    .filter(|f| f.type_ == "warning")
                    .count() as f64;

                let mut score = 100.0 - (error_count * 15.0) - (warning_count * 5.0);
                score = score.clamp(0.0, 100.0);

                let converged = score >= threshold;

                let status = if converged {
                    "completed"
                } else if iteration_num >= max_iter_limit {
                    "failed"
                } else {
                    "running"
                };

                let result = IterationResult {
                    iteration: iteration_num,
                    status: status.to_string(),
                    started_at: Utc::now().to_rfc3339(),
                    completed_at: Some(Utc::now().to_rfc3339()),
                    actions: vec![],
                    feedback: feedback_entries,
                    score,
                };

                event_bus.publish(OuraEvent::IterationCompleted {
                    loop_id: loop_id.clone(),
                    iteration: iteration_num,
                    score,
                    status: status.to_string(),
                    timestamp: Utc::now().to_rfc3339(),
                });

                {
                    let mut state_guard = match state_clone.lock() {
                        Ok(g) => g,
                        Err(_) => { tracing::error!("Lock poisoned in state update"); return; }
                    };
                    let st = match state_guard.as_mut() {
                        Some(s) => s,
                        None => break,
                    };
                    if converged {
                        st.status = "completed".into();
                        event_bus.publish(OuraEvent::LoopCompleted {
                            loop_id: loop_id.clone(),
                            iterations: iteration_num,
                            final_score: score,
                            timestamp: Utc::now().to_rfc3339(),
                        });
                    } else if iteration_num >= max_iter_limit {
                        st.status = "failed".into();
                    }
                    st.history.push(result);
                }

                if converged || status == "failed" {
                    break;
                }

                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                    _ = stop_notify_clone.notified() => {
                        let mut state_guard = match state_clone.lock() {
                            Ok(g) => g,
                            Err(_) => { tracing::error!("Lock poisoned in stop notify"); return; }
                        };
                        if let Some(ref mut st) = *state_guard {
                            st.status = "stopped".into();
                        }
                        event_bus.publish(OuraEvent::LoopStopped {
                            loop_id: loop_id.clone(),
                            iterations: iteration_num,
                            timestamp: Utc::now().to_rfc3339(),
                        });
                        break;
                    }
                }
            }
        });

        self.loop_handle = Some(handle);
        Ok(initial_state)
    }

    pub async fn iterate(&mut self) -> Result<IterationResult> {
        if let Some(ref handle) = self.loop_handle {
            if !handle.is_finished() {
                return Err(OuraError::BackgroundLoopRunning);
            }
        }

        let iteration_num = {
            let mut state_guard = self
                .state
                .lock()
                .map_err(|_| OuraError::Internal("Lock poisoned".to_string()))?;
            let state = state_guard.as_mut().ok_or(OuraError::NoActiveLoop)?;

            if state.status != "running" {
                return Err(OuraError::LoopNotRunning(state.status.clone()));
            }

            state.current_iteration = state.current_iteration.saturating_add(1);
            state.current_iteration
        };

        let feedback_entries = tokio::time::timeout(
            std::time::Duration::from_secs(600),
            self.feedback_collector.collect(),
        ).await.unwrap_or_else(|_| {
            tracing::error!("Feedback collection timed out during manual iteration");
            vec![]
        });

        let error_count = feedback_entries
            .iter()
            .filter(|f| f.type_ == "error")
            .count() as f64;
        let warning_count = feedback_entries
            .iter()
            .filter(|f| f.type_ == "warning")
            .count() as f64;

        let mut score = 100.0 - (error_count * 15.0) - (warning_count * 5.0);
        score = score.clamp(0.0, 100.0);

        let threshold = *lock_value(&self.convergence_threshold)?;
        let converged = score >= threshold;

        let max_iter = *lock_value(&self.max_iterations)?;

        let status = if converged {
            "completed"
        } else if iteration_num >= max_iter {
            "failed"
        } else {
            "running"
        };

        let result = IterationResult {
            iteration: iteration_num,
            status: status.to_string(),
            started_at: Utc::now().to_rfc3339(),
            completed_at: Some(Utc::now().to_rfc3339()),
            actions: vec![],
            feedback: feedback_entries,
            score,
        };

        let mut state_guard = self
            .state
            .lock()
            .map_err(|_| OuraError::Internal("Lock poisoned".to_string()))?;
        let state = state_guard.as_mut().ok_or(OuraError::NoActiveLoop)?;

        if converged {
            state.status = "completed".into();
        } else if iteration_num >= max_iter {
            state.status = "failed".into();
        }
        state.history.push(result.clone());

        Ok(result)
    }

    pub fn stop(&mut self) -> Result<u32> {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.stop_notify.notify_waiters();
        let mut guard = self.state.lock()
            .map_err(|e| OuraError::Internal(format!("Lock poisoned: {}", e)))?;
        if let Some(ref mut st) = *guard {
            st.status = "stopped".into();
            Ok(st.current_iteration)
        } else {
            Ok(0)
        }
    }

    pub fn get_state(&self) -> Result<Option<LoopState>> {
        lock_state(&self.state).map(|g| g.clone())
    }

    pub fn get_results(&self) -> Result<Vec<IterationResult>> {
        let guard = lock_state(&self.state)?;
        Ok(guard
            .as_ref()
            .map(|s| s.history.clone())
            .unwrap_or_default())
    }

    pub fn update_max_iterations(&mut self, max: u32) -> Result<()> {
        *lock_value(&self.max_iterations)? = max;
        Ok(())
    }

    pub fn update_convergence_threshold(&mut self, threshold: f64) -> Result<()> {
        *lock_value(&self.convergence_threshold)? = threshold;
        Ok(())
    }

    pub fn max_iterations(&self) -> Result<u32> {
        lock_value(&self.max_iterations).map(|g| *g)
    }

    pub fn convergence_threshold(&self) -> Result<f64> {
        lock_value(&self.convergence_threshold).map(|g| *g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConnectorConfig, LoopEngineConfig};

    fn test_config(max_iter: u32, threshold: f64) -> LoopEngineConfig {
        LoopEngineConfig {
            max_iterations: max_iter,
            convergence_threshold: threshold,
            feedback_sources: vec!["test".into(), "lint".into(), "profile".into()],
            working_directory: None,
            max_runtime_secs: 3600,
        }
    }

    #[tokio::test]
    async fn test_loop_engine_creation() {
        let engine = LoopEngine::new(&test_config(10, 95.0), &ConnectorConfig::default());
        assert!(engine.get_state().unwrap().is_none());
    }

    #[tokio::test]
    async fn test_loop_engine_start_stop() {
        let mut engine = LoopEngine::new(&test_config(5, 90.0), &ConnectorConfig::default());
        let result = engine.start("test goal", false, None).await;
        assert!(result.is_ok());

        let state = engine.get_state().unwrap();
        assert!(state.is_some());
        assert_eq!(state.unwrap().status, "running");

        let iters = engine.stop().unwrap();
        assert_eq!(iters, 0);
    }

    #[tokio::test]
    async fn test_loop_engine_update_config() {
        let mut engine = LoopEngine::new(&test_config(10, 90.0), &ConnectorConfig::default());
        engine.update_max_iterations(20).unwrap();
        engine.update_convergence_threshold(95.0).unwrap();

        assert_eq!(engine.max_iterations().unwrap(), 20);
        assert_eq!(engine.convergence_threshold().unwrap(), 95.0);
    }

    #[tokio::test]
    async fn test_loop_engine_already_running() {
        let mut engine = LoopEngine::new(&test_config(5, 90.0), &ConnectorConfig::default());
        let _ = engine.start("test goal", false, None).await;
        let result = engine.start("another goal", false, None).await;
        assert!(matches!(result, Err(OuraError::LoopAlreadyRunning)));
    }
}
