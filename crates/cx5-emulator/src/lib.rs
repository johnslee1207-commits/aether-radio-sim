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
    #[error("mbuf pool exhausted")]
    MbufExhausted,
}

/// DPDK-shaped `PacketIO` loaded from `configs/backends/dpdk.yaml`.
///
/// - `backend: mock` — in-process mbuf/burst simulation (no libdpdk)
/// - `backend: hardware` — returns [`Cx5Error::BackendUnavailable`] until a
///   dedicated adapter crate is compiled in
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DpdkBackendConfig {
    pub version: String,
    pub id: String,
    #[serde(default = "default_dpdk_backend")]
    pub backend: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub pci_address: String,
    #[serde(default = "default_one")]
    pub rx_queues: usize,
    #[serde(default = "default_one")]
    pub tx_queues: usize,
    #[serde(default = "default_mbuf_pool")]
    pub mbuf_pool_size: usize,
    #[serde(default = "default_burst")]
    pub burst_size: usize,
    #[serde(default)]
    pub poll_cost_ns: u64,
    #[serde(default)]
    pub hugepage_mb: u64,
}

fn default_dpdk_backend() -> String {
    "mock".into()
}
fn default_one() -> usize {
    1
}
fn default_mbuf_pool() -> usize {
    1024
}
fn default_burst() -> usize {
    32
}

impl DpdkBackendConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, Cx5Error> {
        serde_yaml::from_str(s).map_err(|e| Cx5Error::Config(e.to_string()))
    }
}

/// Mock / unavailable DPDK PacketIO (never links libdpdk).
#[derive(Debug)]
pub struct DpdkPacketIO {
    cfg: DpdkBackendConfig,
    rx: Vec<Packet>,
    tx: Vec<Packet>,
    mbufs_in_use: usize,
    pub poll_cycles: u64,
    pub last_poll_cost_ns: u64,
}

impl DpdkPacketIO {
    pub fn open_yaml(yaml: &str) -> Result<Self, Cx5Error> {
        let cfg = DpdkBackendConfig::from_yaml_str(yaml)?;
        Self::open_config(cfg)
    }

    pub fn open_config(cfg: DpdkBackendConfig) -> Result<Self, Cx5Error> {
        let mode = cfg.backend.to_ascii_lowercase();
        if matches!(mode.as_str(), "hardware" | "libdpdk" | "real") || !cfg.enabled {
            return Err(Cx5Error::BackendUnavailable(format!(
                "DPDK backend={mode} enabled={} is not available in this build; use backend=mock",
                cfg.enabled
            )));
        }
        if !matches!(mode.as_str(), "mock" | "simulation" | "sim") {
            return Err(Cx5Error::Config(format!("unknown DPDK backend '{mode}'")));
        }
        Ok(Self {
            cfg,
            rx: Vec::new(),
            tx: Vec::new(),
            mbufs_in_use: 0,
            poll_cycles: 0,
            last_poll_cost_ns: 0,
        })
    }

    /// Path-based open used by adapters; reads YAML from disk.
    pub fn try_open(config_path: &str) -> Result<Self, Cx5Error> {
        let yaml = std::fs::read_to_string(config_path)
            .map_err(|e| Cx5Error::Config(format!("read {config_path}: {e}")))?;
        Self::open_yaml(&yaml)
    }

    pub fn inject_rx(&mut self, packets: impl IntoIterator<Item = Packet>) -> Result<(), Cx5Error> {
        for p in packets {
            if self.mbufs_in_use >= self.cfg.mbuf_pool_size {
                return Err(Cx5Error::MbufExhausted);
            }
            self.rx.push(p);
            self.mbufs_in_use += 1;
        }
        Ok(())
    }

    pub fn mbufs_in_use(&self) -> usize {
        self.mbufs_in_use
    }

    pub fn mbuf_pool_size(&self) -> usize {
        self.cfg.mbuf_pool_size
    }

    pub fn burst_size(&self) -> usize {
        self.cfg.burst_size
    }

    pub fn config(&self) -> &DpdkBackendConfig {
        &self.cfg
    }
}

impl PacketIO for DpdkPacketIO {
    fn rx_burst(&mut self, max: usize) -> Vec<Packet> {
        self.poll_cycles += 1;
        self.last_poll_cost_ns = self.cfg.poll_cost_ns;
        let n = max.min(self.cfg.burst_size).min(self.rx.len());
        let out: Vec<_> = self.rx.drain(0..n).collect();
        self.mbufs_in_use = self.mbufs_in_use.saturating_sub(out.len());
        out
    }

    fn tx_burst(&mut self, packets: Vec<Packet>) -> usize {
        self.poll_cycles += 1;
        self.last_poll_cost_ns = self.cfg.poll_cost_ns;
        let room = self
            .cfg
            .mbuf_pool_size
            .saturating_sub(self.mbufs_in_use)
            .min(self.cfg.burst_size)
            .min(packets.len());
        let taken: Vec<_> = packets.into_iter().take(room).collect();
        let n = taken.len();
        self.mbufs_in_use += n;
        self.tx.extend(taken);
        // TX completion returns mbufs to the pool in this mock.
        self.mbufs_in_use = self.mbufs_in_use.saturating_sub(n);
        n
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

    const DPDK_MOCK: &str = r#"
version: "1.0.0"
id: dpdk-mock
backend: mock
enabled: true
rx_queues: 1
tx_queues: 1
mbuf_pool_size: 4
burst_size: 2
poll_cost_ns: 10
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
    fn dpdk_hardware_is_unavailable() {
        let yaml = r#"
version: "1.0.0"
id: dpdk-hw
backend: hardware
enabled: true
"#;
        assert!(matches!(
            DpdkPacketIO::open_yaml(yaml),
            Err(Cx5Error::BackendUnavailable(_))
        ));
    }

    #[test]
    fn dpdk_mock_burst_and_mbuf_limit() {
        let mut io = DpdkPacketIO::open_yaml(DPDK_MOCK).unwrap();
        assert_eq!(io.burst_size(), 2);
        for i in 0..4 {
            io.inject_rx([Packet::new(
                StreamId(1),
                Sequence(i),
                Timestamp(0),
                vec![i as u8],
            )])
            .unwrap();
        }
        assert!(matches!(
            io.inject_rx([Packet::new(StreamId(1), Sequence(9), Timestamp(0), vec![9])]),
            Err(Cx5Error::MbufExhausted)
        ));
        let first = io.rx_burst(8);
        assert_eq!(first.len(), 2);
        assert_eq!(io.last_poll_cost_ns, 10);
        let second = io.rx_burst(8);
        assert_eq!(second.len(), 2);
        assert_eq!(io.tx_burst(first), 2);
    }
}
