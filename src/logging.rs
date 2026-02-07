/// Tracing layer that captures log entries for the UI log view.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;

/// A ring buffer that stores log lines for display in the UI.
#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<VecDeque<String>>>,
    max_lines: usize,
}

impl LogBuffer {
    pub fn new(max_lines: usize) -> Self {
        LogBuffer {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(max_lines))),
            max_lines,
        }
    }

    /// Push a log line into the buffer.
    pub fn push(&self, line: String) {
        let mut buf = self.inner.lock().expect("LogBuffer lock poisoned");
        if buf.len() >= self.max_lines {
            buf.pop_front();
        }
        buf.push_back(line);
    }

    /// Get all current log lines.
    pub fn get_lines(&self) -> Vec<String> {
        let buf = self.inner.lock().expect("LogBuffer lock poisoned");
        buf.iter().cloned().collect()
    }

    /// Get the text for display (all lines joined with newlines).
    pub fn get_text(&self) -> String {
        self.get_lines().join("\n")
    }

    /// Clear all log lines.
    pub fn clear(&self) {
        let mut buf = self.inner.lock().expect("LogBuffer lock poisoned");
        buf.clear();
    }
}

/// Writer that appends to the LogBuffer.
pub struct LogBufferWriter {
    buffer: LogBuffer,
    line: Vec<u8>,
}

impl std::io::Write for LogBufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &b in buf {
            if b == b'\n' {
                let line = String::from_utf8_lossy(&self.line).to_string();
                self.buffer.push(line);
                self.line.clear();
            } else {
                self.line.push(b);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.line.is_empty() {
            let line = String::from_utf8_lossy(&self.line).to_string();
            self.buffer.push(line);
            self.line.clear();
        }
        Ok(())
    }
}

impl Drop for LogBufferWriter {
    fn drop(&mut self) {
        if !self.line.is_empty() {
            let _ = std::io::Write::flush(self);
        }
    }
}

/// MakeWriter implementation for tracing_subscriber.
#[derive(Clone)]
pub struct LogBufferMakeWriter {
    buffer: LogBuffer,
}

impl LogBufferMakeWriter {
    pub fn new(buffer: LogBuffer) -> Self {
        LogBufferMakeWriter { buffer }
    }
}

impl<'a> MakeWriter<'a> for LogBufferMakeWriter {
    type Writer = LogBufferWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogBufferWriter {
            buffer: self.buffer.clone(),
            line: Vec::new(),
        }
    }
}
