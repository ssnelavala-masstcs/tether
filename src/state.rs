use crate::auth::AuthManager;
use crate::pty_manager::PtyManager;
use crate::terminal_mirror::TerminalMirror;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pty_manager: Arc<PtyManager>,
    pub auth_manager: Arc<AuthManager>,
    pub terminal_mirror: Arc<TerminalMirror>,
}
