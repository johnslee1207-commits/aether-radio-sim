//! Cross-container network PacketIO adapter.
//!
//! Socket I/O lives only in this crate (not in protocol/transport/fpga business logic).
//! Wire format: AetherHeader (32B LE) + payload over UDP.

use aether_protocol::{decode_frame, encode_frame, ProtocolError};
use aether_types::Packet;
use cx5_emulator::PacketIO;
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetLinkConfig {
    pub version: String,
    pub id: String,
    /// Local bind address (e.g. `0.0.0.0:9000` for host, `0.0.0.0:0` for ephemeral FPGA).
    pub bind_addr: String,
    /// Peer address for TX (FPGA → host). Host RX may leave empty.
    #[serde(default)]
    pub peer_addr: String,
    /// UDP recv timeout in milliseconds (0 = blocking).
    pub recv_timeout_ms: u64,
    /// Max UDP datagram size.
    pub max_datagram_bytes: usize,
}

impl NetLinkConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, NetIoError> {
        serde_yaml::from_str(s).map_err(|e| NetIoError::Config(e.to_string()))
    }
}

#[derive(Debug, Error)]
pub enum NetIoError {
    #[error("config error: {0}")]
    Config(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("peer address required for TX")]
    MissingPeer,
}

fn resolve_one(addr: &str) -> Result<SocketAddr, NetIoError> {
    addr.to_socket_addrs()?
        .next()
        .ok_or_else(|| NetIoError::Config(format!("cannot resolve {addr}")))
}

/// FPGA-side sink: emit framed packets to the host dataplane container.
#[derive(Debug)]
pub struct NetPacketSink {
    sock: UdpSocket,
    peer: SocketAddr,
    pub sent: u64,
}

impl NetPacketSink {
    pub fn bind(cfg: &NetLinkConfig) -> Result<Self, NetIoError> {
        if cfg.peer_addr.is_empty() {
            return Err(NetIoError::MissingPeer);
        }
        let sock = UdpSocket::bind(&cfg.bind_addr)?;
        let peer = resolve_one(&cfg.peer_addr)?;
        Ok(Self {
            sock,
            peer,
            sent: 0,
        })
    }

    pub fn send_packet(&mut self, packet: &Packet) -> Result<usize, NetIoError> {
        let frame = encode_frame(packet);
        let n = self.sock.send_to(&frame, self.peer)?;
        self.sent += 1;
        Ok(n)
    }
}

/// Host-side PacketIO: receive framed UDP datagrams into an RX queue.
#[derive(Debug)]
pub struct NetPacketIO {
    sock: UdpSocket,
    peer: Option<SocketAddr>,
    max_datagram_bytes: usize,
    rx: Vec<Packet>,
    pub received: u64,
    pub decode_errors: u64,
}

impl NetPacketIO {
    pub fn bind(cfg: &NetLinkConfig) -> Result<Self, NetIoError> {
        let sock = UdpSocket::bind(&cfg.bind_addr)?;
        if cfg.recv_timeout_ms > 0 {
            sock.set_read_timeout(Some(Duration::from_millis(cfg.recv_timeout_ms)))?;
        }
        let peer = if cfg.peer_addr.is_empty() {
            None
        } else {
            Some(resolve_one(&cfg.peer_addr)?)
        };
        Ok(Self {
            sock,
            peer,
            max_datagram_bytes: cfg.max_datagram_bytes.max(64),
            rx: Vec::new(),
            received: 0,
            decode_errors: 0,
        })
    }

    /// Non-blocking-ish poll: read available datagrams until timeout/WouldBlock.
    pub fn poll_rx(&mut self, max: usize) -> Result<usize, NetIoError> {
        let mut buf = vec![0u8; self.max_datagram_bytes];
        let mut got = 0usize;
        while got < max {
            match self.sock.recv_from(&mut buf) {
                Ok((n, _src)) => match decode_frame(&buf[..n]) {
                    Ok(pkt) => {
                        self.rx.push(pkt);
                        self.received += 1;
                        got += 1;
                    }
                    Err(_) => {
                        self.decode_errors += 1;
                    }
                },
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(e) => return Err(NetIoError::Io(e)),
            }
        }
        Ok(got)
    }
}

impl PacketIO for NetPacketIO {
    fn rx_burst(&mut self, max: usize) -> Vec<Packet> {
        let _ = self.poll_rx(max);
        let n = max.min(self.rx.len());
        self.rx.drain(0..n).collect()
    }

    fn tx_burst(&mut self, packets: Vec<Packet>) -> usize {
        let Some(peer) = self.peer else {
            return 0;
        };
        let mut sent = 0usize;
        for pkt in packets {
            let frame = encode_frame(&pkt);
            if self.sock.send_to(&frame, peer).is_ok() {
                sent += 1;
            }
        }
        sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_types::{Packet, Sequence, StreamId, Timestamp};

    #[test]
    fn localhost_udp_roundtrip() {
        let host_cfg = NetLinkConfig {
            version: "1.0.0".into(),
            id: "test-host".into(),
            bind_addr: "127.0.0.1:19001".into(),
            peer_addr: String::new(),
            recv_timeout_ms: 50,
            max_datagram_bytes: 2048,
        };
        let fpga_cfg = NetLinkConfig {
            version: "1.0.0".into(),
            id: "test-fpga".into(),
            bind_addr: "127.0.0.1:0".into(),
            peer_addr: "127.0.0.1:19001".into(),
            recv_timeout_ms: 50,
            max_datagram_bytes: 2048,
        };

        let mut host = NetPacketIO::bind(&host_cfg).unwrap();
        let mut fpga = NetPacketSink::bind(&fpga_cfg).unwrap();
        let pkt = Packet::new(StreamId(1), Sequence(7), Timestamp(99), vec![9, 8, 7]);
        fpga.send_packet(&pkt).unwrap();

        // Allow one retry for scheduling jitter.
        let mut got = host.rx_burst(8);
        if got.is_empty() {
            std::thread::sleep(Duration::from_millis(20));
            got = host.rx_burst(8);
        }
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], pkt);
        assert_eq!(fpga.sent, 1);
        assert_eq!(host.received, 1);
    }
}
