use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;
use portable_pty::{PtySize, CommandBuilder};
use std::sync::Mutex;

pub struct TerminalSession {
    pub id: String,
    pub pair: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    pub child: Arc<Mutex<Box<dyn portable_pty::Child + Send>>>,
    pub last_activity: std::time::Instant,
    pub waiting_for_input: bool,
}

unsafe impl Send for TerminalSession {}
unsafe impl Sync for TerminalSession {}

pub struct PtyManager {
    pub terminals: DashMap<String, Arc<Mutex<TerminalSession>>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            terminals: DashMap::new(),
        }
    }

    pub fn spawn_terminal(&self, shell: Option<&str>) -> Result<String, String> {
        let pty_system = portable_pty::native_pty_system();
        
        let size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(size)
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        let shell_cmd = shell.unwrap_or("bash");
        let cmd = CommandBuilder::new(shell_cmd);
        
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn shell: {}", e))?;

        let id = Uuid::new_v4().to_string();
        
        let session = TerminalSession {
            id: id.clone(),
            pair: Arc::new(Mutex::new(pair.master)),
            child: Arc::new(Mutex::new(child)),
            last_activity: std::time::Instant::now(),
            waiting_for_input: false,
        };

        self.terminals.insert(id.clone(), Arc::new(Mutex::new(session)));
        Ok(id)
    }

    pub fn get_terminal(&self, id: &str) -> Option<Arc<Mutex<TerminalSession>>> {
        self.terminals.get(id).map(|t| t.clone())
    }

    pub fn remove_terminal(&self, id: &str) -> bool {
        if let Some((_, session)) = self.terminals.remove(id) {
            if let Ok(term) = session.lock() {
                if let Ok(mut child) = term.child.lock() {
                    let _ = child.kill();
                }
            }
            true
        } else {
            false
        }
    }

    pub fn resize_terminal(&self, id: &str, rows: u16, cols: u16) -> Result<(), String> {
        if let Some(session) = self.get_terminal(id) {
            if let Ok(term) = session.lock() {
                if let Ok(pair) = term.pair.lock() {
                    pair.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    }).map_err(|e| format!("Failed to resize: {}", e))?;
                }
            }
            Ok(())
        } else {
            Err("Terminal not found".to_string())
        }
    }

    pub fn list_terminals(&self) -> Vec<(String, bool)> {
        self.terminals
            .iter()
            .map(|t| {
                let guard = t.value().lock().unwrap();
                (t.key().clone(), guard.waiting_for_input)
            })
            .collect()
    }
}
