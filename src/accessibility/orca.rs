//! Orca screen reader management
//!
//! Handles starting Orca via systemd user service or directly.

use anyhow::{Context, Result};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use super::systemd;
use crate::config::AccessibilityConfig;

/// Global flag to track if Orca was started by us
static ORCA_STARTED: AtomicBool = AtomicBool::new(false);

/// Global handle to Orca process if started directly
static ORCA_PROCESS: Mutex<Option<Child>> = Mutex::const_new(None);

/// Start Orca screen reader
pub async fn start_orca(config: &AccessibilityConfig) -> Result<()> {
    // Check if Orca is already running
    if is_orca_running().await {
        info!("Orca is already running");
        return Ok(());
    }

    // Ensure AT-SPI bus is available
    ensure_atspi_bus().await?;

    // Always start Orca directly (not via systemd) so it inherits
    // waygreet's environment variables (WAYLAND_DISPLAY, AT-SPI bus, etc.)
    // The systemd orca.service doesn't have these set correctly.
    start_orca_directly(config).await?;
    ORCA_STARTED.store(true, Ordering::SeqCst);

    Ok(())
}

/// Start Orca via systemd user service
#[allow(dead_code)]
async fn start_orca_via_systemd() -> Result<()> {
    info!("Attempting to start Orca via systemd");

    // Check if orca.service is available
    if !systemd::is_service_available("orca.service").await {
        anyhow::bail!("orca.service not found");
    }

    // Start the service
    systemd::start_service("orca.service").await?;

    // Wait for Orca to initialize
    sleep(Duration::from_millis(1000)).await;

    // Verify it's running
    if systemd::is_service_active("orca.service").await {
        info!("Orca started via systemd");
        Ok(())
    } else {
        anyhow::bail!("Orca service did not start properly")
    }
}

/// Start Orca directly as a subprocess
async fn start_orca_directly(config: &AccessibilityConfig) -> Result<()> {
    info!("Starting Orca directly as subprocess");

    let mut cmd = Command::new(&config.orca_path);
    cmd.args(&config.orca_args);

    // Set up environment - ensure accessibility is enabled
    cmd.env("GTK_MODULES", "gail:atk-bridge");
    cmd.env("GNOME_ACCESSIBILITY", "1");

    // Log the key environment variables for debugging
    if let Ok(wayland_display) = std::env::var("WAYLAND_DISPLAY") {
        info!("WAYLAND_DISPLAY={}", wayland_display);
    } else {
        warn!("WAYLAND_DISPLAY not set!");
    }

    // Redirect stdout to null, but capture stderr for debugging
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()
        .with_context(|| format!("Failed to spawn {}", config.orca_path))?;

    info!("Orca started with PID {}", child.id().unwrap_or(0));

    // Spawn a task to log stderr output
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                debug!("Orca stderr: {}", line);
            }
        });
    }

    // Store the process handle
    let mut guard = ORCA_PROCESS.lock().await;
    *guard = Some(child);

    // Wait for Orca to initialize
    sleep(Duration::from_millis(1500)).await;

    // Check if it's still running
    if is_orca_running().await {
        info!("Orca is running");
        Ok(())
    } else {
        warn!("Orca may have exited early - check logs for stderr output");
        Ok(()) // Don't fail - it might start up slower
    }
}

/// Check if Orca is currently running
pub async fn is_orca_running() -> bool {
    let output = Command::new("pgrep")
        .args(["-x", "orca"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    output.map(|s| s.success()).unwrap_or(false)
}

/// Ensure the AT-SPI D-Bus bus is available
async fn ensure_atspi_bus() -> Result<()> {
    // Check if at-spi-bus-launcher is available via systemd
    if systemd::is_user_session_available().await
        && systemd::is_service_available("at-spi-dbus-bus.service").await
        && !systemd::is_service_active("at-spi-dbus-bus.service").await
    {
        debug!("Starting at-spi-dbus-bus.service");
        if let Err(e) = systemd::start_service("at-spi-dbus-bus.service").await {
            warn!("Failed to start AT-SPI bus service: {}", e);
        }
    }

    // Also try to start the launcher directly if needed
    let output = Command::new("at-spi-bus-launcher")
        .args(["--launch-immediately"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match output {
        Ok(mut child) => {
            // Let it run in the background
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            debug!("Started at-spi-bus-launcher");
        }
        Err(e) => {
            debug!("Could not start at-spi-bus-launcher: {}", e);
            // This is okay - it might already be running or started by systemd
        }
    }

    // Wait a moment for the bus to be ready
    sleep(Duration::from_millis(200)).await;

    Ok(())
}

/// Stop Orca screen reader
#[allow(dead_code)]
pub async fn stop_orca() -> Result<()> {
    if !ORCA_STARTED.load(Ordering::SeqCst) {
        return Ok(());
    }

    info!("Stopping Orca");

    // If started via systemd, stop it that way
    if systemd::is_systemctl_available().await
        && systemd::is_service_active("orca.service").await
    {
        let _ = systemd::stop_service("orca.service").await;
    }

    // If we have a direct process handle, kill it
    let mut guard = ORCA_PROCESS.lock().await;
    if let Some(ref mut child) = *guard {
        let _ = child.kill().await;
        *guard = None;
    }

    ORCA_STARTED.store(false, Ordering::SeqCst);

    Ok(())
}

/// Restart Orca (useful if it crashes)
#[allow(dead_code)]
pub async fn restart_orca(config: &AccessibilityConfig) -> Result<()> {
    stop_orca().await?;
    sleep(Duration::from_millis(500)).await;
    start_orca(config).await
}
