use dashmap::DashMap;
use serde::Serialize;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{info, warn};

#[derive(Clone, Debug, Serialize)]
pub struct TerminalInfo {
    pub id: String,
    pub pts_path: String,
    pub pid: u32,
    pub command: String,
    pub tmux_session: String,
    pub tmux_pane: String,
    pub is_setup: bool,
}

struct MirrorState {
    subscribers: Vec<tokio::sync::mpsc::Sender<String>>,
    last_content: String,
}

pub struct TerminalMirror {
    terminals: DashMap<String, TerminalInfo>,
    mirrors: DashMap<String, Arc<Mutex<MirrorState>>>,
}

impl TerminalMirror {
    pub fn new() -> Self {
        Self {
            terminals: DashMap::new(),
            mirrors: DashMap::new(),
        }
    }

    /// Discover tmux sessions and map them to existing terminals
    pub fn discover_terminals(&self) -> Result<Vec<TerminalInfo>, String> {
        self.terminals.clear();

        // Check if tmux is available
        let tmux_check = Command::new("tmux")
            .arg("list-sessions")
            .output();
        
        if tmux_check.is_err() {
            return Err("tmux not available".to_string());
        }

        let output = tmux_check.unwrap();
        if !output.status.success() {
            return Err("no tmux sessions".to_string());
        }

        let sessions_str = String::from_utf8_lossy(&output.stdout);
        let mut terminals = Vec::new();
        let mut idx = 0;

        for line in sessions_str.lines() {
            // Format: "session_name: windows (created ...)"
            let session_name = match line.split(':').next() {
                Some(n) => n.trim(),
                None => continue,
            };

            // Get panes for this session
            let panes_output = Command::new("tmux")
                .arg("list-panes")
                .arg("-t")
                .arg(session_name)
                .arg("-F")
                .arg("#{pane_id} #{pane_pid} #{pane_tty}")
                .output();

            if let Ok(panes) = panes_output {
                let panes_str = String::from_utf8_lossy(&panes.stdout);
                for pane_line in panes_str.lines() {
                    let parts: Vec<&str> = pane_line.split_whitespace().collect();
                    if parts.len() < 3 { continue; }

                    let pane_id = parts[0];
                    let pid: u32 = parts[1].parse().unwrap_or(0);
                    let pts_path = parts[2];

                    let id = format!("mirror-{}", idx);
                    let info = TerminalInfo {
                        id: id.clone(),
                        pts_path: pts_path.to_string(),
                        pid,
                        command: "bash".to_string(),
                        tmux_session: session_name.to_string(),
                        tmux_pane: pane_id.to_string(),
                        is_setup: self.mirrors.contains_key(&id),
                    };

                    self.terminals.insert(id.clone(), info.clone());
                    terminals.push(info);
                    idx += 1;
                }
            }
        }

        // Also discover non-tmux bash sessions
        let bash_terminals = self._discover_all_bash();
        for info in bash_terminals {
            // Skip if we already have this pts_path
            let exists = self.terminals.iter().any(|e| e.value().pts_path == info.pts_path);
            if !exists {
                self.terminals.insert(info.id.clone(), info.clone());
                terminals.push(info);
            }
        }

        if terminals.is_empty() {
            return Err("No terminal sessions found".to_string());
        }

        Ok(terminals)
    }

    fn _discover_all_bash(&self) -> Vec<TerminalInfo> {
        let mut terminals = Vec::new();
        let mut idx = self.terminals.len();

        let output = Command::new("sh")
            .arg("-c")
            .arg("ps -eo pid,tty,comm= 2>/dev/null | grep '/pts/' | grep -E 'bash|zsh|sh|fish' | grep -v 'tmux'")
            .output();

        let output_str = match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
            Err(_) => return terminals,
        };

        let my_pid = std::process::id();

        for line in output_str.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 { continue; }

            let pid: u32 = match parts[0].parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            if pid == my_pid || pid > my_pid + 100 { continue; }

            let pts_path = match std::fs::read_link(format!("/proc/{}/fd/0", pid)) {
                Ok(path) => {
                    let path_str = path.to_string_lossy().to_string();
                    if path_str.starts_with("/dev/pts/") { path_str } else { continue }
                }
                Err(_) => continue,
            };

            let command = std::fs::read_to_string(format!("/proc/{}/comm", pid))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string());

            let id = format!("mirror-{}", idx);
            let info = TerminalInfo {
                id: id.clone(),
                pts_path,
                pid,
                command,
                tmux_session: String::new(),
                tmux_pane: String::new(),
                is_setup: false,
            };

            terminals.push(info);
            idx += 1;
        }

        terminals
    }

    pub fn start_mirror(&self, id: &str) -> Result<String, String> {
        info!("start_mirror called for {}", id);
        let terminal = self.terminals.get(id)
            .ok_or_else(|| format!("Terminal {} not found", id))?;
        
        let tmux_session = terminal.value().tmux_session.clone();
        let tmux_pane = terminal.value().tmux_pane.clone();
        let has_tmux = !tmux_session.is_empty();
        let id_str = id.to_string();

        if has_tmux {
            info!("Using tmux capture for {} (session={}, pane={})", id, tmux_session, tmux_pane);
        } else {
            info!("Using FIFO method for {} (pts={})", id, terminal.value().pts_path);
        }

        let mirror_state = Arc::new(Mutex::new(MirrorState {
            subscribers: Vec::new(),
            last_content: String::new(),
        }));
        let mirror_clone = mirror_state.clone();

        let id_clone = id_str.clone();
        let tmux_pane_clone = tmux_pane.clone();
        let tmux_session_clone = tmux_session.clone();

        std::thread::spawn(move || {
            info!("Reader thread starting for {}", id_clone);
            
            let mut poll_interval = std::time::Duration::from_millis(200);

            loop {
                let content = if has_tmux {
                    // Use tmux capture-pane for reliable output
                    let capture = Command::new("tmux")
                        .arg("capture-pane")
                        .arg("-t")
                        .arg(&tmux_pane_clone)
                        .arg("-p")
                        .arg("-e")
                        .output();

                    match capture {
                        Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
                        Err(e) => {
                            warn!("tmux capture error for {}: {}", id_clone, e);
                            std::thread::sleep(poll_interval);
                            continue;
                        }
                    }
                } else {
                    // For non-tmux terminals, try to read from /proc/[pid]/fd/1
                    // This only works for terminals that have readable output
                    std::thread::sleep(poll_interval);
                    continue;
                };

                let mut state = mirror_clone.lock().unwrap();
                
                // Only send if content changed
                if content != state.last_content && !content.is_empty() {
                    state.last_content = content.clone();
                    
                    let to_remove: Vec<usize> = state.subscribers.iter()
                        .enumerate()
                        .filter(|(_, tx)| tx.blocking_send(content.clone()).is_err())
                        .map(|(i, _)| i)
                        .collect();
                    
                    for i in to_remove.into_iter().rev() {
                        state.subscribers.remove(i);
                    }
                }
                
                drop(state);
                std::thread::sleep(poll_interval);
            }
        });

        self.mirrors.insert(id_str.clone(), mirror_state);
        info!("Started mirror for terminal {}", id);
        Ok(id_str)
    }

    pub fn subscribe(&self, id: &str) -> Result<tokio::sync::mpsc::Receiver<String>, String> {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(256);
        if let Some(mirror) = self.mirrors.get(id) {
            let mut state = mirror.value().lock().unwrap();
            // Send initial content if available
            if !state.last_content.is_empty() {
                let _ = tx.blocking_send(state.last_content.clone());
            }
            state.subscribers.push(tx);
        } else {
            return Err(format!("Mirror for {} not started", id));
        }
        info!("Subscriber added to terminal {}", id);
        Ok(rx)
    }

    pub fn send_input(&self, id: &str, text: &str) -> Result<(), String> {
        let terminal = self.terminals.get(id)
            .ok_or_else(|| format!("Terminal {} not found", id))?;
        
        let tmux_session = terminal.value().tmux_session.clone();
        let tmux_pane = terminal.value().tmux_pane.clone();
        let has_tmux = !tmux_session.is_empty();

        if has_tmux {
            // Use tmux send-keys for reliable input
            let result = Command::new("tmux")
                .arg("send-keys")
                .arg("-t")
                .arg(&tmux_pane)
                .arg(text)
                .output();
            
            match result {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("tmux send-keys error: {}", e)),
            }
        } else {
            // Fallback: write to /dev/pts/N
            std::fs::write(&terminal.value().pts_path, text)
                .map_err(|e| format!("Failed to write to {}: {}", terminal.value().pts_path, e))
        }
    }

    pub fn get_all_terminals(&self) -> Vec<TerminalInfo> {
        self.terminals.iter().map(|entry| {
            let mut info = entry.value().clone();
            info.is_setup = self.mirrors.contains_key(&info.id);
            info
        }).collect()
    }

    pub fn register_terminal(&self, info: TerminalInfo) {
        self.terminals.insert(info.id.clone(), info);
    }

    pub fn clear(&self) {
        self.terminals.clear();
        self.mirrors.clear();
    }
}
