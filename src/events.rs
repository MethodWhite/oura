use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OuraEvent {
    LoopStarted {
        loop_id: String,
        goal: String,
        timestamp: String,
    },
    LoopStopped {
        loop_id: String,
        iterations: u32,
        timestamp: String,
    },
    LoopCompleted {
        loop_id: String,
        iterations: u32,
        final_score: f64,
        timestamp: String,
    },
    IterationStarted {
        loop_id: String,
        iteration: u32,
        timestamp: String,
    },
    IterationCompleted {
        loop_id: String,
        iteration: u32,
        score: f64,
        status: String,
        timestamp: String,
    },
    FeedbackCollected {
        loop_id: String,
        iteration: u32,
        entry_count: usize,
        timestamp: String,
    },
    Error {
        loop_id: Option<String>,
        message: String,
        timestamp: String,
    },
}

impl OuraEvent {
    pub fn timestamp(&self) -> &str {
        match self {
            OuraEvent::LoopStarted { timestamp, .. } => timestamp,
            OuraEvent::LoopStopped { timestamp, .. } => timestamp,
            OuraEvent::LoopCompleted { timestamp, .. } => timestamp,
            OuraEvent::IterationStarted { timestamp, .. } => timestamp,
            OuraEvent::IterationCompleted { timestamp, .. } => timestamp,
            OuraEvent::FeedbackCollected { timestamp, .. } => timestamp,
            OuraEvent::Error { timestamp, .. } => timestamp,
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<OuraEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self { sender }
    }

    pub fn publish(&self, event: OuraEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OuraEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EventLogger {
    receiver: broadcast::Receiver<OuraEvent>,
}

impl EventLogger {
    pub fn new(event_bus: &EventBus) -> Self {
        Self {
            receiver: event_bus.subscribe(),
        }
    }

    pub async fn run(mut self) {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    let event_type = format!("{:?}", std::mem::discriminant(&event));
                    tracing::info!(event_type, "{}", format_event(&event));
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    }
}

fn format_event(event: &OuraEvent) -> String {
    match event {
        OuraEvent::LoopStarted { loop_id, goal, .. } => {
            format!("Loop {} started: {}", loop_id, goal)
        }
        OuraEvent::LoopStopped { loop_id, iterations, .. } => {
            format!("Loop {} stopped after {} iterations", loop_id, iterations)
        }
        OuraEvent::LoopCompleted { loop_id, iterations, final_score, .. } => {
            format!("Loop {} completed: {} iterations, score {:.1}", loop_id, iterations, final_score)
        }
        OuraEvent::IterationStarted { loop_id, iteration, .. } => {
            format!("Loop {} iteration {} started", loop_id, iteration)
        }
        OuraEvent::IterationCompleted { loop_id, iteration, score, status, .. } => {
            format!("Loop {} iteration {} completed: score {:.1}, status {}", loop_id, iteration, score, status)
        }
        OuraEvent::FeedbackCollected { loop_id, iteration, entry_count, .. } => {
            format!("Loop {} iteration {} collected {} feedback entries", loop_id, iteration, entry_count)
        }
        OuraEvent::Error { loop_id, message, .. } => {
            if let Some(id) = loop_id {
                format!("Loop {} error: {}", id, message)
            } else {
                format!("Error: {}", message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new();
        let mut receiver = bus.subscribe();

        let event = OuraEvent::LoopStarted {
            loop_id: "test-123".to_string(),
            goal: "test goal".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        bus.publish(event);

        let received = receiver.recv().await.unwrap();
        match received {
            OuraEvent::LoopStarted { loop_id, .. } => {
                assert_eq!(loop_id, "test-123");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new();
        let mut receiver1 = bus.subscribe();
        let mut receiver2 = bus.subscribe();

        let event = OuraEvent::IterationCompleted {
            loop_id: "test-123".to_string(),
            iteration: 1,
            score: 95.0,
            status: "converged".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        bus.publish(event);

        let received1 = receiver1.recv().await.unwrap();
        let received2 = receiver2.recv().await.unwrap();

        match received1 {
            OuraEvent::IterationCompleted { score, .. } => assert_eq!(score, 95.0),
            _ => panic!("Wrong event type"),
        }

        match received2 {
            OuraEvent::IterationCompleted { score, .. } => assert_eq!(score, 95.0),
            _ => panic!("Wrong event type"),
        }
    }
}
