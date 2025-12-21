//! systemd user service control via D-Bus
//!
//! Provides functionality to start, stop, and query systemd user services.

use anyhow::Result;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, info, warn};

/// Default timeout for systemctl commands (2 seconds)
const SYSTEMCTL_TIMEOUT: Duration = Duration::from_secs(2);

/// Check if systemd user session is available and responsive
pub async fn is_user_session_available() -> bool {
    let result = timeout(
        SYSTEMCTL_TIMEOUT,
        Command::new("systemctl")
            .args(["--user", "is-system-running"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            // Any response means the user session is working
            // (could be "running", "degraded", "starting", etc.)
            let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
            debug!("systemd user session status: {}", status);
            !status.is_empty() && status != "offline"
        }
        Ok(Err(e)) => {
            debug!("systemctl failed: {}", e);
            false
        }
        Err(_) => {
            warn!("systemctl --user timed out - no user session available");
            false
        }
    }
}

/// Check if a systemd user service unit file exists
pub async fn is_service_available(service_name: &str) -> bool {
    let result = timeout(
        SYSTEMCTL_TIMEOUT,
        Command::new("systemctl")
            .args(["--user", "cat", service_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await;

    match result {
        Ok(Ok(status)) => status.success(),
        Ok(Err(_)) | Err(_) => false,
    }
}

/// Check if a systemd user service is currently active
pub async fn is_service_active(service_name: &str) -> bool {
    let result = timeout(
        SYSTEMCTL_TIMEOUT,
        Command::new("systemctl")
            .args(["--user", "is-active", service_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
            status == "active"
        }
        Ok(Err(_)) | Err(_) => false,
    }
}

/// Start a systemd user service
pub async fn start_service(service_name: &str) -> Result<()> {
    debug!("Starting systemd user service: {}", service_name);

    let result = timeout(
        Duration::from_secs(5), // Longer timeout for starting services
        Command::new("systemctl")
            .args(["--user", "start", service_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            info!("Started service: {}", service_name);
            Ok(())
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to start {}: {}", service_name, stderr.trim())
        }
        Ok(Err(e)) => {
            anyhow::bail!("Failed to run systemctl: {}", e)
        }
        Err(_) => {
            anyhow::bail!("Timed out starting {}", service_name)
        }
    }
}

/// Stop a systemd user service
#[allow(dead_code)]
pub async fn stop_service(service_name: &str) -> Result<()> {
    debug!("Stopping systemd user service: {}", service_name);

    let result = timeout(
        Duration::from_secs(5),
        Command::new("systemctl")
            .args(["--user", "stop", service_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            info!("Stopped service: {}", service_name);
            Ok(())
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to stop {}: {}", service_name, stderr.trim())
        }
        Ok(Err(e)) => {
            anyhow::bail!("Failed to run systemctl: {}", e)
        }
        Err(_) => {
            anyhow::bail!("Timed out stopping {}", service_name)
        }
    }
}

/// Start a systemd user socket (for socket activation)
pub async fn start_socket(socket_name: &str) -> Result<()> {
    debug!("Starting systemd user socket: {}", socket_name);

    let result = timeout(
        Duration::from_secs(5),
        Command::new("systemctl")
            .args(["--user", "start", socket_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            info!("Started socket: {}", socket_name);
            Ok(())
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Socket might already be running, which is fine
            if stderr.contains("already") {
                debug!("Socket {} already active", socket_name);
                Ok(())
            } else {
                anyhow::bail!("Failed to start {}: {}", socket_name, stderr.trim())
            }
        }
        Ok(Err(e)) => {
            anyhow::bail!("Failed to run systemctl: {}", e)
        }
        Err(_) => {
            anyhow::bail!("Timed out starting {}", socket_name)
        }
    }
}

/// Enable and start a systemd user service
#[allow(dead_code)]
pub async fn enable_and_start_service(service_name: &str) -> Result<()> {
    // First try to enable
    let _ = Command::new("systemctl")
        .args(["--user", "enable", service_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    // Then start
    start_service(service_name).await
}

/// Check if systemctl is available
pub async fn is_systemctl_available() -> bool {
    Command::new("systemctl")
        .args(["--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Get the status of a service
#[allow(dead_code)]
pub async fn get_service_status(service_name: &str) -> Option<String> {
    let output = Command::new("systemctl")
        .args(["--user", "status", service_name, "--no-pager"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}
