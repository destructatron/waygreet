//! Session selector component
//!
//! Dropdown for selecting the login session.

use gtk4::prelude::*;
use relm4::prelude::*;

use crate::sessions::Session;

/// Session selector input messages
#[derive(Debug)]
pub enum SessionSelectorInput {
    /// Set available sessions
    SetSessions(Vec<Session>),
    /// Select a session by index
    SelectByIndex(usize),
    /// Select next session
    SelectNext,
    /// Select previous session
    SelectPrevious,
}

/// Session selector output messages
#[derive(Debug, Clone)]
pub enum SessionSelectorOutput {
    /// User selected a session
    SessionSelected(Session),
}

/// Session selector state
pub struct SessionSelector {
    sessions: Vec<Session>,
    selected_index: usize,
}

impl SessionSelector {
    /// Get the currently selected session
    pub fn selected_session(&self) -> Option<&Session> {
        self.sessions.get(self.selected_index)
    }
}

#[relm4::component(pub)]
impl SimpleComponent for SessionSelector {
    type Init = Vec<Session>;
    type Input = SessionSelectorInput;
    type Output = SessionSelectorOutput;

    view! {
        gtk4::Box {
            set_orientation: gtk4::Orientation::Horizontal,
            set_spacing: 8,
            set_halign: gtk4::Align::Center,

            gtk4::Label {
                set_label: "Session:",
                set_css_classes: &["caption"],
            },

            #[name = "dropdown"]
            gtk4::DropDown {
                set_hexpand: false,
                #[watch]
                set_model: Some(&create_string_list(&model.sessions)),
                #[watch]
                set_selected: model.selected_index as u32,

                // Accessibility
                update_property: &[gtk4::accessible::Property::Label("Session selector")],

                connect_selected_notify[sender] => move |dropdown| {
                    let idx = dropdown.selected() as usize;
                    sender.input(SessionSelectorInput::SelectByIndex(idx));
                },
            },

            // Session type indicator
            gtk4::Label {
                #[watch]
                set_label: &format_session_type(&model.sessions, model.selected_index),
                set_css_classes: &["dim-label", "caption"],
            },
        }
    }

    fn init(
        sessions: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = SessionSelector {
            sessions,
            selected_index: 0,
        };

        let widgets = view_output!();

        // Emit initial selection
        if let Some(session) = model.sessions.first() {
            let _ = sender.output(SessionSelectorOutput::SessionSelected(session.clone()));
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            SessionSelectorInput::SetSessions(sessions) => {
                self.sessions = sessions;
                self.selected_index = 0;

                // Emit new selection
                if let Some(session) = self.sessions.first() {
                    let _ = sender.output(SessionSelectorOutput::SessionSelected(session.clone()));
                }
            }
            SessionSelectorInput::SelectByIndex(index) => {
                if index < self.sessions.len() && index != self.selected_index {
                    self.selected_index = index;

                    if let Some(session) = self.sessions.get(index) {
                        let _ = sender.output(SessionSelectorOutput::SessionSelected(session.clone()));
                    }
                }
            }
            SessionSelectorInput::SelectNext => {
                if !self.sessions.is_empty() {
                    let new_index = (self.selected_index + 1) % self.sessions.len();
                    sender.input(SessionSelectorInput::SelectByIndex(new_index));
                }
            }
            SessionSelectorInput::SelectPrevious => {
                if !self.sessions.is_empty() {
                    let new_index = if self.selected_index == 0 {
                        self.sessions.len() - 1
                    } else {
                        self.selected_index - 1
                    };
                    sender.input(SessionSelectorInput::SelectByIndex(new_index));
                }
            }
        }
    }
}

/// Create a StringList model from sessions
fn create_string_list(sessions: &[Session]) -> gtk4::StringList {
    let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
    gtk4::StringList::new(&names)
}

/// Format the session type for display
fn format_session_type(sessions: &[Session], index: usize) -> String {
    sessions
        .get(index)
        .map(|s| format!("({})", s.session_type))
        .unwrap_or_default()
}
