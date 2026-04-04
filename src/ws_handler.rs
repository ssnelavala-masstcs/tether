use axum::extract::ws::{Message, WebSocket};
use std::os::unix::io::AsRawFd;
use tracing::{error, info, warn};

use crate::state::AppState;

pub async fn handle_ws_connection(
    socket: WebSocket,
    state: AppState,
    terminal_id: Option<String>,
    mirror_id: Option<String>,
) {
    if let Some(mid) = mirror_id {
        info!("WebSocket connected to mirror: {}", mid);
        handle_mirror_io(socket, state, mid).await;
    } else {
        let terminal_id = match terminal_id {
            Some(id) => id,
            None => match state.pty_manager.spawn_terminal(None) {
                Ok(id) => id,
                Err(e) => {
                    error!("Failed to spawn terminal: {}", e);
                    return;
                }
            },
        };

        info!("WebSocket connected to terminal: {}", terminal_id);

        let terminal = match state.pty_manager.get_terminal(&terminal_id) {
            Some(t) => t,
            None => {
                error!("Terminal not found: {}", terminal_id);
                return;
            }
        };

        handle_terminal_io(socket, terminal, terminal_id, state.pty_manager.clone()).await;
    }
}

async fn handle_mirror_io(
    mut socket: WebSocket,
    state: AppState,
    mirror_id: String,
) {
    let mut rx = match state.terminal_mirror.subscribe(&mirror_id) {
        Ok(rx) => rx,
        Err(e) => {
            error!("Failed to subscribe to mirror {}: {}", mirror_id, e);
            return;
        }
    };

    loop {
        tokio::select! {
            Some(output) = rx.recv() => {
                if socket.send(Message::Text(output)).await.is_err() {
                    break;
                }
            }

            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if state.terminal_mirror.send_input(&mirror_id, &text).is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        if let Ok(text) = String::from_utf8(data) {
                            if state.terminal_mirror.send_input(&mirror_id, &text).is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("WebSocket closed for mirror: {}", mirror_id);
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        info!("WebSocket connection closed");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    info!("Disconnected from mirror: {}", mirror_id);
}

async fn handle_terminal_io(
    mut socket: WebSocket,
    terminal: std::sync::Arc<std::sync::Mutex<crate::pty_manager::TerminalSession>>,
    terminal_id: String,
    pty_manager: std::sync::Arc<crate::pty_manager::PtyManager>,
) {
    let (pty_tx, mut pty_rx) = tokio::sync::mpsc::channel::<String>(256);

    // Get the master fd
    let master_fd = {
        let term = terminal.lock().unwrap();
        term.master_fd.as_raw_fd()
    };

    // Spawn reader thread - read from master fd
    let tid_for_reader = terminal_id.clone();
    let pty_tx_for_reader = pty_tx.clone();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match unsafe {
                libc::read(master_fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len())
            } {
                0 => {
                    warn!("PTY closed for terminal {}", tid_for_reader);
                    break;
                }
                n if n > 0 => {
                    let output = String::from_utf8_lossy(&buffer[..n as usize]).to_string();
                    if pty_tx_for_reader.blocking_send(output).is_err() {
                        break;
                    }
                }
                _ => {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        // Non-blocking: no data available, sleep briefly
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                    warn!("Read error for terminal {}: {}", tid_for_reader, err);
                    break;
                }
            }
        }
    });

    // Main loop: forward PTY output to WebSocket, forward WebSocket input to PTY
    loop {
        tokio::select! {
            Some(output) = pty_rx.recv() => {
                if socket.send(Message::Text(output)).await.is_err() {
                    break;
                }
            }

            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let master_fd = {
                            let term = terminal.lock().unwrap();
                            term.master_fd.as_raw_fd()
                        };
                        let bytes = text.as_bytes();
                        let result = unsafe {
                            libc::write(master_fd, bytes.as_ptr() as *const libc::c_void, bytes.len())
                        };
                        if result < 0 {
                            warn!("Write error: {}", std::io::Error::last_os_error());
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        let master_fd = {
                            let term = terminal.lock().unwrap();
                            term.master_fd.as_raw_fd()
                        };
                        let result = unsafe {
                            libc::write(master_fd, data.as_ptr() as *const libc::c_void, data.len())
                        };
                        if result < 0 {
                            warn!("Write error: {}", std::io::Error::last_os_error());
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("WebSocket closed for terminal: {}", terminal_id);
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        info!("WebSocket connection closed");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Cleanup after disconnect
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        pty_manager.remove_terminal(&terminal_id);
        info!("Cleaned up terminal: {}", terminal_id);
    });
}
