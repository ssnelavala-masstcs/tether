use dashmap::DashMap;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::pty::Winsize;
use nix::unistd::{self, ForkResult, Pid};
use std::os::unix::io::AsRawFd;
use std::os::unix::io::OwnedFd;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

pub struct TerminalSession {
    #[allow(dead_code)]
    pub id: String,
    pub master_fd: OwnedFd,
    pub child_pid: Pid,
    #[allow(dead_code)]
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
        let shell_cmd = shell.unwrap_or("/bin/bash");

        let winsize = Winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        match unsafe { nix::pty::forkpty(&winsize, None) } {
            Ok(result) => match result.fork_result {
                ForkResult::Parent { child } => {
                    let master_fd = result.master;

                    // Set master fd to non-blocking
                    let raw_fd = master_fd.as_raw_fd();
                    let flags = fcntl(raw_fd, FcntlArg::F_GETFL)
                        .map_err(|e| format!("Failed to get flags: {}", e))?;
                    let new_flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
                    fcntl(raw_fd, FcntlArg::F_SETFL(new_flags))
                        .map_err(|e| format!("Failed to set non-blocking: {}", e))?;

                    let id = Uuid::new_v4().to_string();

                    let session = TerminalSession {
                        id: id.clone(),
                        master_fd,
                        child_pid: child,
                        last_activity: std::time::Instant::now(),
                        waiting_for_input: false,
                    };

                    self.terminals
                        .insert(id.clone(), Arc::new(Mutex::new(session)));
                    Ok(id)
                }
                ForkResult::Child => {
                    // Child process - exec the shell
                    let shell_cstr = std::ffi::CString::new(shell_cmd).unwrap();
                    unistd::execvp(&shell_cstr, &[&shell_cstr]).unwrap_or_else(|e| {
                        eprintln!("Failed to exec {}: {}", shell_cmd, e);
                        std::process::exit(1);
                    });
                    unreachable!();
                }
            },
            Err(e) => Err(format!("Failed to forkpty: {}", e)),
        }
    }

    pub fn get_terminal(&self, id: &str) -> Option<Arc<Mutex<TerminalSession>>> {
        self.terminals.get(id).map(|t| t.clone())
    }

    pub fn remove_terminal(&self, id: &str) -> bool {
        if let Some((_, session)) = self.terminals.remove(id) {
            if let Ok(term) = session.lock() {
                // Send SIGHUP to child
                unsafe { libc::kill(term.child_pid.as_raw(), libc::SIGHUP) };
                // master_fd will be closed when OwnedFd is dropped
            }
            true
        } else {
            false
        }
    }

    pub fn resize_terminal(&self, id: &str, rows: u16, cols: u16) -> Result<(), String> {
        if let Some(session) = self.get_terminal(id) {
            if let Ok(term) = session.lock() {
                let winsize = Winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                unsafe {
                    libc::ioctl(
                        term.master_fd.as_raw_fd(),
                        libc::TIOCSWINSZ,
                        &winsize as *const Winsize,
                    );
                }
                // Also send SIGWINCH to the child
                unsafe { libc::kill(term.child_pid.as_raw(), libc::SIGWINCH) };
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
