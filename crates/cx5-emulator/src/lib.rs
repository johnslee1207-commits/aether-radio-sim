//! CX5 queue and DMA model interfaces (simulation backend).

use aether_types::Packet;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NicDmaConfig {
    pub version: String,
    pub id: String,
    pub nic_dma_latency_us: f64,
    pub rx_queue_depth: usize,
    pub tx_queue_depth: usize,
    pub completion_queue_depth: usize,
}

impl NicDmaConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, Cx5Error> {
        serde_yaml::from_str(s).map_err(|e| Cx5Error::Config(e.to_string()))
    }

    pub fn dma_latency_ns(&self) -> u64 {
        (self.nic_dma_latency_us * 1_000.0) as u64
    }
}

/// Packet I/O abstraction — never bind DPDK directly in business code.
pub trait PacketIO {
    fn rx_burst(&mut self, max: usize) -> Vec<Packet>;
    fn tx_burst(&mut self, packets: Vec<Packet>) -> usize;
}

#[derive(Debug, Default)]
pub struct SimPacketIO {
    pub rx: Vec<Packet>,
    pub tx: Vec<Packet>,
}

impl PacketIO for SimPacketIO {
    fn rx_burst(&mut self, max: usize) -> Vec<Packet> {
        let n = max.min(self.rx.len());
        self.rx.drain(0..n).collect()
    }

    fn tx_burst(&mut self, packets: Vec<Packet>) -> usize {
        let n = packets.len();
        self.tx.extend(packets);
        n
    }
}

#[derive(Debug, Clone)]
struct PendingRx {
    ready_at_ns: u64,
    packet: Packet,
}

/// ConnectX-5 style NIC: RX → PCIe DMA delay → host-visible completion.
#[derive(Debug)]
pub struct Cx5Nic {
    cfg: NicDmaConfig,
    now_ns: u64,
    pending: Vec<PendingRx>,
    rx_ready: Vec<Packet>,
    tx: Vec<Packet>,
    pub completions: u64,
}

impl Cx5Nic {
    pub fn new(cfg: NicDmaConfig) -> Self {
        Self {
            cfg,
            now_ns: 0,
            pending: Vec::new(),
            rx_ready: Vec::new(),
            tx: Vec::new(),
            completions: 0,
        }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, Cx5Error> {
        Ok(Self::new(NicDmaConfig::from_yaml_str(yaml)?))
    }

    pub fn dma_latency_ns(&self) -> u64 {
        self.cfg.dma_latency_ns()
    }

    pub fn advance_time(&mut self, now_ns: u64) {
        self.now_ns = now_ns;
        self.poll_completions();
    }

    pub fn now_ns(&self) -> u64 {
        self.now_ns
    }

    /// Submit a packet into the RX DMA pipeline.
    pub fn submit_rx(&mut self, packet: Packet) -> Result<(), Cx5Error> {
        let ready_at_ns = self.now_ns.saturating_add(self.cfg.dma_latency_ns());
        self.submit_rx_ready_at(packet, ready_at_ns)
    }

    /// Submit with an explicit completion time (ns).
    pub fn submit_rx_ready_at(&mut self, packet: Packet, ready_at_ns: u64) -> Result<(), Cx5Error> {
        if self.pending.len() + self.rx_ready.len() >= self.cfg.rx_queue_depth {
            return Err(Cx5Error::QueueFull);
        }
        self.pending.push(PendingRx {
            ready_at_ns,
            packet,
        });
        self.poll_completions();
        Ok(())
    }

    fn poll_completions(&mut self) {
        let now = self.now_ns;
        let mut still = Vec::new();
        for p in self.pending.drain(..) {
            if p.ready_at_ns <= now {
                self.rx_ready.push(p.packet);
                self.completions += 1;
            } else {
                still.push(p);
            }
        }
        self.pending = still;
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn ready_count(&self) -> usize {
        self.rx_ready.len()
    }
}

impl PacketIO for Cx5Nic {
    fn rx_burst(&mut self, max: usize) -> Vec<Packet> {
        self.poll_completions();
        let n = max.min(self.rx_ready.len());
        self.rx_ready.drain(0..n).collect()
    }

    fn tx_burst(&mut self, packets: Vec<Packet>) -> usize {
        let n = packets.len();
        if self.tx.len() + n > self.cfg.tx_queue_depth {
            let room = self.cfg.tx_queue_depth.saturating_sub(self.tx.len());
            self.tx.extend(packets.into_iter().take(room));
            return room;
        }
        self.tx.extend(packets);
        n
    }
}

#[derive(Debug, Error)]
pub enum Cx5Error {
    #[error("config error: {0}")]
    Config(String),
    #[error("queue full")]
    QueueFull,
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),
}

/// Placeholder for a future DPDK `PacketIO` backend.
///
/// This type intentionally does **not** link libdpdk. Business crates keep using
/// `PacketIO`; enable a real DPDK adapter in a dedicated crate/feature later.
#[derive(Debug, Default)]
pub struct DpdkPacketIO;

impl DpdkPacketIO {
    pub fn try_open(_config_path: &str) -> Result<Self, Cx5Error> {
        Err(Cx5Error::BackendUnavailable(
            "DPDK PacketIO is not compiled in; use SimPacketIO, NetPacketIO, or ShmPacketIO".into(),
        ))
    }
}

impl PacketIO for DpdkPacketIO {
    fn rx_burst(&mut self, _max: usize) -> Vec<Packet> {
        Vec::new()
    }

    fn tx_burst(&mut self, _packets: Vec<Packet>) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_types::{Packet, Sequence, StreamId, Timestamp};

    const YAML: &str = r#"
version: "1.0.0"
id: cx5-dma-model
nic_dma_latency_us: 2.0
rx_queue_depth: 8
tx_queue_depth: 8
completion_queue_depth: 8
"#;

    #[test]
    fn sim_packet_io_burst() {
        let mut io = SimPacketIO::default();
        io.rx
            .push(Packet::new(StreamId(1), Sequence(0), Timestamp(0), vec![1]));
        let got = io.rx_burst(8);
        assert_eq!(got.len(), 1);
        assert_eq!(io.tx_burst(got), 1);
    }

    #[test]
    fn dma_delay_before_rx_visible() {
        let mut nic = Cx5Nic::from_yaml(YAML).unwrap();
        assert_eq!(nic.dma_latency_ns(), 2_000);
        nic.advance_time(0);
        nic.submit_rx(Packet::new(StreamId(1), Sequence(0), Timestamp(0), vec![7]))
            .unwrap();
        assert!(nic.rx_burst(8).is_empty());
        assert_eq!(nic.pending_count(), 1);
        nic.advance_time(2_000);
        let got = nic.rx_burst(8);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].payload, vec![7]);
        assert_eq!(nic.completions, 1);
    }

    #[test]
    fn dpdk_stub_is_unavailable() {
        assert!(matches!(
            DpdkPacketIO::try_open("configs/backends/dpdk.yaml"),
            Err(Cx5Error::BackendUnavailable(_))
        ));
    }
}
