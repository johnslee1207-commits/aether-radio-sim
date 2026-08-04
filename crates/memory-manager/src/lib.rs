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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryError {
    #[error("buffer not found: {0}")]
    NotFound(BufferId),
    #[error("out of bounds")]
    OutOfBounds,
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
}
