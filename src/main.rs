//! Waygreet - Accessibility-first Wayland GTK greeter for greetd
//!
//! This greeter prioritizes accessibility by starting Orca screen reader
//! and PipeWire audio by default.

mod accessibility;
mod app;
mod components;
mod config;
mod greetd;
mod session_env;
mod sessions;
mod style;

use anyhow::{Context, Result};
use clap::Parser;
use gtk4::prelude::*;
use relm4::RelmApp;
use std::path::PathBuf;
use tracing::{info, warn};

use crate::app::App;
use crate::config::Config;

/// Accessibility-first Wayland GTK greeter for greetd
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/greetd/waygreet.toml")]
    config: PathBuf,

    /// Path to custom CSS stylesheet
    #[arg(short, long)]
    style: Option<PathBuf>,

    /// Run in demo mode (no greetd connection)
    #[arg(long)]
    demo: bool,

    /// Skip starting accessibility services
    #[arg(long)]
    no_accessibility: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

fn setup_logging(level: &str) -> Result<()> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    Ok(())
}

fn setup_accessibility_environment() {
    // Set AT-SPI environment variables for accessibility
    std::env::set_var("GTK_MODULES", "gail:atk-bridge");
    std::env::set_var("GNOME_ACCESSIBILITY", "1");
    std::env::set_var("QT_ACCESSIBILITY", "1");
    std::env::set_var("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1");

    info!("Accessibility environment variables set");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Disable services that aren't needed for a greeter and cause delays
    std::env::set_var("GTK_USE_PORTAL", "0"); // Disable portal file dialogs
    std::env::set_var("GIO_USE_VFS", "local"); // Disable gvfs, use local VFS only
    std::env::set_var("GSETTINGS_BACKEND", "memory"); // Don't need dconf

    // Set up logging first
    setup_logging(&args.log_level)?;

    info!("Waygreet starting...");

    // Load configuration
    let config = Config::load(&args.config).unwrap_or_else(|e| {
        warn!("Failed to load config from {:?}: {}, using defaults", args.config, e);
        Config::default()
    });

    // Override config with CLI arguments
    let config = config.with_cli_overrides(&args);

    // Set up session environment (XDG_RUNTIME_DIR, D-Bus)
    if let Err(e) = session_env::ensure_session_environment().await {
        warn!("Failed to set up session environment: {}", e);
        // Continue anyway - some features may not work
    }

    // Set up accessibility environment variables
    setup_accessibility_environment();

    // Start audio services if enabled (audio can start before GTK)
    if !args.no_accessibility && config.accessibility.enable_audio {
        match accessibility::audio::start_audio().await {
            Ok(()) => info!("Audio services started"),
            Err(e) => warn!("Failed to start audio services: {}", e),
        }
    }

    // Note: Orca is started AFTER GTK initializes, from the App component
    // This is because Orca needs AT-SPI bus which GTK creates

    // Initialize GTK
    gtk4::init().context("Failed to initialize GTK4")?;

    // Load CSS styling
    style::load_css(&config, args.style.as_deref())?;

    // Run the Relm4 application
    // Pass empty args to prevent GTK from parsing our CLI arguments
    let app = RelmApp::new("org.waygreet.Greeter")
        .with_args(Vec::<String>::new());

    // Set up layer shell before showing window
    // Pass config, demo_mode, and no_accessibility flag
    app.run_async::<App>((config, args.demo, args.no_accessibility));

    Ok(())
}
