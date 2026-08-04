//! Host and GPU memory backends (simulation first).

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type BufferId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryKind {
    Host,
    Gpu,
}

pub trait MemoryBackend {
    fn allocate(&mut self, size: usize, kind: MemoryKind) -> Result<BufferId, MemoryError>;
    fn write(&mut self, id: BufferId, offset: usize, data: &[u8]) -> Result<(), MemoryError>;
    fn read(&mut self, id: BufferId, offset: usize, len: usize) -> Result<Vec<u8>, MemoryError>;
    fn free(&mut self, id: BufferId) -> Result<(), MemoryError>;
}

#[derive(Debug)]
struct Buffer {
    kind: MemoryKind,
    data: Vec<u8>,
}

/// In-process simulation memory (host + GPU as separate address spaces).
#[derive(Debug, Default)]
pub struct SimMemory {
    next_id: BufferId,
    buffers: Vec<Option<Buffer>>,
}

impl SimMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn kind_of(&self, id: BufferId) -> Result<MemoryKind, MemoryError> {
        self.buffers
            .get(id as usize)
            .and_then(|b| b.as_ref())
            .map(|b| b.kind)
            .ok_or(MemoryError::NotFound(id))
    }
}

impl MemoryBackend for SimMemory {
    fn allocate(&mut self, size: usize, kind: MemoryKind) -> Result<BufferId, MemoryError> {
        let id = self.next_id;
        self.next_id += 1;
        self.buffers.push(Some(Buffer {
            kind,
            data: vec![0; size],
        }));
        Ok(id)
    }

    fn write(&mut self, id: BufferId, offset: usize, data: &[u8]) -> Result<(), MemoryError> {
        let buf = self
            .buffers
            .get_mut(id as usize)
            .and_then(|b| b.as_mut())
            .ok_or(MemoryError::NotFound(id))?;
        let end = offset
            .checked_add(data.len())
            .ok_or(MemoryError::OutOfBounds)?;
        if end > buf.data.len() {
            return Err(MemoryError::OutOfBounds);
        }
        buf.data[offset..end].copy_from_slice(data);
        Ok(())
    }

    fn read(&mut self, id: BufferId, offset: usize, len: usize) -> Result<Vec<u8>, MemoryError> {
        let buf = self
            .buffers
            .get(id as usize)
            .and_then(|b| b.as_ref())
            .ok_or(MemoryError::NotFound(id))?;
        let end = offset.checked_add(len).ok_or(MemoryError::OutOfBounds)?;
        if end > buf.data.len() {
            return Err(MemoryError::OutOfBounds);
        }
        Ok(buf.data[offset..end].to_vec())
    }

    fn free(&mut self, id: BufferId) -> Result<(), MemoryError> {
        let slot = self
            .buffers
            .get_mut(id as usize)
            .ok_or(MemoryError::NotFound(id))?;
        if slot.take().is_none() {
            return Err(MemoryError::NotFound(id));
        }
        Ok(())
    }
}

/// Capacity-limited host/GPU pools with modelled H2D/D2H copy cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryPoolConfig {
    pub version: String,
    pub id: String,
    pub host_pool_bytes: usize,
    pub gpu_pool_bytes: usize,
    pub max_buffers: usize,
    pub h2d_bandwidth_gbps: f64,
    pub d2h_bandwidth_gbps: f64,
    pub fixed_copy_latency_ns: u64,
}

impl MemoryPoolConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, MemoryError> {
        serde_yaml::from_str(s).map_err(|e| MemoryError::Config(e.to_string()))
    }
}

#[derive(Debug)]
pub struct PooledMemory {
    cfg: MemoryPoolConfig,
    inner: SimMemory,
    host_used: usize,
    gpu_used: usize,
    buffer_count: usize,
    sizes: Vec<Option<(MemoryKind, usize)>>,
    pub last_copy_ns: Option<u64>,
    pub copy_ops: u64,
}

impl PooledMemory {
    pub fn new(cfg: MemoryPoolConfig) -> Self {
        Self {
            cfg,
            inner: SimMemory::new(),
            host_used: 0,
            gpu_used: 0,
            buffer_count: 0,
            sizes: Vec::new(),
            last_copy_ns: None,
            copy_ops: 0,
        }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, MemoryError> {
        Ok(Self::new(MemoryPoolConfig::from_yaml_str(yaml)?))
    }

    pub fn host_used_bytes(&self) -> usize {
        self.host_used
    }

    pub fn gpu_used_bytes(&self) -> usize {
        self.gpu_used
    }

    fn copy_latency_ns(&self, bytes: usize, h2d: bool) -> u64 {
        let gbps = if h2d {
            self.cfg.h2d_bandwidth_gbps
        } else {
            self.cfg.d2h_bandwidth_gbps
        };
        let wire = if gbps <= 0.0 {
            0
        } else {
            ((bytes as f64) * 8.0 / (gbps * 1e9) * 1e9) as u64
        };
        wire.saturating_add(self.cfg.fixed_copy_latency_ns)
    }

    /// Copy `len` bytes from `src` into a newly allocated destination of `dst_kind`.
    pub fn copy(
        &mut self,
        src: BufferId,
        offset: usize,
        len: usize,
        dst_kind: MemoryKind,
    ) -> Result<(BufferId, u64), MemoryError> {
        let src_kind = self.inner.kind_of(src)?;
        let data = self.inner.read(src, offset, len)?;
        let dst = self.allocate(len, dst_kind)?;
        self.write(dst, 0, &data)?;
        let h2d = matches!((src_kind, dst_kind), (MemoryKind::Host, MemoryKind::Gpu));
        let d2h = matches!((src_kind, dst_kind), (MemoryKind::Gpu, MemoryKind::Host));
        let ns = if h2d {
            self.copy_latency_ns(len, true)
        } else if d2h {
            self.copy_latency_ns(len, false)
        } else {
            self.cfg.fixed_copy_latency_ns
        };
        self.last_copy_ns = Some(ns);
        self.copy_ops += 1;
        Ok((dst, ns))
    }
}

impl MemoryBackend for PooledMemory {
    fn allocate(&mut self, size: usize, kind: MemoryKind) -> Result<BufferId, MemoryError> {
        if self.buffer_count >= self.cfg.max_buffers {
            return Err(MemoryError::PoolExhausted);
        }
        match kind {
            MemoryKind::Host => {
                if self.host_used.saturating_add(size) > self.cfg.host_pool_bytes {
                    return Err(MemoryError::PoolExhausted);
                }
            }
            MemoryKind::Gpu => {
                if self.gpu_used.saturating_add(size) > self.cfg.gpu_pool_bytes {
                    return Err(MemoryError::PoolExhausted);
                }
            }
        }
        let id = self.inner.allocate(size, kind)?;
        while self.sizes.len() <= id as usize {
            self.sizes.push(None);
        }
        self.sizes[id as usize] = Some((kind, size));
        self.buffer_count += 1;
        match kind {
            MemoryKind::Host => self.host_used += size,
            MemoryKind::Gpu => self.gpu_used += size,
        }
        Ok(id)
    }

    fn write(&mut self, id: BufferId, offset: usize, data: &[u8]) -> Result<(), MemoryError> {
        self.inner.write(id, offset, data)
    }

    fn read(&mut self, id: BufferId, offset: usize, len: usize) -> Result<Vec<u8>, MemoryError> {
        self.inner.read(id, offset, len)
    }

    fn free(&mut self, id: BufferId) -> Result<(), MemoryError> {
        let (kind, size) = self
            .sizes
            .get(id as usize)
            .and_then(|s| *s)
            .ok_or(MemoryError::NotFound(id))?;
        self.inner.free(id)?;
        self.sizes[id as usize] = None;
        self.buffer_count = self.buffer_count.saturating_sub(1);
        match kind {
            MemoryKind::Host => self.host_used = self.host_used.saturating_sub(size),
            MemoryKind::Gpu => self.gpu_used = self.gpu_used.saturating_sub(size),
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryError {
    #[error("buffer not found: {0}")]
    NotFound(BufferId),
    #[error("out of bounds")]
    OutOfBounds,
    #[error("memory pool exhausted")]
    PoolExhausted,
    #[error("config error: {0}")]
    Config(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_write_read() {
        let mut mem = SimMemory::new();
        let id = mem.allocate(16, MemoryKind::Host).unwrap();
        assert_eq!(mem.kind_of(id).unwrap(), MemoryKind::Host);
        mem.write(id, 0, b"hello").unwrap();
        assert_eq!(&mem.read(id, 0, 5).unwrap(), b"hello");
        mem.free(id).unwrap();
    }

    #[test]
    fn pooled_h2d_copy_and_limit() {
        let yaml = r#"
version: "1.0.0"
id: mem-test
host_pool_bytes: 64
gpu_pool_bytes: 64
max_buffers: 8
h2d_bandwidth_gbps: 16.0
d2h_bandwidth_gbps: 16.0
fixed_copy_latency_ns: 200
"#;
        let mut mem = PooledMemory::from_yaml(yaml).unwrap();
        let host = mem.allocate(32, MemoryKind::Host).unwrap();
        mem.write(host, 0, &[1u8; 32]).unwrap();
        let (gpu, ns) = mem.copy(host, 0, 32, MemoryKind::Gpu).unwrap();
        assert!(ns >= 200);
        assert_eq!(mem.read(gpu, 0, 32).unwrap(), vec![1u8; 32]);
        assert!(matches!(
            mem.allocate(64, MemoryKind::Host),
            Err(MemoryError::PoolExhausted)
        ));
    }
}
