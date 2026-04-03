use dashmap::DashMap;
use serde::Serialize;
use std::io::{BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{info, warn};

#[derive(Clone, Debug, Serialize)]
pub struct TmuxPane {
    pub pane_id: String,
    pub session_name: String,
    pub window_index: String,
    pub pane_index: String,
    pub pane_title: String,
    pub pane_current_command: String,
}

pub struct TmuxManager {
    /// Active pipe processes for real-time streaming: pane_id -> (child, reader_thread_handle)
    pipes: DashMap<String, Arc<Mutex<Option<std::process::Child>>>>,
    /// WebSocket senders for each pane: pane_id -> Vec<tx>
    subscribers: DashMap<String, Vec<tokio::sync::mpsc::Sender<String>>>,
}

impl TmuxManager {
    pub fn new() -> Self {
        Self {
            pipes: DashMap::new(),
            subscribers: DashMap::new(),
        }
    }

    /// Check if tmux is available and running
    pub fn is_available() -> bool {
        Command::new("tmux")
            .arg("list-sessions")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// List all tmux sessions with their panes
    pub fn list_sessions(&self) -> Result<Vec<TmuxPane>, String> {
        // Get all panes with format
        let output = Command::new("tmux")
            .args([
                "list-panes",
                "-a",
                "-F",
                "#{session_name}:#{window_index}.#{pane_index}|#{session_name}|#{window_index}|#{pane_index}|#{pane_title}|#{pane_current_command}",
            ])
            .output()
            .map_err(|e| format!("Failed to run tmux: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("tmux error: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut panes = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(6, '|').collect();
            if parts.len() >= 6 {
                let pane_id = parts[0].to_string();
                panes.push(TmuxPane {
                    pane_id: pane_id.clone(),
                    session_name: parts[1].to_string(),
                    window_index: parts[2].to_string(),
                    pane_index: parts[3].to_string(),
                    pane_title: parts[4].to_string(),
                    pane_current_command: parts[5].to_string(),
                });
            }
        }

        Ok(panes)
    }

    /// Capture current content of a pane
    pub fn capture_pane(&self, pane_id: &str) -> Result<String, String> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-p", "-e", "-t", pane_id])
            .output()
            .map_err(|e| format!("Failed to capture pane: {}", e))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Send keys to a pane (simulates typing)
    pub fn send_keys(&self, pane_id: &str, text: &str) -> Result<(), String> {
        // For special keys, we need to handle them differently
        if text.starts_with('{') && text.ends_with('}') {
            // JSON-encoded special key
            let key = &text[1..text.len() - 1];
            Command::new("tmux")
                .args(["send-keys", "-t", pane_id, key])
                .output()
                .map_err(|e| format!("Failed to send key: {}", e))?;
        } else {
            // Send as literal text
            Command::new("tmux")
                .args(["send-keys", "-l", "-t", pane_id, text])
                .output()
                .map_err(|e| format!("Failed to send keys: {}", e))?;
        }

        Ok(())
    }

    /// Start piping a pane for real-time output streaming
    /// This uses tmux pipe-pane to stream output to a FIFO
    pub fn start_pane_pipe(&self, pane_id: &str) -> Result<String, String> {
        let fifo_path = format!("/tmp/tether-pipe-{}", pane_id.replace('.', "_"));

        // Create FIFO if it doesn't exist
        let _ = Command::new("mkfifo").arg(&fifo_path).output();

        // Start tmux pipe-pane to write output to FIFO
        let pipe_cmd = format!("cat >> {}", fifo_path);
        let _ = Command::new("tmux")
            .args(["pipe-pane", "-t", pane_id, &pipe_cmd])
            .output();

        // Spawn a background thread to read from the FIFO
        let pane_id_clone = pane_id.to_string();
        let fifo_path_clone = fifo_path.clone();
        let subscribers = self.subscribers.clone();

        std::thread::spawn(move || {
            // Open FIFO for reading (this blocks until something writes to it)
            let file = match std::fs::File::open(&fifo_path_clone) {
                Ok(f) => f,
                Err(e) => {
                    warn!("Failed to open FIFO for pane {}: {}", pane_id_clone, e);
                    return;
                }
            };

            let mut reader = BufReader::new(file);
            let mut buffer = [0u8; 4096];

            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buffer[..n]).to_string();

                        // Send to all subscribers
                        if let Some(subs) = subscribers.get(&pane_id_clone) {
                            let mut to_remove = Vec::new();
                            for (i, tx) in subs.value().iter().enumerate() {
                                if tx.blocking_send(text.clone()).is_err() {
                                    to_remove.push(i);
                                }
                            }
                            // Remove dead subscribers
                            for i in to_remove.into_iter().rev() {
                                if let Some(mut subs) = subscribers.get_mut(&pane_id_clone) {
                                    subs.value_mut().remove(i);
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(fifo_path)
    }

    /// Subscribe to a pane's output stream
    pub fn subscribe_pane(
        &self,
        pane_id: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, String> {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(256);

        // Start piping if not already started
        if !self.pipes.contains_key(pane_id) {
            self.start_pane_pipe(pane_id)?;
            self.pipes.insert(pane_id.to_string(), Arc::new(Mutex::new(None)));
        }

        self.subscribers
            .entry(pane_id.to_string())
            .or_insert_with(Vec::new)
            .push(tx);

        info!("Subscriber added to pane {}", pane_id);
        Ok(rx)
    }

    /// Unsubscribe from a pane
    #[allow(dead_code)]
    pub fn unsubscribe_pane(&self, pane_id: &str, tx: &tokio::sync::mpsc::Sender<String>) {
        if let Some(mut subs) = self.subscribers.get_mut(pane_id) {
            subs.value_mut()
                .retain(|s| !std::ptr::eq(s, tx));

            // If no more subscribers, stop piping
            if subs.value().is_empty() {
                self.stop_pane_pipe(pane_id);
            }
        }
    }

    /// Stop piping a pane
    #[allow(dead_code)]
    fn stop_pane_pipe(&self, pane_id: &str) {
        // Kill the pipe-pane
        let _ = Command::new("tmux")
            .args(["pipe-pane", "-t", pane_id])
            .output();

        // Clean up FIFO
        let fifo_path = format!("/tmp/tether-pipe-{}", pane_id.replace('.', "_"));
        let _ = std::fs::remove_file(&fifo_path);

        self.pipes.remove(pane_id);
        info!("Stopped piping pane {}", pane_id);
    }

    /// Resize a pane
    #[allow(dead_code)]
    pub fn resize_pane(&self, _pane_id: &str, _rows: u16, _cols: u16) -> Result<(), String> {
        // tmux panes are resized by the tmux client, not by us
        // We could implement this but it would affect the user's tmux layout
        Ok(())
    }
}
