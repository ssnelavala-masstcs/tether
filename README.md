# 🪢 Tether

> **Your terminal, anywhere.** A lightweight, mobile-optimized web terminal controller that mirrors your laptop's tmux sessions and spawns fresh PTY shells — zero cloud dependencies, fully self-hosted.

<div align="center">

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-blue.svg)](https://www.rust-lang.org)
[![Release](https://img.shields.io/badge/version-2.0.0-green.svg)](https://github.com/ssnelavala-masstcs/tether/releases)

</div>

## ✨ Features

| Feature | Description |
|---------|-------------|
| 📱 **Mobile-First UI** | Touch-optimized, responsive xterm.js interface designed for phones |
| 🔐 **Password Auth** | Argon2id hashed passwords with session cookies & rate limiting |
| 🖥️ **tmux Session Mirroring** | Auto-discovers tmux sessions — see and control your laptop terminals from your phone |
| 🆕 **Fresh PTY Terminals** | Spawn new bash sessions with full PTY control when tmux isn't available |
| ⚡ **Real-Time** | WebSocket-based streaming with ≤150ms input→output latency |
| 📡 **LAN Access** | Auto-detects LAN IP, prints QR code for instant phone access |
| 🔒 **Secure by Default** | Binds to localhost unless `--allow-lan` is explicitly set |
| 📦 **Single Binary** | Frontend embedded via rust-embed — no npm, no external deps |
| 🔄 **Bidirectional Sync** | What you type in the browser appears in tmux, and vice versa |

## 🚀 Quick Start

### One-Liner

```bash
cargo run --release -- serve --password "yourpassword" --allow-lan
```

Then open `http://<your-laptop-ip>:8080` on your phone.

### Installation

#### From Source

```bash
git clone https://github.com/ssnelavala-masstcs/tether.git
cd tether
cargo build --release
./target/release/tether serve --password "mypassword" --allow-lan
```

#### Pre-built Binaries

Download from [Releases](https://github.com/ssnelavala-masstcs/tether/releases).

### Usage

```bash
# Basic usage (localhost only)
./target/release/tether serve --password "secret"

# Allow LAN access (binds to 0.0.0.0)
./target/release/tether serve --password "secret" --allow-lan

# Custom port
./target/release/tether serve --password "secret" --port 3000 --allow-lan
```

## 📱 How It Works

### tmux Session Mirroring (Recommended)

If you have **tmux** running with sessions, Tether will automatically discover them and show them as panels in the web app:

```
Your Laptop                          Your Phone
┌─────────────────────┐              ┌─────────────────────┐
│  tmux session: work  │   ◄──────►   │  Panel: bash (%0)   │
│  ┌─────────────────┐ │   WiFi/WS   │  ┌───────────────┐  │
│  │ $ ls             │ │ ◄───────►  │  │ $ ls           │  │
│  │ file1  file2     │ │            │  │ file1  file2   │  │
│  │ $ _              │ │            │  │ $ _            │  │
│  └─────────────────┘ │            │  └───────────────┘  │
└─────────────────────┘              └─────────────────────┘
```

- **Output**: Captured via `tmux capture-pane` every 200ms
- **Input**: Sent via `tmux send-keys`
- **Bidirectional**: Works both ways — type in tmux, see it in browser; type in browser, see it in tmux

### Fresh PTY Terminals

If no tmux sessions exist, or you click **"+ New"**, Tether spawns a brand new bash session using raw `forkpty`:

- Full PTY control with proper terminal emulation
- Independent from your laptop's existing terminals
- Useful for quick commands when you don't need to mirror

## 📖 User Guide

### Getting Started

1. **Install tmux** (recommended for session mirroring):
   ```bash
   sudo apt install tmux   # Debian/Ubuntu
   brew install tmux       # macOS
   ```

2. **Start tmux sessions** on your laptop:
   ```bash
   tmux new -s work    # Create a session named "work"
   tmux new -s monitor # Create another session
   ```

3. **Start Tether**:
   ```bash
   ./target/release/tether serve --password "yourpassword" --allow-lan
   ```

4. **Open the URL** on your phone (printed in the server logs or scan the QR code)

5. **Login** with your password

6. **Your tmux sessions appear as tabs** — tap to switch between them

### Using the Web Terminal

| Action | How |
|--------|-----|
| **Type commands** | Use the on-screen keyboard — works like a real terminal |
| **Switch terminals** | Tap tabs at the top |
| **Close a terminal** | Tap the ✕ on the tab |
| **Create new terminal** | Tap "+ New" button |
| **Reconnect** | Tap the ↻ refresh button |

### tmux Tips

- Sessions persist across Tether restarts — tmux keeps running independently
- Resize your phone browser and the terminal auto-adjusts
- Long-running commands (like `top`, `htop`) stream output in real-time
- Copy/paste works: select text in the terminal, use your phone's clipboard

### Accessing from Your Phone

1. Start the server with `--allow-lan`
2. Note the printed IP address or scan the QR code
3. Open the URL on your phone
4. Enter the password
5. Start controlling your laptop from your couch 🛋️

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      Your Laptop                         │
│                                                          │
│  ┌──────────────┐    ┌──────────────┐    ┌────────────┐ │
│  │   Axum HTTP  │◄──►│   PTY Mgr    │◄──►│  bash/sh   │ │
│  │   + WS Srv   │    │ (forkpty)    │    │  sessions  │ │
│  └──────┬───────┘    └──────────────┘    └────────────┘ │
│         │                                                 │
│  ┌──────┴───────┐    ┌──────────────┐    ┌────────────┐ │
│  │   Mirror Mgr │◄──►│   tmux CLI   │◄──►│  tmux      │ │
│  │  (discover)  │    │  capture/send│    │  sessions  │ │
│  └──────────────┘    └──────────────┘    └────────────┘ │
│                                                          │
│         │ WebSocket (real-time I/O)                       │
│         ▼                                                 │
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
│  │  [Tab 1] [Tab 2] [Tab 3]     [+ New] [↻]          │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### Tech Stack

| Layer | Technology |
|-------|-----------|
| **HTTP/WS Server** | [Axum](https://github.com/tokio-rs/axum) + Tower |
| **PTY Management** | Raw `forkpty` via [nix](https://github.com/nix-rust/nix) crate |
| **tmux Mirroring** | `tmux capture-pane` + `tmux send-keys` |
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

> ⚠️ **Warning**: Tether is designed for **trusted LAN environments**. Do not expose it to the public internet without additional security layers (reverse proxy, TLS, firewall rules).

## 📂 Project Structure

```
tether/
├── Cargo.toml              # Rust dependencies and metadata
├── src/
│   ├── main.rs             # CLI entry point (clap)
│   ├── server.rs           # Axum HTTP server, routing, static assets
│   ├── pty_manager.rs      # PTY lifecycle (forkpty: spawn, resize, kill)
│   ├── terminal_mirror.rs  # tmux session discovery and mirroring
│   ├── auth.rs             # Argon2 password hashing, session management
│   ├── ws_handler.rs       # WebSocket ↔ PTY/tmux bridge
│   └── state.rs            # Shared application state
├── assets/
│   ├── index.html          # Main dashboard HTML
│   ├── login.html          # Password gate page
│   ├── css/
│   │   └── style.css       # Mobile-first responsive styles
│   └── js/
│       └── app.js          # Frontend logic (WS, xterm, auto-discover)
├── .github/
│   └── workflows/
│       ├── ci.yml          # Build + test on push
│       └── release.yml     # Automated release binaries
└── README.md               # You are here
```

## 🧪 Development

```bash
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

## 🐛 Known Limitations

- **iOS Keyboard**: May occasionally overlay terminal content. The app includes positioning fixes, but iOS Safari behavior can vary.
- **PTY Zombies**: Terminals are killed on WebSocket disconnect with a 30-second grace period.
- **Large Output**: Buffered to 10k lines; older output is trimmed automatically.
- **No TLS by Default**: LAN is assumed trusted. For production use, place behind a reverse proxy (nginx, caddy) with TLS.
- **tmux Required for Mirroring**: Without tmux, only fresh PTY terminals are available.

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
- [nix](https://github.com/nix-rust/nix) — POSIX API bindings for Rust
- [Axum](https://github.com/tokio-rs/axum) — Ergonomic HTTP server framework
- [Argon2](https://github.com/P-H-C/phc-winner-argon2) — Winner of the Password Hashing Competition
- [tmux](https://github.com/tmux/tmux) — Terminal multiplexer used for session mirroring

---

<div align="center">
  <p>Made with ❤️ for mobile sysadmins</p>
  <p>
    <a href="https://github.com/ssnelavala-masstcs/tether/issues">Report Bug</a>
    ·
    <a href="https://github.com/ssnelavala-masstcs/tether/issues">Request Feature</a>
  </p>
</div>
