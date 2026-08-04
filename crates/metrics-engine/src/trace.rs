//! Packet/slot stage tracing (Ops Framework §7). Hot path: fixed ring only.

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TraceStage {
    FpgaTx,
    WireDepart,
    Cx5Rx,
    DmaDone,
    HostReady,
    GpuEnqueue,
    CudaStart,
    CudaDone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageStamp {
    pub stage: TraceStage,
    pub time_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketTrace {
    pub trace_id: u64,
    pub stream_id: u32,
    pub sequence: u64,
    pub stages: Vec<StageStamp>,
}

/// In-memory SPSC-friendly ring of packet traces; optional JSONL export.
pub struct TraceEngine {
    capacity: usize,
    traces: Vec<PacketTrace>,
    next_id: u64,
    enabled: bool,
    export_path: Option<String>,
    dropped: u64,
}

impl TraceEngine {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            traces: Vec::with_capacity(capacity.min(1024)),
            next_id: 1,
            enabled: true,
            export_path: None,
            dropped: 0,
        }
    }

    pub fn disabled() -> Self {
        let mut t = Self::new(1);
        t.enabled = false;
        t
    }

    pub fn from_config(enabled: bool, capacity: usize, export_path: impl Into<String>) -> Self {
        let mut eng = Self::new(capacity);
        eng.enabled = enabled;
        eng.export_path = Some(export_path.into());
        eng
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn start(&mut self, stream_id: u32, sequence: u64, first: TraceStage, time_ns: u64) -> u64 {
        if !self.enabled {
            return 0;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let trace = PacketTrace {
            trace_id: id,
            stream_id,
            sequence,
            stages: vec![StageStamp {
                stage: first,
                time_ns,
            }],
        };
        if self.traces.len() >= self.capacity {
            self.traces.remove(0);
            self.dropped += 1;
        }
        self.traces.push(trace);
        id
    }

    pub fn stamp(&mut self, trace_id: u64, stage: TraceStage, time_ns: u64) {
        if !self.enabled || trace_id == 0 {
            return;
        }
        if let Some(t) = self
            .traces
            .iter_mut()
            .rev()
            .find(|t| t.trace_id == trace_id)
        {
            t.stages.push(StageStamp { stage, time_ns });
        }
    }

    pub fn latest(&self) -> Option<&PacketTrace> {
        self.traces.last()
    }

    pub fn len(&self) -> usize {
        self.traces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.traces.is_empty()
    }

    /// Flush all buffered traces to JSONL (ops path; not on hot poll).
    pub fn export_jsonl(&self, path: impl AsRef<Path>) -> std::io::Result<usize> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let mut w = BufWriter::new(file);
        for t in &self.traces {
            serde_json::to_writer(&mut w, t)?;
            w.write_all(b"\n")?;
        }
        w.flush()?;
        Ok(self.traces.len())
    }

    pub fn export_configured(&self) -> std::io::Result<usize> {
        match &self.export_path {
            Some(p) if self.enabled => self.export_jsonl(p),
            _ => Ok(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_records_stages() {
        let mut eng = TraceEngine::new(8);
        let id = eng.start(1, 10, TraceStage::FpgaTx, 1000);
        eng.stamp(id, TraceStage::Cx5Rx, 1500);
        eng.stamp(id, TraceStage::CudaDone, 3500);
        let t = eng.latest().unwrap();
        assert_eq!(t.stages.len(), 3);
        assert_eq!(t.sequence, 10);
    }
}
