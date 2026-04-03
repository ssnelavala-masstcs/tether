use crate::auth::AuthManager;
use crate::pty_manager::PtyManager;
use crate::tmux_manager::TmuxManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pty_manager: Arc<PtyManager>,
    pub auth_manager: Arc<AuthManager>,
    pub tmux_manager: Arc<TmuxManager>,
}
