//! Structured event logging + taxonomy (Ops Framework §5–6).

use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Canonical event names (do not invent ad-hoc strings in datapath code).
pub mod taxonomy {
    pub const RUNTIME_STARTED: &str = "runtime_started";
    pub const LINK_UP: &str = "LINK_UP";
    pub const LINK_DOWN: &str = "LINK_DOWN";
    pub const STREAM_CREATE: &str = "STREAM_CREATE";
    pub const PACKET_RX: &str = "packet_rx";
    pub const PACKET_LOSS: &str = "PACKET_LOSS";
    pub const SEQ_ERROR: &str = "SEQ_ERROR";
    pub const SEQUENCE_GAP: &str = "sequence_gap";
    pub const LATE_PACKET: &str = "late_packet";
    pub const SLOT_MISS: &str = "SLOT_MISS";
    pub const SYMBOL_TIMEOUT: &str = "SYMBOL_TIMEOUT";
    pub const BUFFER_FULL: &str = "BUFFER_FULL";
    pub const RING_STALL: &str = "RING_STALL";
    pub const KERNEL_SLOW: &str = "KERNEL_SLOW";
    pub const HEALTH_CHANGED: &str = "HEALTH_CHANGED";
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEvent {
    pub time: u64,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_us: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl LogEvent {
    pub fn now(event: impl Into<String>) -> Self {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            time,
            event: event.into(),
            component: None,
            stream: None,
            sequence: None,
            latency_us: None,
            detail: None,
        }
    }

    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.component = Some(component.into());
        self
    }

    pub fn with_stream(mut self, stream: u32) -> Self {
        self.stream = Some(stream);
        self
    }

    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = Some(sequence);
        self
    }

    pub fn with_latency_us(mut self, latency_us: f64) -> Self {
        self.latency_us = Some(latency_us);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Append-only JSONL event logger.
pub struct EventLogger {
    writer: BufWriter<File>,
    pub path: String,
}

impl EventLogger {
    pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let path_str = path.as_ref().display().to_string();
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            writer: BufWriter::new(file),
            path: path_str,
        })
    }

    pub fn emit(&mut self, event: &LogEvent) -> std::io::Result<()> {
        serde_json::to_writer(&mut self.writer, event)?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_json_line() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("aether_evt_{stamp}.jsonl"));
        let mut log = EventLogger::create(&path).unwrap();
        let ev = LogEvent::now(taxonomy::PACKET_RX)
            .with_component("transport")
            .with_stream(1)
            .with_sequence(100)
            .with_latency_us(3.2);
        log.emit(&ev).unwrap();
        log.flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("packet_rx"));
        assert!(text.contains("component"));
        let _ = std::fs::remove_file(path);
    }
}
