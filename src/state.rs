use crate::pty_manager::PtyManager;
use crate::auth::AuthManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pty_manager: Arc<PtyManager>,
    pub auth_manager: Arc<AuthManager>,
}
