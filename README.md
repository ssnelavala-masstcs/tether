# 🪢 Tether

> **Your terminal, anywhere.** A lightweight, mobile-optimized web terminal controller that mirrors and controls local shell sessions from your phone — zero cloud dependencies, fully self-hosted.

<div align="center">

[![Build Status](https://github.com/ssnelavala-masstcs/tether/actions/workflows/ci.yml/badge.svg)](https://github.com/ssnelavala-masstcs/tether/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-blue.svg)](https://www.rust-lang.org)
[![Release](https://img.shields.io/badge/version-1.0.0-green.svg)](https://github.com/ssnelavala-masstcs/tether/releases)

</div>

<p align="center">
  <img src="docs/assets/hero-screenshot.png" alt="Tether - Mobile Terminal Controller" width="600">
</p>

## ✨ Features

| Feature | Description |
|---------|-------------|
| 📱 **Mobile-First UI** | Touch-optimized, responsive xterm.js interface designed for phones |
| 🔐 **Password Auth** | Argon2id hashed passwords with session cookies & rate limiting |
| 🖥️ **Multi-PTY** | Spawn and manage multiple terminal sessions simultaneously |
| ⚡ **Real-Time** | WebSocket-based streaming with ≤150ms input→output latency |
| 🎛️ **Preset Commands** | Quick-tap buttons for common commands (ls, top, df, free) |
| 📋 **Console Drawer** | Swipeable panel to switch between active terminals |
| 🔔 **Input Detection** | Notifies you when a terminal is waiting for input |
| 📡 **LAN Access** | Auto-detects LAN IP, prints QR code for instant phone access |
| 🔒 **Secure by Default** | Binds to localhost unless `--allow-lan` is explicitly set |
| 📦 **Single Binary** | Frontend embedded via rust-embed — no npm, no external deps |

## 🚀 Quick Start

### One-Liner

```bash
cargo run --release -- serve --password "yourpassword" --allow-lan
```

Then open `http://<your-laptop-ip>:8080` on your phone.

### Installation

#### From Source (Recommended)

```bash
# Clone the repository
git clone https://github.com/ssnelavala-masstcs/tether.git
cd tether

# Build (requires Rust 1.75+)
cargo build --release

# Run
./target/release/tether serve --password "mypassword" --allow-lan
```

#### Using Cargo Install

```bash
cargo install --git https://github.com/ssnelavala-masstcs/tether.git
tether serve --password "mypassword" --allow-lan
```

#### Pre-built Binaries

Download from [Releases](https://github.com/ssnelavala-masstcs/tether/releases) for your platform.

### Usage

```bash
# Basic usage (localhost only)
tether serve --password "secret"

# Allow LAN access (binds to 0.0.0.0)
tether serve --password "secret" --allow-lan

# Custom port
tether serve --password "secret" --port 3000 --allow-lan

# Full options
tether serve --help
```

### CLI Reference

```
Usage: tether serve [OPTIONS]

Options:
  -p, --password <PASSWORD>  Password for authentication
  -P, --port <PORT>          Port to bind to [default: 8080]
      --allow-lan            Allow LAN access (binds to 0.0.0.0 instead of 127.0.0.1)
  -h, --help                 Print help
```

## 📱 Accessing from Your Phone

1. **Start the server** with `--allow-lan`:
   ```bash
   tether serve --password "secret" --allow-lan
   ```

2. **Note the printed IP address** — Tether auto-detects your LAN IP:
   ```
   INFO Access Tether at: http://192.168.1.100:8080
   ```

3. **Open that URL** on your phone's browser

4. **Enter the password** to access the terminal dashboard

5. **Start controlling your laptop** from your couch 🛋️

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      Your Laptop                         │
│                                                          │
│  ┌──────────────┐    ┌──────────────┐    ┌────────────┐ │
│  │   Axum HTTP  │◄──►│   PTY Mgr    │◄──►│  bash/sh   │ │
│  │   + WS Srv   │    │ (portable-pty)│    │  sessions  │ │
│  └──────┬───────┘    └──────────────┘    └────────────┘ │
│         │                                                 │
│         │ WebSocket (real-time I/O)                       │
│         ▼                                                 │
│  ┌──────────────┐                                        │
│  │   Auth Mgr   │  Argon2id + Session Cookies            │
│  │   (argon2)   │  Rate limiting: 5 attempts / 10 min    │
│  └──────────────┘                                        │
└─────────────────────────┬───────────────────────────────┘
                          │ LAN (WiFi)
                          ▼
┌─────────────────────────────────────────────────────────┐
│                      Your Phone                          │
│                                                          │
│  ┌────────────────────────────────────────────────────┐ │
│  │              Mobile Web Browser                     │ │
│  │                                                     │ │
│  │  ┌─────────────────────────────────────────────┐   │ │
│  │  │              xterm.js Terminal               │   │ │
│  │  │  (real-time output rendering + input)       │   │ │
│  │  └─────────────────────────────────────────────┘   │ │
│  │  ┌─────┬─────┬─────┬─────┬───────┐                 │ │
│  │  │  1  │  2  │  3  │  4  │ Enter │  Preset Btns   │ │
│  │  └─────┴─────┴─────┴─────┴───────┘                 │ │
│  │  [ Custom command input field ........ ] [Send]    │ │
│  │                                                     │ │
│  │  ☰ Drawer ──────────────────────────────           │ │
│  │  │  Consoles                          │            │ │
│  │  │  + New Terminal                    │            │ │
│  │  │  ● abc12345...  Active             │            │ │
│  │  │  ● def67890...  ⏳ Waiting         │            │ │
│  │  └─────────────────────────────────────           │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### Tech Stack

| Layer | Technology |
|-------|-----------|
| **HTTP/WS Server** | [Axum](https://github.com/tokio-rs/axum) + Tower |
| **PTY Management** | [portable-pty](https://github.com/wez/wezterm/tree/main/pty) |
| **Authentication** | [Argon2](https://github.com/RustCrypto/password-hashes) + Session Cookies |
| **Async Runtime** | [Tokio](https://tokio.rs/) |
| **Terminal UI** | [xterm.js](https://xtermjs.org/) + FitAddon + WebGLAddon |
| **Frontend** | Vanilla JS + CSS Grid/Flex (no frameworks) |
| **Asset Embedding** | [rust-embed](https://github.com/pyros2097/rust-embed) |
| **QR Codes** | [qrcode](https://github.com/kennytm/qrcode-rust) |

## 🔒 Security Model

| Layer | Implementation | Rationale |
|-------|---------------|-----------|
| **Access Control** | Argon2id (memory-hard), session cookie with 7-day expiry | Prevents brute force |
| **Rate Limiting** | 5 failed attempts per 10-minute window per IP | Thwarts password guessing |
| **Network Binding** | Defaults to `127.0.0.1`; `0.0.0.0` only with `--allow-lan` | No accidental public exposure |
| **Cookie Flags** | `HttpOnly`, `SameSite=Strict`, `Path=/` | Prevents XSS/CSRF |
| **Session Isolation** | Each PTY gets a unique UUID, no cross-console data leakage | Prevents state confusion |
| **Command Execution** | Shells run under host user, inherit user env | Matches SSH/tmux threat model |

> ⚠️ **Warning**: Tether is designed for **trusted LAN environments**. Do not expose it to the public internet without additional security layers (reverse proxy, TLS, firewall rules).

## 📂 Project Structure

```
tether/
├── Cargo.toml              # Rust dependencies and metadata
├── src/
│   ├── main.rs             # CLI entry point (clap)
│   ├── server.rs           # Axum HTTP server, routing, static assets
│   ├── pty_manager.rs      # PTY lifecycle (spawn, resize, kill)
│   ├── auth.rs             # Argon2 password hashing, session management
│   ├── ws_handler.rs       # WebSocket ↔ PTY bridge
│   └── state.rs            # Shared application state
├── assets/
│   ├── index.html          # Main dashboard HTML
│   ├── login.html          # Password gate page
│   ├── css/
│   │   └── style.css       # Mobile-first responsive styles
│   └── js/
│       └── app.js          # Frontend logic (WS, xterm, drawer)
├── .github/
│   ├── workflows/
│   │   ├── ci.yml          # Build + test on push
│   │   └── release.yml     # Automated release binaries
│   └── pages/
│       └── docs/           # GitHub Pages documentation site
└── README.md               # You are here
```

## 🧪 Development

```bash
# Clone and enter
git clone https://github.com/ssnelavala-masstcs/tether.git
cd tether

# Run in development mode (with logging)
RUST_LOG=debug cargo run -- serve --password "dev"

# Run tests
cargo test

# Build release binary
cargo build --release

# Check code quality
cargo clippy -- -D warnings
cargo fmt --check
```

## 🎯 Success Metrics

| Metric | Target | How to Verify |
|--------|--------|---------------|
| Input→Output Latency | ≤150ms on LAN | WebSocket round-trip benchmark |
| Memory Footprint | ≤40MB for 5 shells | `ps aux` / `heaptrack` |
| Mobile UX Score | ≥85 Lighthouse | Chrome DevTools audit |
| Auth Bypass Attempts | 0 | Pen-test with curl |
| Input-Wait Detection | ≥90% accuracy | Regex prompt matching |

## 🐛 Known Limitations

- **iOS Keyboard**: May occasionally overlay terminal content. The app includes positioning fixes, but iOS Safari behavior can vary.
- **PTY Zombies**: Terminals are killed on WebSocket disconnect with a 30-second grace period.
- **Large Output**: Buffered to 10k lines; older output is trimmed automatically.
- **No TLS by Default**: LAN is assumed trusted. For production use, place behind a reverse proxy (nginx, caddy) with TLS.

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [xterm.js](https://xtermjs.org/) — The terminal rendering engine
- [portable-pty](https://github.com/wez/wezterm) — Cross-platform PTY implementation
- [Axum](https://github.com/tokio-rs/axum) — Ergonomic HTTP server framework
- [Argon2](https://github.com/P-H-C/phc-winner-argon2) — Winner of the Password Hashing Competition

---

<div align="center">
  <p>Made with ❤️ for mobile sysadmins</p>
  <p>
    <a href="https://github.com/ssnelavala-masstcs/tether/issues">Report Bug</a>
    ·
    <a href="https://github.com/ssnelavala-masstcs/tether/issues">Request Feature</a>
  </p>
</div>
