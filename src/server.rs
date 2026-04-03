use axum::{
    extract::{Query, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post, delete},
    Form,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use tracing::info;
use std::net::SocketAddr;

use crate::state::AppState;
use crate::ws_handler::handle_ws_connection;

// Embedded frontend
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

pub async fn start_server(password: Option<String>, port: u16, allow_lan: bool) {
    let host = if allow_lan { "0.0.0.0" } else { "127.0.0.1" };

    // Initialize state
    let pty_manager = std::sync::Arc::new(crate::pty_manager::PtyManager::new());
    let auth_manager = std::sync::Arc::new(crate::auth::AuthManager::new());

    if let Some(pwd) = password {
        auth_manager.set_password(&pwd).expect("Failed to set password");
        info!("Password authentication enabled");
    } else {
        info!("No password set - authentication disabled");
    }

    let state = AppState {
        pty_manager,
        auth_manager,
    };

    // Build router with auth middleware on protected routes
    let app = axum::Router::new()
        // Public routes
        .route("/login", get(serve_login))
        .route("/auth", post(handle_auth))
        .route("/api/qr", get(generate_qr))
        // Protected routes (auth middleware applied)
        .route("/", get(serve_index))
        .route("/ws", get(ws_handler))
        .route("/api/terminals", get(list_terminals))
        .route("/api/terminals/new", post(create_terminal))
        .route("/api/terminals/{id}", delete(delete_terminal))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    // Add static file serving as a fallback route (must be last)
    let app = app.fallback(serve_static_fallback);

    let addr = format!("{}:{}", host, port);
    info!("Starting Tether server on http://{}", addr);

    if allow_lan {
        print_lan_info(port);
    }

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!("Server running at http://{}", addr);

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("Server failed");
}

/// Authentication middleware - checks session cookie on protected routes
async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Skip auth for public routes
    if path == "/auth" || path == "/login" || path == "/api/qr"
        || path.starts_with("/css/") || path.starts_with("/js/")
        || path.starts_with("/assets/")
    {
        return next.run(request).await;
    }

    // Check if password is set
    if !state.auth_manager.is_password_set() {
        return next.run(request).await;
    }

    // Validate session
    let jar = CookieJar::from_headers(request.headers());
    let authenticated = jar
        .get("session_token")
        .map(|c| state.auth_manager.validate_session(c.value()))
        .unwrap_or(false);

    if !authenticated {
        // Redirect to login for HTML requests, return 401 for API
        let accepts_html = request.headers()
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("text/html"))
            .unwrap_or(false);

        if accepts_html || path == "/" {
            return (
                StatusCode::FOUND,
                [(header::LOCATION, "/login")],
            ).into_response();
        } else {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    next.run(request).await
}

async fn serve_index() -> Response {
    match Asset::get("index.html") {
        Some(content) => Html(String::from_utf8_lossy(&content.data).into_owned()).into_response(),
        None => Html("<h1>Frontend not found</h1>").into_response(),
    }
}

async fn serve_login() -> Response {
    Html(include_str!("../assets/login.html")).into_response()
}

/// Serve static files from embedded assets (fallback handler)
async fn serve_static_fallback(
    uri: axum::http::Uri,
) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    match Asset::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                content.data.to_vec(),
            ).into_response()
        }
        None => {
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

#[derive(Deserialize)]
struct AuthForm {
    password: String,
}

async fn handle_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<AuthForm>,
) -> Response {
    // Use a default IP for rate limiting when behind a proxy
    // In production, the reverse proxy should set X-Forwarded-For
    let ip = jar.get("X-Forwarded-For")
        .map(|c| c.value().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string());

    match state.auth_manager.authenticate(&form.password, &ip) {
        Ok(token) => {
            let cookie = format!(
                "session_token={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
                token,
                60 * 60 * 24 * 7 // 7 days
            );

            (
                [(header::SET_COOKIE, cookie)],
                axum::response::Redirect::to("/"),
            )
                .into_response()
        }
        Err(e) => {
            // Return login page with error
            let error_html = format!(
                r#"<!DOCTYPE html>
                <html><head><title>Login - Tether</title>
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <style>
                    body {{ background: #1a1a2e; color: #eee; font-family: system-ui; display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0; }}
                    .login-box {{ background: #16213e; padding: 2rem; border-radius: 12px; text-align: center; max-width: 360px; width: 90%; }}
                    h2 {{ color: #00d4ff; margin-bottom: 1rem; }}
                    .error {{ color: #e94560; margin-bottom: 1rem; }}
                    a {{ color: #00d4ff; text-decoration: none; }}
                </style></head>
                <body><div class="login-box">
                    <h2>🪢 Tether</h2>
                    <p class="error">{}</p>
                    <a href="/login">← Back to login</a>
                </div></body></html>"#,
                e
            );
            Html(error_html).into_response()
        }
    }
}

async fn ws_handler(
    ws: axum::extract::WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let terminal_id = params.get("terminal_id").cloned();

    ws.on_upgrade(move |socket| handle_ws_connection(socket, state, terminal_id))
}

async fn list_terminals(State(state): State<AppState>) -> impl IntoResponse {
    let terminals = state.pty_manager.list_terminals();
    let json = serde_json::json!({
        "terminals": terminals.iter().map(|(id, waiting)| {
            serde_json::json!({
                "id": id,
                "waiting_for_input": waiting
            })
        }).collect::<Vec<_>>()
    });

    axum::Json(json).into_response()
}

async fn create_terminal(State(state): State<AppState>) -> impl IntoResponse {
    match state.pty_manager.spawn_terminal(None) {
        Ok(id) => axum::Json(serde_json::json!({ "id": id })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn delete_terminal(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if state.pty_manager.remove_terminal(&id) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn generate_qr(Query(params): Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    let default_url = "http://localhost:8080".to_string();
    let url = params.get("url").unwrap_or(&default_url);

    let qr = qrcode::QrCode::new(url.as_bytes()).unwrap();
    let svg = qr.render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();

    (
        [(header::CONTENT_TYPE, "image/svg+xml")],
        svg,
    )
}

fn print_lan_info(port: u16) {
    if let Ok(ifaces) = get_if_addrs::get_if_addrs() {
        for iface in ifaces.iter() {
            if !iface.is_loopback() && iface.ip().is_ipv4() {
                let url = format!("http://{}:{}", iface.ip(), port);
                info!("Access Tether at: {}", url);

                // Generate and print QR code as ASCII art
                let qr = qrcode::QrCode::new(url.clone()).unwrap();
                let modules = qr.to_colors();
                let size = qr.width();
                info!("QR Code for: {}", url);
                // Print QR code using block characters (2 rows per line)
                for y in (0..size).step_by(2) {
                    let mut line = String::new();
                    for x in 0..size {
                        let top = matches!(modules.get(y * size + x), Some(qrcode::Color::Dark));
                        let bottom = matches!(modules.get((y + 1) * size + x), Some(qrcode::Color::Dark));
                        match (top, bottom) {
                            (false, false) => line.push(' '),
                            (true, false) => line.push('▀'),
                            (false, true) => line.push('▄'),
                            (true, true) => line.push('█'),
                        }
                    }
                    info!("  {}", line);
                }
            }
        }
    }
}
