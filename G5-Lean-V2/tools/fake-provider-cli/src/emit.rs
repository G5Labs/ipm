//! Line-oriented JSON output with an explicit flush after every frame, so
//! scenario delays are observable by the reader and a consumer that closed the
//! pipe ends the fake quietly.

use std::io::{self, Write};

use serde_json::Value;

/// Error raised when the frame consumer went away.
#[derive(Debug)]
pub struct ConsumerGone;

/// Writes one JSON object per line to stdout and flushes it immediately.
pub struct FrameWriter {
    stdout: io::Stdout,
    frames_written: usize,
    app_server: Option<crate::codex_app_server::Binding>,
}

impl Default for FrameWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameWriter {
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            frames_written: 0,
            app_server: None,
        }
    }

    pub fn app_server(thread: String, turn: String) -> Self {
        Self {
            app_server: Some(crate::codex_app_server::Binding { thread, turn }),
            ..Self::new()
        }
    }

    /// Number of frames written so far.
    pub fn frames_written(&self) -> usize {
        self.frames_written
    }

    /// Serialise `frame` compactly and write it as one line.
    pub fn emit(&mut self, frame: &Value) -> Result<(), ConsumerGone> {
        if let Some(binding) = self.app_server.clone() {
            let translated = binding.translate(frame);
            self.app_server = None;
            let result = translated.iter().try_for_each(|frame| self.emit(frame));
            self.app_server = Some(binding);
            return result;
        }
        let mut line = serde_json::to_string(frame).expect("serde_json::Value always serialises");
        line.push('\n');
        let mut handle = self.stdout.lock();
        if handle.write_all(line.as_bytes()).is_err() || handle.flush().is_err() {
            return Err(ConsumerGone);
        }
        self.frames_written += 1;
        Ok(())
    }
}

/// Write a diagnostic line to stderr, ignoring a closed stderr.
pub fn stderr_line(message: &str) {
    let mut handle = io::stderr().lock();
    let _ = writeln!(handle, "{message}");
    let _ = handle.flush();
}
