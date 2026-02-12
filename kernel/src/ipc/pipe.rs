//! Named Pipes for IPC

use crate::sync::IrqMutex;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

extern crate alloc;

const PIPE_BUFFER_SIZE: usize = 4096;

/// Pipe object
pub struct Pipe {
    buffer: VecDeque<u8>,
    readers: Vec<crate::process::Tid>,
    writers: Vec<crate::process::Tid>,
}

impl Pipe {
    pub fn new() -> Self {
        Pipe {
            buffer: VecDeque::with_capacity(PIPE_BUFFER_SIZE),
            readers: Vec::new(),
            writers: Vec::new(),
        }
    }
    
    /// Write to pipe
    pub fn write(&mut self, data: &[u8]) -> usize {
        let available = PIPE_BUFFER_SIZE - self.buffer.len();
        let to_write = core::cmp::min(data.len(), available);
        
        for &byte in &data[..to_write] {
            self.buffer.push_back(byte);
        }
        
        // Wake up readers
        for tid in self.readers.drain(..) {
            crate::process::scheduler::ready(tid);
        }
        
        to_write
    }
    
    /// Read from pipe
    pub fn read(&mut self, buffer: &mut [u8]) -> usize {
        let to_read = core::cmp::min(buffer.len(), self.buffer.len());
        
        for i in 0..to_read {
            buffer[i] = self.buffer.pop_front().unwrap();
        }
        
        // Wake up writers
        for tid in self.writers.drain(..) {
            crate::process::scheduler::ready(tid);
        }
        
        to_read
    }
    
    /// Check if pipe has data
    pub fn has_data(&self) -> bool {
        !self.buffer.is_empty()
    }
}

static PIPES: IrqMutex<BTreeMap<u32, Pipe>> = IrqMutex::new(BTreeMap::new());
static NEXT_PIPE_ID: IrqMutex<u32> = IrqMutex::new(1);

/// Create new pipe
pub fn create_pipe() -> u32 {
    let mut next_id = NEXT_PIPE_ID.lock();
    let id = *next_id;
    *next_id += 1;
    drop(next_id);
    
    PIPES.lock().insert(id, Pipe::new());
    id
}

/// Write data to a pipe (non-blocking)
pub fn write_pipe(id: u32, data: &[u8]) -> Option<usize> {
    if let Some(pipe) = PIPES.lock().get_mut(&id) {
        Some(pipe.write(data))
    } else {
        None
    }
}

/// Read data from a pipe. If pipe is empty, adds tid as reader and returns None (must block).
pub fn read_pipe(id: u32, buf: &mut [u8], tid: crate::process::Tid) -> Option<usize> {
    if let Some(pipe) = PIPES.lock().get_mut(&id) {
        if pipe.has_data() {
            Some(pipe.read(buf))
        } else {
            pipe.readers.push(tid);
            None // caller should block
        }
    } else {
        Some(0) // invalid pipe
    }
}
