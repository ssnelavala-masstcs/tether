use dashmap::DashMap;
use serde::Serialize;
use std::io::{BufReader, Read};
use std::os::unix::io::FromRawFd;
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
    pub fifo_path: String,
    pub is_setup: bool,
    pub setup_command: String,
}

struct MirrorState {
    subscribers: Vec<tokio::sync::mpsc::Sender<String>>,
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

    /// Discover all bash shells under gnome-terminal-server and store them
    pub fn discover_terminals(&self) -> Result<Vec<TerminalInfo>, String> {
        // Clear existing entries first
        self.terminals.clear();

        let gts_output = Command::new("sh")
            .arg("-c")
            .arg("pgrep -f 'gnome-terminal-server' | head -1")
            .output()
            .map_err(|e| format!("Failed to find gnome-terminal-server: {}", e))?;

        let gts_pid_str = String::from_utf8_lossy(&gts_output.stdout).trim().to_string();
        if gts_pid_str.is_empty() {
            return Err("gnome-terminal-server not found".to_string());
        }

        let gts_pid: u32 = gts_pid_str
            .parse()
            .map_err(|e| format!("Invalid PID: {}", e))?;

        let children_output = Command::new("sh")
            .arg("-c")
            .arg(&format!("ps --ppid {} -o pid= 2>/dev/null", gts_pid))
            .output()
            .map_err(|e| format!("Failed to list children: {}", e))?;

        let mut terminals = Vec::new();
        let mut idx = 0;

        for line in String::from_utf8_lossy(&children_output.stdout).lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            let pid: u32 = match line.parse() { Ok(p) => p, Err(_) => continue };

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

            let id = format!("term-{}", idx);
            let fifo_path = format!("/tmp/tether-mirror-{}", id);
            let setup_command = format!("exec > >(tee {} >&1) 2>&1", fifo_path);

            let info = TerminalInfo {
                id: id.clone(), pts_path: pts_path.clone(), pid, command: command.clone(),
                fifo_path: fifo_path.clone(), is_setup: self.mirrors.contains_key(&id),
                setup_command: setup_command.clone(),
            };

            // Store in the DashMap
            self.terminals.insert(id.clone(), info.clone());

            terminals.push(info);
            idx += 1;
        }

        Ok(terminals)
    }

    pub fn start_mirror(&self, id: &str) -> Result<String, String> {
        info!("start_mirror called for {}", id);
        let terminal = self.terminals.get(id)
            .ok_or_else(|| format!("Terminal {} not found", id))?;
        let fifo_path = terminal.value().fifo_path.clone();
        let pts_path = terminal.value().pts_path.clone();
        let id_str = id.to_string();
        info!("Found terminal {}, fifo_path={}, pts_path={}", id, fifo_path, pts_path);

        let _ = std::fs::remove_file(&fifo_path);
        let _ = Command::new("mkfifo").arg(&fifo_path).output();
        info!("Created FIFO at {}", fifo_path);

        // Build the setup command that pipes terminal output to our FIFO
        let setup_command = format!("exec > >(tee {} >&1) 2>&1\n", fifo_path);

        // Automatically inject the setup command into the terminal
        // We CAN write to /dev/pts/N even though we can't read from it
        // Strategy: send Ctrl+C to get to a clean prompt, wait, then send the command
        info!("Injecting setup command into {}...", pts_path);

        // Step 1: Send Ctrl+C to interrupt any running command
        let _ = std::fs::write(&pts_path, "\x03");
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Step 2: Send Ctrl+L to clear screen and ensure we're at a prompt
        let _ = std::fs::write(&pts_path, "\x0c");
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Step 3: Send the setup command with a newline
        std::fs::write(&pts_path, &setup_command)
            .map_err(|e| format!("Failed to inject setup command into {}: {}", pts_path, e))?;
        info!("Injected setup command into {}", pts_path);

        // Step 4: Wait for the tee process to start
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Step 5: Verify tee is running by checking for children
        let tee_check = Command::new("sh")
            .arg("-c")
            .arg(&format!("ps --ppid {} -o pid,cmd= 2>/dev/null | grep tee | grep -v grep", terminal.value().pid))
            .output();
        if let Ok(output) = tee_check {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if output_str.contains("tee") {
                info!("Verified: tee process running for {}", id_str);
            } else {
                warn!("WARNING: tee process not found for {} - command may not have executed", id_str);
            }
        }

        let mirror_state = Arc::new(Mutex::new(MirrorState { subscribers: Vec::new() }));
        let mirror_clone = mirror_state.clone();
        let fifo_clone = fifo_path.clone();
        let id_clone = id_str.clone();

        std::thread::spawn(move || {
            info!("Reader thread starting for {}", id_clone);
            // Open FIFO with O_RDWR - this never blocks and keeps the FIFO open
            // even when no external writer is connected. The write end we hold
            // prevents EOF when the tee process disconnects.
            let fd = unsafe {
                libc::open(fifo_clone.as_ptr() as *const libc::c_char, libc::O_RDWR)
            };
            if fd < 0 {
                warn!("Failed to open FIFO for {}: {}", id_clone, std::io::Error::last_os_error());
                return;
            }
            info!("Opened FIFO fd={} (O_RDWR) for {}", fd, id_clone);

            let mut reader = BufReader::new(unsafe { std::fs::File::from_raw_fd(fd) });
            let mut buffer = [0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        // Should never happen with O_RDWR since we hold the write end
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        continue;
                    }
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buffer[..n]).to_string();
                        let state = mirror_clone.lock().unwrap();
                        let mut to_remove = Vec::new();
                        for (i, tx) in state.subscribers.iter().enumerate() {
                            if tx.blocking_send(text.clone()).is_err() { to_remove.push(i); }
                        }
                        drop(state);
                        let mut state = mirror_clone.lock().unwrap();
                        for i in to_remove.into_iter().rev() { state.subscribers.remove(i); }
                    }
                    Err(e) => {
                        warn!("Read error for {}: {}", id_clone, e);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        continue;
                    }
                }
            }
        });

        info!("Inserting mirror state for {}", id_str);
        self.mirrors.insert(id_str.clone(), mirror_state);

        info!("Started mirror for terminal {}", id);
        Ok(fifo_path)
    }

    pub fn subscribe(&self, id: &str) -> Result<tokio::sync::mpsc::Receiver<String>, String> {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(256);
        if let Some(mirror) = self.mirrors.get(id) {
            let mut state = mirror.value().lock().unwrap();
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
        let pts_path = terminal.value().pts_path.clone();
        std::fs::write(&pts_path, text)
            .map_err(|e| format!("Failed to write to {}: {}", pts_path, e))?;
        Ok(())
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
