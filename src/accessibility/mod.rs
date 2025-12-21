//! Accessibility services management
//!
//! This module handles starting and managing accessibility services:
//! - Orca screen reader
//! - PipeWire audio
//! - systemd user service control

pub mod audio;
pub mod orca;
pub mod systemd;
