//! Audio service management (PipeWire)
//!
//! Handles starting PipeWire and WirePlumber for audio output,
//! which is required for the screen reader to work.

use anyhow::Result;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use super::systemd;

/// Start audio services (PipeWire + WirePlumber)
pub async fn start_audio() -> Result<()> {
    // Check if systemd user session is available and responsive
    if systemd::is_user_session_available().await {
        start_audio_via_systemd().await
    } else {
        info!("systemd user session not available, starting audio directly");
        start_audio_directly().await
    }
}

/// Start audio services via systemd
async fn start_audio_via_systemd() -> Result<()> {
    info!("Starting audio services via systemd");

    // Check if PipeWire socket is available
    let pipewire_available = systemd::is_service_available("pipewire.socket").await
        || systemd::is_service_available("pipewire.service").await;

    if !pipewire_available {
        warn!("PipeWire service not found in systemd");
        return start_audio_directly().await;
    }

    // Check if PipeWire is already running
    if systemd::is_service_active("pipewire.service").await {
        info!("PipeWire is already running");
    } else {
        // Start PipeWire socket (this triggers socket activation)
        if systemd::is_service_available("pipewire.socket").await {
            if let Err(e) = systemd::start_socket("pipewire.socket").await {
                warn!("Failed to start pipewire.socket: {}", e);
                // Try starting the service directly
                systemd::start_service("pipewire.service").await?;
            }
        } else {
            systemd::start_service("pipewire.service").await?;
        }
    }

    // Start WirePlumber if available and not already running
    if systemd::is_service_available("wireplumber.service").await {
        if systemd::is_service_active("wireplumber.service").await {
            info!("WirePlumber is already running");
        } else if let Err(e) = systemd::start_service("wireplumber.service").await {
            warn!("Failed to start WirePlumber: {}", e);
            // Continue without WirePlumber - basic audio may still work
        }
    } else {
        debug!("WirePlumber service not available");
    }

    // Start PipeWire-Pulse for PulseAudio compatibility (needed by Orca)
    // Check if already running first
    if systemd::is_service_active("pipewire-pulse.service").await {
        info!("pipewire-pulse is already running");
    } else if systemd::is_service_available("pipewire-pulse.service").await {
        if let Err(e) = systemd::start_service("pipewire-pulse.service").await {
            warn!("Failed to start pipewire-pulse: {}", e);
        }
    } else if systemd::is_service_available("pipewire-pulse.socket").await {
        if let Err(e) = systemd::start_socket("pipewire-pulse.socket").await {
            warn!("Failed to start pipewire-pulse.socket: {}", e);
        }
    } else {
        debug!("pipewire-pulse service not available");
    }

    // Wait a moment for services to initialize (only if we started something)
    sleep(Duration::from_millis(500)).await;

    // Verify PipeWire is running
    if systemd::is_service_active("pipewire.service").await {
        info!("PipeWire audio is running");
        Ok(())
    } else {
        warn!("PipeWire may not be running properly");
        Ok(()) // Don't fail - audio might still work
    }
}

/// Start audio services directly (without systemd)
async fn start_audio_directly() -> Result<()> {
    info!("Starting audio services directly");

    // Check if PipeWire is already running
    if is_pipewire_running().await {
        info!("PipeWire is already running");
    } else {
        // Try to start PipeWire
        let pipewire_result = Command::new("pipewire")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match pipewire_result {
            Ok(mut child) => {
                // Don't wait for it - let it run in background
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
                info!("Started PipeWire directly");
            }
            Err(e) => {
                warn!("Failed to start PipeWire: {}", e);
                // Try PulseAudio as fallback
                return start_pulseaudio_fallback().await;
            }
        }

        // Wait a moment for PipeWire to initialize
        sleep(Duration::from_millis(300)).await;
    }

    // Check if WirePlumber is already running
    if is_process_running("wireplumber").await {
        info!("WirePlumber is already running");
    } else {
        // Try to start WirePlumber
        let wireplumber_result = Command::new("wireplumber")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match wireplumber_result {
            Ok(mut child) => {
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
                info!("Started WirePlumber directly");
            }
            Err(e) => {
                debug!("Could not start WirePlumber: {}", e);
                // Continue without WirePlumber
            }
        }

        // Wait a moment before starting pipewire-pulse
        sleep(Duration::from_millis(300)).await;
    }

    // Check if pipewire-pulse is already running
    if is_process_running("pipewire-pulse").await {
        info!("pipewire-pulse is already running");
    } else {
        // Try to start pipewire-pulse for PulseAudio compatibility
        let pulse_result = Command::new("pipewire-pulse")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match pulse_result {
            Ok(mut child) => {
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
                info!("Started pipewire-pulse directly");
            }
            Err(e) => {
                debug!("Could not start pipewire-pulse: {}", e);
            }
        }
    }

    // Give services time to fully initialize
    sleep(Duration::from_millis(500)).await;

    Ok(())
}

/// Check if PipeWire is already running
async fn is_pipewire_running() -> bool {
    is_process_running("pipewire").await
}

/// Check if a process is already running by name
async fn is_process_running(name: &str) -> bool {
    let output = Command::new("pgrep")
        .args(["-x", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    output.map(|s| s.success()).unwrap_or(false)
}

/// Fallback to PulseAudio if PipeWire is not available
async fn start_pulseaudio_fallback() -> Result<()> {
    info!("Attempting PulseAudio fallback");

    // Check if PulseAudio is already running
    let output = Command::new("pgrep")
        .args(["-x", "pulseaudio"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    if output.map(|s| s.success()).unwrap_or(false) {
        info!("PulseAudio is already running");
        return Ok(());
    }

    // Try to start PulseAudio
    let result = Command::new("pulseaudio")
        .args(["--start"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    match result {
        Ok(status) if status.success() => {
            info!("Started PulseAudio");
            Ok(())
        }
        Ok(_) => {
            warn!("PulseAudio failed to start");
            Ok(()) // Don't fail - continue without audio
        }
        Err(e) => {
            warn!("Could not start PulseAudio: {}", e);
            Ok(()) // Don't fail - continue without audio
        }
    }
}

/// Stop audio services (called on shutdown if needed)
#[allow(dead_code)]
pub async fn stop_audio() -> Result<()> {
    // Don't stop audio services - let them keep running
    // Other applications might need them
    debug!("Leaving audio services running");
    Ok(())
}
