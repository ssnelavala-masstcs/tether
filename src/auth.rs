use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::Rng;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SESSION_DURATION: Duration = Duration::from_secs(60 * 60 * 24 * 7); // 7 days
const MAX_FAILED_ATTEMPTS: u32 = 5;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(600); // 10 minutes

#[derive(Clone)]
pub struct Session {
    #[allow(dead_code)]
    pub token: String,
    pub created_at: Instant,
}

struct RateLimit {
    attempts: u32,
    window_start: Instant,
}

pub struct AuthManager {
    password_hash: Mutex<Option<String>>,
    sessions: Mutex<HashMap<String, Session>>,
    rate_limits: Mutex<HashMap<String, RateLimit>>,
}

impl AuthManager {
    pub fn new() -> Self {
        Self {
            password_hash: Mutex::new(None),
            sessions: Mutex::new(HashMap::new()),
            rate_limits: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_password(&self, password: &str) -> Result<(), String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| format!("Failed to hash password: {}", e))?;

        *self.password_hash.lock().unwrap() = Some(hash.to_string());
        Ok(())
    }

    pub fn is_password_set(&self) -> bool {
        self.password_hash.lock().unwrap().is_some()
    }

    pub fn authenticate(&self, password: &str, ip: &str) -> Result<String, String> {
        // Check rate limiting
        {
            let mut rate_limits = self.rate_limits.lock().unwrap();
            let now = Instant::now();

            let limit = rate_limits.entry(ip.to_string()).or_insert(RateLimit {
                attempts: 0,
                window_start: now,
            });

            if now.duration_since(limit.window_start) > RATE_LIMIT_WINDOW {
                limit.attempts = 0;
                limit.window_start = now;
            }

            if limit.attempts >= MAX_FAILED_ATTEMPTS {
                return Err("Too many failed attempts. Please try again later.".to_string());
            }

            limit.attempts += 1;
        }

        // Verify password
        let hash = self.password_hash.lock().unwrap();
        let hash_str = hash.as_ref().ok_or("No password set")?;

        let parsed_hash =
            PasswordHash::new(hash_str).map_err(|e| format!("Invalid password hash: {}", e))?;

        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .map_err(|_| "Invalid password".to_string())?;

        // Reset rate limit on success
        {
            let mut rate_limits = self.rate_limits.lock().unwrap();
            rate_limits.remove(ip);
        }

        // Create session
        let token = self.generate_token();
        let session = Session {
            token: token.clone(),
            created_at: Instant::now(),
        };

        self.sessions.lock().unwrap().insert(token.clone(), session);
        Ok(token)
    }

    pub fn validate_session(&self, token: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap();

        if let Some(session) = sessions.get(token) {
            if session.created_at.elapsed() < SESSION_DURATION {
                return true;
            }
            sessions.remove(token);
        }
        false
    }

    #[allow(dead_code)]
    pub fn cleanup_expired(&self) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, s| s.created_at.elapsed() < SESSION_DURATION);
    }

    /// Clean up expired rate limit entries for a given IP
    #[allow(dead_code)]
    pub fn cleanup_rate_limits(&self, ip: &str) {
        let mut rate_limits = self.rate_limits.lock().unwrap();
        if let Some(limit) = rate_limits.get(ip) {
            if limit.window_start.elapsed() > RATE_LIMIT_WINDOW {
                rate_limits.remove(ip);
            }
        }
    }

    fn generate_token(&self) -> String {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
