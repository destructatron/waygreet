//! Power menu component
//!
//! Provides reboot and shutdown buttons.

use gtk4::prelude::*;
use relm4::prelude::*;
use std::process::Command;
use tracing::{error, info};

use crate::config::CommandsConfig;

/// Power menu input messages
#[derive(Debug)]
pub enum PowerMenuInput {
    /// Reboot the system
    Reboot,
    /// Shutdown the system
    Shutdown,
    /// Show confirmation dialog
    ShowConfirmation(PowerAction),
    /// Confirm action
    ConfirmAction,
    /// Cancel action
    CancelAction,
}

/// Power menu output messages
#[derive(Debug)]
pub enum PowerMenuOutput {
    /// Action completed
    ActionCompleted,
    /// Action failed
    ActionFailed(String),
}

/// Power action type
#[derive(Debug, Clone, PartialEq)]
pub enum PowerAction {
    Reboot,
    Shutdown,
}

/// Power menu state
pub struct PowerMenu {
    config: CommandsConfig,
    pending_action: Option<PowerAction>,
    enabled: bool,
}

#[relm4::component(pub)]
impl SimpleComponent for PowerMenu {
    type Init = CommandsConfig;
    type Input = PowerMenuInput;
    type Output = PowerMenuOutput;

    view! {
        gtk4::Box {
            set_orientation: gtk4::Orientation::Horizontal,
            set_spacing: 8,
            set_halign: gtk4::Align::End,
            set_valign: gtk4::Align::End,
            set_margin_all: 16,
            #[watch]
            set_visible: model.enabled,

            // Confirmation overlay
            #[name = "confirmation_box"]
            gtk4::Box {
                set_orientation: gtk4::Orientation::Horizontal,
                set_spacing: 8,
                #[watch]
                set_visible: model.pending_action.is_some(),

                gtk4::Label {
                    #[watch]
                    set_label: &format_confirmation(&model.pending_action),
                },

                gtk4::Button {
                    set_label: "Yes",
                    set_css_classes: &["destructive-action"],

                    connect_clicked[sender] => move |_| {
                        sender.input(PowerMenuInput::ConfirmAction);
                    },
                },

                gtk4::Button {
                    set_label: "No",

                    connect_clicked[sender] => move |_| {
                        sender.input(PowerMenuInput::CancelAction);
                    },
                },
            },

            // Power buttons
            #[name = "buttons_box"]
            gtk4::Box {
                set_orientation: gtk4::Orientation::Horizontal,
                set_spacing: 8,
                #[watch]
                set_visible: model.pending_action.is_none(),

                gtk4::Button {
                    set_icon_name: "system-reboot-symbolic",
                    set_tooltip_text: Some("Reboot"),
                    set_css_classes: &["circular"],

                    // Accessibility
                    update_property: &[gtk4::accessible::Property::Label("Reboot system")],

                    connect_clicked[sender] => move |_| {
                        sender.input(PowerMenuInput::ShowConfirmation(PowerAction::Reboot));
                    },
                },

                gtk4::Button {
                    set_icon_name: "system-shutdown-symbolic",
                    set_tooltip_text: Some("Shutdown"),
                    set_css_classes: &["circular"],

                    // Accessibility
                    update_property: &[gtk4::accessible::Property::Label("Shutdown system")],

                    connect_clicked[sender] => move |_| {
                        sender.input(PowerMenuInput::ShowConfirmation(PowerAction::Shutdown));
                    },
                },
            },
        }
    }

    fn init(
        config: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let _ = &sender; // Silence unused warning, sender is used in view! macro
        let enabled = config.enable_power_menu;

        let model = PowerMenu {
            config,
            pending_action: None,
            enabled,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PowerMenuInput::Reboot => {
                sender.input(PowerMenuInput::ShowConfirmation(PowerAction::Reboot));
            }
            PowerMenuInput::Shutdown => {
                sender.input(PowerMenuInput::ShowConfirmation(PowerAction::Shutdown));
            }
            PowerMenuInput::ShowConfirmation(action) => {
                self.pending_action = Some(action);
            }
            PowerMenuInput::ConfirmAction => {
                if let Some(action) = self.pending_action.take() {
                    let result = match action {
                        PowerAction::Reboot => execute_command(&self.config.reboot),
                        PowerAction::Shutdown => execute_command(&self.config.shutdown),
                    };

                    match result {
                        Ok(()) => {
                            let _ = sender.output(PowerMenuOutput::ActionCompleted);
                        }
                        Err(e) => {
                            let _ = sender.output(PowerMenuOutput::ActionFailed(e));
                        }
                    }
                }
            }
            PowerMenuInput::CancelAction => {
                self.pending_action = None;
            }
        }
    }
}

/// Format the confirmation message
fn format_confirmation(action: &Option<PowerAction>) -> String {
    match action {
        Some(PowerAction::Reboot) => "Reboot now?".to_string(),
        Some(PowerAction::Shutdown) => "Shutdown now?".to_string(),
        None => String::new(),
    }
}

/// Execute a power command
fn execute_command(cmd: &[String]) -> Result<(), String> {
    if cmd.is_empty() {
        return Err("Empty command".to_string());
    }

    info!("Executing power command: {:?}", cmd);

    let result = Command::new(&cmd[0])
        .args(&cmd[1..])
        .status();

    match result {
        Ok(status) if status.success() => {
            info!("Power command executed successfully");
            Ok(())
        }
        Ok(status) => {
            let msg = format!("Command failed with status: {}", status);
            error!("{}", msg);
            Err(msg)
        }
        Err(e) => {
            let msg = format!("Failed to execute command: {}", e);
            error!("{}", msg);
            Err(msg)
        }
    }
}
