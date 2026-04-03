mod auth;
mod pty_manager;
mod server;
mod state;
mod ws_handler;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "tether")]
#[command(about = "Remote Terminal Controller - Local Web Server + WebSocket PTY Bridge")]
struct Args {
    /// Command to run
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Start the tether server
    Serve {
        /// Password for authentication
        #[arg(short, long)]
        password: Option<String>,

        /// Port to bind to
        #[arg(short = 'P', long, default_value = "8080")]
        port: u16,

        /// Allow LAN access (binds to 0.0.0.0 instead of 127.0.0.1)
        #[arg(long, default_value = "false")]
        allow_lan: bool,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    let args = Args::parse();

    match args.command {
        Commands::Serve {
            password,
            port,
            allow_lan,
        } => {
            server::start_server(password, port, allow_lan).await;
        }
    }
}
