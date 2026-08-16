//! Shared-memory SPSC ring for framed Aether packets.
//!
//! Same-host dual-process µs-oriented path. Cross-machine use UDP (`net-io`).

use aether_protocol::{decode_frame, encode_frame, ProtocolError};
use aether_types::Packet;
use cx5_emulator::PacketIO;
use memmap2::{MmapMut, MmapOptions};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use thiserror::Error;

const MAGIC: u32 = 0x314D_4853; // "SHM1" LE
const HEADER_BYTES: usize = 64;
const SLOT_STATE_EMPTY: u32 = 0;
const SLOT_STATE_FULL: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShmLinkConfig {
    pub version: String,
    pub id: String,
    pub path: String,
    pub slot_count: usize,
    pub slot_bytes: usize,
    pub create: bool,
}

impl ShmLinkConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, ShmIoError> {
        serde_yaml::from_str(s).map_err(|e| ShmIoError::Config(e.to_string()))
    }
}

#[derive(Debug, Error)]
pub enum ShmIoError {
    #[error("config error: {0}")]
    Config(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("ring full")]
    RingFull,
    #[error("bad shm magic/header")]
    BadHeader,
    #[error("frame exceeds slot capacity")]
    FrameTooLarge,
}

pub struct ShmPacketRing {
    _file: std::fs::File,
    map: MmapMut,
    slot_count: usize,
    slot_bytes: usize,
    path: PathBuf,
    pub pushed: u64,
    pub popped: u64,
}

impl ShmPacketRing {
    pub fn open(cfg: &ShmLinkConfig) -> Result<Self, ShmIoError> {
        if cfg.slot_count == 0 || cfg.slot_bytes < 64 {
            return Err(ShmIoError::Config(
                "slot_count > 0 and slot_bytes >= 64 required".into(),
            ));
        }
        let path = PathBuf::from(&cfg.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let total = HEADER_BYTES + cfg.slot_count * slot_stride(cfg.slot_bytes);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(cfg.create)
            .open(&path)?;
        if cfg.create {
            file.set_len(total as u64)?;
        } else if file.metadata()?.len() < total as u64 {
            return Err(ShmIoError::Config(format!(
                "shm file too small: {} < {total}",
                file.metadata()?.len()
            )));
        }
        let mut map = unsafe { MmapOptions::new().len(total).map_mut(&file)? };
        if cfg.create {
            init_ring(&mut map, cfg.slot_count, cfg.slot_bytes);
        } else {
            validate_header(&map, cfg.slot_count as u32, cfg.slot_bytes as u32)?;
        }
        Ok(Self {
            _file: file,
            map,
            slot_count: cfg.slot_count,
            slot_bytes: cfg.slot_bytes,
            path,
            pushed: 0,
            popped: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn write_pos(&self) -> &AtomicU64 {
        unsafe { AtomicU64::from_ptr(self.map.as_ptr().add(16) as *mut u64) }
    }

    fn read_pos(&self) -> &AtomicU64 {
        unsafe { AtomicU64::from_ptr(self.map.as_ptr().add(24) as *mut u64) }
    }

    fn slot_state(&self, idx: usize) -> &AtomicU32 {
        let base = HEADER_BYTES + idx * slot_stride(self.slot_bytes);
        unsafe { AtomicU32::from_ptr(self.map.as_ptr().add(base) as *mut u32) }
    }

    fn slot_len(&self, idx: usize) -> &AtomicU32 {
        let base = HEADER_BYTES + idx * slot_stride(self.slot_bytes) + 4;
        unsafe { AtomicU32::from_ptr(self.map.as_ptr().add(base) as *mut u32) }
    }

    fn slot_data_mut(&mut self, idx: usize) -> &mut [u8] {
        let base = HEADER_BYTES + idx * slot_stride(self.slot_bytes) + 8;
        &mut self.map[base..base + self.slot_bytes]
    }

    fn slot_data(&self, idx: usize) -> &[u8] {
        let base = HEADER_BYTES + idx * slot_stride(self.slot_bytes) + 8;
        &self.map[base..base + self.slot_bytes]
    }

    pub fn try_push(&mut self, packet: &Packet) -> Result<(), ShmIoError> {
        let frame = encode_frame(packet);
        if frame.len() > self.slot_bytes {
            return Err(ShmIoError::FrameTooLarge);
        }
        let w = self.write_pos().load(Ordering::Acquire);
        let r = self.read_pos().load(Ordering::Acquire);
        if w.wrapping_sub(r) >= self.slot_count as u64 {
            return Err(ShmIoError::RingFull);
        }
        let idx = (w as usize) % self.slot_count;
        if self.slot_state(idx).load(Ordering::Acquire) != SLOT_STATE_EMPTY {
            return Err(ShmIoError::RingFull);
        }
        self.slot_data_mut(idx)[..frame.len()].copy_from_slice(&frame);
        self.slot_len(idx)
            .store(frame.len() as u32, Ordering::Release);
        self.slot_state(idx)
            .store(SLOT_STATE_FULL, Ordering::Release);
        self.write_pos().store(w.wrapping_add(1), Ordering::Release);
        self.pushed += 1;
        Ok(())
    }

    pub fn try_pop(&mut self) -> Result<Option<Packet>, ShmIoError> {
        let r = self.read_pos().load(Ordering::Acquire);
        let w = self.write_pos().load(Ordering::Acquire);
        if r == w {
            return Ok(None);
        }
        let idx = (r as usize) % self.slot_count;
        if self.slot_state(idx).load(Ordering::Acquire) != SLOT_STATE_FULL {
            return Ok(None);
        }
        let n = self.slot_len(idx).load(Ordering::Acquire) as usize;
        let pkt = decode_frame(&self.slot_data(idx)[..n])?;
        self.slot_state(idx)
            .store(SLOT_STATE_EMPTY, Ordering::Release);
        self.read_pos().store(r.wrapping_add(1), Ordering::Release);
        self.popped += 1;
        Ok(Some(pkt))
    }
}

fn slot_stride(slot_bytes: usize) -> usize {
    8 + slot_bytes
}

fn init_ring(map: &mut MmapMut, slot_count: usize, slot_bytes: usize) {
    map.fill(0);
    map[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    map[4..8].copy_from_slice(&1u32.to_le_bytes());
    map[8..12].copy_from_slice(&(slot_count as u32).to_le_bytes());
    map[12..16].copy_from_slice(&(slot_bytes as u32).to_le_bytes());
    // write_pos / read_pos already zeroed
    let stride = slot_stride(slot_bytes);
    for i in 0..slot_count {
        let base = HEADER_BYTES + i * stride;
        let state = unsafe { AtomicU32::from_ptr(map.as_mut_ptr().add(base) as *mut u32) };
        state.store(SLOT_STATE_EMPTY, Ordering::Relaxed);
    }
}

fn validate_header(map: &MmapMut, slot_count: u32, slot_bytes: u32) -> Result<(), ShmIoError> {
    let magic = u32::from_le_bytes(map[0..4].try_into().unwrap());
    let version = u32::from_le_bytes(map[4..8].try_into().unwrap());
    let sc = u32::from_le_bytes(map[8..12].try_into().unwrap());
    let sb = u32::from_le_bytes(map[12..16].try_into().unwrap());
    if magic != MAGIC || version != 1 {
        return Err(ShmIoError::BadHeader);
    }
    if sc != slot_count || sb != slot_bytes {
        return Err(ShmIoError::Config("shm geometry mismatch vs config".into()));
    }
    Ok(())
}

pub struct ShmPacketSink {
    ring: ShmPacketRing,
}

impl ShmPacketSink {
    pub fn open(cfg: &ShmLinkConfig) -> Result<Self, ShmIoError> {
        // Respect `cfg.create`. Forcing create=true here truncated an existing ring
        // while host-recv still held an mmap (Bus error on Linux/WSL dual-process).
        Ok(Self {
            ring: ShmPacketRing::open(cfg)?,
        })
    }

    pub fn send_packet(&mut self, packet: &Packet) -> Result<(), ShmIoError> {
        for _ in 0..10_000 {
            match self.ring.try_push(packet) {
                Ok(()) => return Ok(()),
                Err(ShmIoError::RingFull) => std::hint::spin_loop(),
                Err(e) => return Err(e),
            }
        }
        Err(ShmIoError::RingFull)
    }

    pub fn pushed(&self) -> u64 {
        self.ring.pushed
    }
}

pub struct ShmPacketIO {
    ring: ShmPacketRing,
    rx: Vec<Packet>,
}

impl ShmPacketIO {
    pub fn open(cfg: &ShmLinkConfig) -> Result<Self, ShmIoError> {
        let mut c = cfg.clone();
        c.create = false;
        Ok(Self {
            ring: ShmPacketRing::open(&c)?,
            rx: Vec::new(),
        })
    }

    pub fn poll_rx(&mut self, max: usize) -> Result<usize, ShmIoError> {
        let mut n = 0;
        while n < max {
            match self.ring.try_pop()? {
                Some(pkt) => {
                    self.rx.push(pkt);
                    n += 1;
                }
                None => break,
            }
        }
        Ok(n)
    }

    pub fn popped(&self) -> u64 {
        self.ring.popped
    }
}

impl PacketIO for ShmPacketIO {
    fn rx_burst(&mut self, max: usize) -> Vec<Packet> {
        let _ = self.poll_rx(max);
        let n = max.min(self.rx.len());
        self.rx.drain(0..n).collect()
    }

    fn tx_burst(&mut self, _packets: Vec<Packet>) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_types::{Packet, Sequence, StreamId, Timestamp};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn spsc_same_process_roundtrip() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("aether_shm_{stamp}.bin"));
        let cfg = ShmLinkConfig {
            version: "1.0.0".into(),
            id: "test".into(),
            path: path.to_string_lossy().into(),
            slot_count: 8,
            slot_bytes: 1024,
            create: true,
        };
        let mut prod = ShmPacketSink::open(&cfg).unwrap();
        let mut cons_cfg = cfg.clone();
        cons_cfg.create = false;
        let mut cons = ShmPacketIO::open(&cons_cfg).unwrap();

        for i in 0..5u64 {
            let pkt = Packet::new(
                StreamId(1),
                Sequence(i),
                Timestamp(i * 10),
                vec![i as u8; 16],
            );
            prod.send_packet(&pkt).unwrap();
        }
        let got = cons.rx_burst(8);
        assert_eq!(got.len(), 5);
        assert_eq!(got[4].sequence.0, 4);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn open_sink_without_create_does_not_truncate_prepared_ring() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("aether_shm_ntruc_{stamp}.bin"));
        let mut prep = ShmLinkConfig {
            version: "1.0.0".into(),
            id: "test".into(),
            path: path.to_string_lossy().into(),
            slot_count: 8,
            slot_bytes: 1024,
            create: true,
        };
        let _ = ShmPacketSink::open(&prep).unwrap();
        let len_after_prep = std::fs::metadata(&path).unwrap().len();

        prep.create = false;
        let mut sink = ShmPacketSink::open(&prep).unwrap();
        let len_after_reopen = std::fs::metadata(&path).unwrap().len();
        assert_eq!(len_after_prep, len_after_reopen);

        let pkt = Packet::new(StreamId(1), Sequence(0), Timestamp(0), vec![9; 8]);
        sink.send_packet(&pkt).unwrap();
        let mut cons = ShmPacketIO::open(&prep).unwrap();
        assert_eq!(cons.rx_burst(1).len(), 1);
        let _ = std::fs::remove_file(path);
    }
}
