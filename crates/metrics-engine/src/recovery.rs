//! Recovery policy executor (Ops Framework §9).

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

use crate::{taxonomy, EventLogger, LogEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    MarkInvalidContinue,
    RecoverSequence,
    DropOldestResyncSlot,
    RestartStream,
    AlertAndContinue,
    RecordAndDrop,
    Unknown,
}

impl RecoveryAction {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "mark_invalid_continue" => Self::MarkInvalidContinue,
            "recover_sequence" => Self::RecoverSequence,
            "drop_oldest_resync_slot" => Self::DropOldestResyncSlot,
            "restart_stream" => Self::RestartStream,
            "alert_and_continue" => Self::AlertAndContinue,
            "record_and_drop" => Self::RecordAndDrop,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MarkInvalidContinue => "mark_invalid_continue",
            Self::RecoverSequence => "recover_sequence",
            Self::DropOldestResyncSlot => "drop_oldest_resync_slot",
            Self::RestartStream => "restart_stream",
            Self::AlertAndContinue => "alert_and_continue",
            Self::RecordAndDrop => "record_and_drop",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ActionEntry {
    action: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RecoveryPolicyFile {
    #[allow(dead_code)]
    pub version: String,
    pub id: String,
    pub actions: HashMap<String, ActionEntry>,
}

#[derive(Debug, Clone)]
pub struct RecoveryPolicy {
    pub id: String,
    actions: HashMap<String, RecoveryAction>,
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("config error: {0}")]
    Config(String),
}

impl RecoveryPolicy {
    pub fn from_yaml_str(s: &str) -> Result<Self, RecoveryError> {
        let file: RecoveryPolicyFile =
            serde_yaml::from_str(s).map_err(|e| RecoveryError::Config(e.to_string()))?;
        let actions = file
            .actions
            .into_iter()
            .map(|(k, v)| (k, RecoveryAction::parse(&v.action)))
            .collect();
        Ok(Self {
            id: file.id,
            actions,
        })
    }

    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, RecoveryError> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| RecoveryError::Config(format!("read: {e}")))?;
        Self::from_yaml_str(&text)
    }

    pub fn action_for(&self, fault_class: &str) -> RecoveryAction {
        self.actions
            .get(fault_class)
            .copied()
            .unwrap_or(RecoveryAction::Unknown)
    }
}

/// Records applied recovery actions; datapath callers decide how to execute.
#[derive(Debug)]
pub struct RecoveryExecutor {
    policy: RecoveryPolicy,
    pub applied: Vec<(String, RecoveryAction)>,
}

impl RecoveryExecutor {
    pub fn new(policy: RecoveryPolicy) -> Self {
        Self {
            policy,
            applied: Vec::new(),
        }
    }

    pub fn policy_id(&self) -> &str {
        &self.policy.id
    }

    pub fn apply(&mut self, fault_class: &str, events: Option<&mut EventLogger>) -> RecoveryAction {
        let action = self.policy.action_for(fault_class);
        self.applied.push((fault_class.to_string(), action));
        if let Some(log) = events {
            let _ = log.emit(
                &LogEvent::now(taxonomy::HEALTH_CHANGED)
                    .with_component("recovery")
                    .with_detail(format!("{fault_class} -> {}", action.as_str())),
            );
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_repo_recovery_policy() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let policy =
            RecoveryPolicy::load_path(root.join("configs/ops/recovery_policy.yaml")).unwrap();
        assert_eq!(
            policy.action_for("sequence_gap"),
            RecoveryAction::RecoverSequence
        );
        let mut ex = RecoveryExecutor::new(policy);
        assert_eq!(ex.apply("late_packet", None), RecoveryAction::RecordAndDrop);
        assert_eq!(ex.applied.len(), 1);
    }
}
