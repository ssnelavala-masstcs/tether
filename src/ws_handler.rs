use axum::extract::ws::{Message, WebSocket};
use std::io::Write;
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
    let (pty_tx, mut pty_rx) = tokio::sync::mpsc::channel::<String>(100);

    let term_for_reader = terminal.clone();
    let pty_tx_clone = pty_tx.clone();

    tokio::task::spawn_blocking(move || {
        let mut buffer = [0u8; 4096];
        loop {
            let reader = {
                match term_for_reader.lock() {
                    Ok(term) => match term.pair.lock() {
                        Ok(pair) => match pair.try_clone_reader() {
                            Ok(r) => Some(r),
                            Err(e) => {
                                warn!("Failed to clone reader: {}", e);
                                None
                            }
                        },
                        Err(_) => None,
                    },
                    Err(_) => None,
                }
            };

            if let Some(mut reader) = reader {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        warn!("PTY closed for terminal");
                        break;
                    }
                    Ok(n) => {
                        let output = String::from_utf8_lossy(&buffer[..n]).to_string();
                        if pty_tx_clone.blocking_send(output).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Read error: {}", e);
                        break;
                    }
                }
            } else {
                break;
            }
        }
    });

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
                        if let Ok(term) = terminal.lock() {
                            if let Ok(pair) = term.pair.lock() {
                                if let Ok(mut writer) = pair.take_writer() {
                                    if writer.write_all(text.as_bytes()).is_err() {
                                        break;
                                    }
                                    let _ = writer.flush();
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        if let Ok(term) = terminal.lock() {
                            if let Ok(pair) = term.pair.lock() {
                                if let Ok(mut writer) = pair.take_writer() {
                                    if writer.write_all(&data).is_err() {
                                        break;
                                    }
                                    let _ = writer.flush();
                                }
                            }
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

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        pty_manager.remove_terminal(&terminal_id);
        info!("Cleaned up terminal: {}", terminal_id);
    });
}
