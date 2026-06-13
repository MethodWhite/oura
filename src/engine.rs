use crate::error::{OuraError, Result};
use crate::events::{EventBus, OuraEvent};
use crate::feedback::{ClippyFeedbackCollector, ProfileFeedbackCollector, TestFeedbackCollector};
use crate::traits::{CompositeFeedbackCollector, DefaultCommandRunner, FeedbackCollector};
use crate::types::*;
use chrono::Utc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::Notify;
use uuid::Uuid;

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
        if let Some(handle) = self.loop_handle.take() {
            handle.abort();
        }
    }
}

impl LoopEngine {
    pub fn new(max_iterations: u32, convergence_threshold: f64) -> Self {
        let command_runner = Box::new(DefaultCommandRunner);
        let mut composite = CompositeFeedbackCollector::new();
        composite.add(Box::new(TestFeedbackCollector::new(
            Box::new(DefaultCommandRunner),
            "cargo test 2>&1".to_string(),
        )));
        composite.add(Box::new(ClippyFeedbackCollector::new(
            Box::new(DefaultCommandRunner),
            "cargo clippy 2>&1".to_string(),
        )));
        composite.add(Box::new(ProfileFeedbackCollector));

        Self {
            state: Arc::new(Mutex::new(None)),
            max_iterations: Arc::new(Mutex::new(max_iterations)),
            convergence_threshold: Arc::new(Mutex::new(convergence_threshold)),
            max_runtime_secs: Arc::new(Mutex::new(3600)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            stop_notify: Arc::new(Notify::new()),
            feedback_collector: Arc::new(composite),
            event_bus: EventBus::new(),
            loop_handle: None,
        }
    }

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

    pub async fn start(&mut self, goal: &str) -> Result<LoopState> {
        if let Some(ref handle) = self.loop_handle {
            if !handle.is_finished() {
                return Err(OuraError::LoopAlreadyRunning);
            }
        }

        self.stop_flag.store(false, Ordering::SeqCst);

        {
            let state_guard = self.state.lock().unwrap();
            if let Some(ref state) = *state_guard {
                if state.status == "running" {
                    return Err(OuraError::LoopAlreadyRunning);
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
            let mut state_guard = self.state.lock().unwrap();
            *state_guard = Some(initial_state.clone());
        }

        self.event_bus.publish(OuraEvent::LoopStarted {
            loop_id: initial_state.id.clone(),
            goal: goal.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        });

        let state_clone = self.state.clone();
        let max_iter_clone = self.max_iterations.clone();
        let threshold_clone = self.convergence_threshold.clone();
        let runtime_clone = self.max_runtime_secs.clone();
        let stop_flag_clone = self.stop_flag.clone();
        let stop_notify_clone = self.stop_notify.clone();
        let feedback_collector = self.feedback_collector.clone();
        let event_bus = self.event_bus.clone();
        let loop_id = initial_state.id.clone();

        let handle = tokio::spawn(async move {
            let loop_start = std::time::Instant::now();
            let max_runtime = *runtime_clone.lock().unwrap();

            loop {
                if stop_flag_clone.load(Ordering::SeqCst) {
                    let mut state_guard = state_clone.lock().unwrap();
                    if let Some(ref mut st) = *state_guard {
                        st.status = "stopped".into();
                    }
                    event_bus.publish(OuraEvent::LoopStopped {
                        loop_id: loop_id.clone(),
                        iterations: state_guard.as_ref().map(|s| s.current_iteration).unwrap_or(0),
                        timestamp: Utc::now().to_rfc3339(),
                    });
                    break;
                }

                if max_runtime > 0 && loop_start.elapsed().as_secs() > max_runtime {
                    let mut state_guard = state_clone.lock().unwrap();
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
                    let mut state_guard = state_clone.lock().unwrap();
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

                event_bus.publish(OuraEvent::IterationStarted {
                    loop_id: loop_id.clone(),
                    iteration: iteration_num,
                    timestamp: Utc::now().to_rfc3339(),
                });

                let max_iter = *max_iter_clone.lock().unwrap();
                let threshold = *threshold_clone.lock().unwrap();

                let feedback_entries = feedback_collector.collect().await;

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

                let has_feedback = !feedback_entries.is_empty();
                let converged =
                    has_feedback && (score >= threshold || (error_count == 0.0 && warning_count == 0.0));

                let status = if converged {
                    "converged"
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

                event_bus.publish(OuraEvent::IterationCompleted {
                    loop_id: loop_id.clone(),
                    iteration: iteration_num,
                    score,
                    status: status.to_string(),
                    timestamp: Utc::now().to_rfc3339(),
                });

                {
                    let mut state_guard = state_clone.lock().unwrap();
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
                    } else if iteration_num >= max_iter {
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
                        let mut state_guard = state_clone.lock().unwrap();
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

        let mut state_guard = self
            .state
            .lock()
            .map_err(|_| OuraError::Internal("Lock poisoned".to_string()))?;
        let state = state_guard
            .as_mut()
            .ok_or(OuraError::NoActiveLoop)?;

        if state.status != "running" {
            return Err(OuraError::LoopNotRunning(state.status.clone()));
        }

        state.current_iteration += 1;
        let iteration_num = state.current_iteration;

        let feedback_entries = self.feedback_collector.collect().await;

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

        let has_feedback = !feedback_entries.is_empty();
        let converged =
            has_feedback && (score >= *self.convergence_threshold.lock().unwrap() || (error_count == 0.0 && warning_count == 0.0));

        let max_iter = *self.max_iterations.lock().unwrap();

        let status = if converged {
            "converged"
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

        if converged {
            state.status = "completed".into();
        } else if iteration_num >= max_iter {
            state.status = "failed".into();
        }
        state.history.push(result.clone());

        Ok(result)
    }

    pub fn stop(&mut self) -> u32 {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.stop_notify.notify_waiters();
        let state_guard = self.state.lock().unwrap();
        state_guard
            .as_ref()
            .map(|s| s.current_iteration)
            .unwrap_or(0)
    }

    pub fn get_state(&self) -> Option<LoopState> {
        self.state.lock().unwrap().clone()
    }

    pub fn get_results(&self) -> Vec<IterationResult> {
        self.state
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.history.clone())
            .unwrap_or_default()
    }

    pub fn update_max_iterations(&mut self, max: u32) {
        *self.max_iterations.lock().unwrap() = max;
    }

    pub fn update_convergence_threshold(&mut self, threshold: f64) {
        *self.convergence_threshold.lock().unwrap() = threshold;
    }

    pub fn max_iterations(&self) -> u32 {
        *self.max_iterations.lock().unwrap()
    }

    pub fn convergence_threshold(&self) -> f64 {
        *self.convergence_threshold.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_loop_engine_creation() {
        let engine = LoopEngine::new(10, 95.0);
        assert!(engine.get_state().is_none());
    }

    #[tokio::test]
    async fn test_loop_engine_start_stop() {
        let mut engine = LoopEngine::new(5, 90.0);
        let result = engine.start("test goal").await;
        assert!(result.is_ok());

        let state = engine.get_state();
        assert!(state.is_some());
        assert_eq!(state.unwrap().status, "running");

        let iters = engine.stop();
        assert_eq!(iters, 0);
    }

    #[tokio::test]
    async fn test_loop_engine_update_config() {
        let mut engine = LoopEngine::new(10, 90.0);
        engine.update_max_iterations(20);
        engine.update_convergence_threshold(95.0);

        assert_eq!(engine.max_iterations(), 20);
        assert_eq!(engine.convergence_threshold(), 95.0);
    }

    #[tokio::test]
    async fn test_loop_engine_already_running() {
        let mut engine = LoopEngine::new(5, 90.0);
        let _ = engine.start("test goal").await;
        let result = engine.start("another goal").await;
        assert!(matches!(result, Err(OuraError::LoopAlreadyRunning)));
    }
}
