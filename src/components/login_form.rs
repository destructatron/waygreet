//! Login form component
//!
//! Handles username and password entry with accessibility support.

use gtk4::prelude::*;
use gtk4::gdk;
use relm4::prelude::*;

/// Login form input messages
#[derive(Debug)]
pub enum LoginFormInput {
    /// Set the username
    SetUsername(String),
    /// Append a character to the password (sudo-style, no echo)
    PasswordKeyPress(char),
    /// Delete last character from password
    PasswordBackspace,
    /// Clear the password
    PasswordClear,
    /// Submit the form
    Submit,
    /// Clear the form
    Clear,
    /// Toggle password visibility
    TogglePasswordVisibility,
    /// Focus the username field
    FocusUsername,
    /// Focus the password field
    FocusPassword,
    /// Set the authentication prompt message
    SetPrompt(String),
    /// Show an error message
    ShowError(String),
    /// Clear error message
    ClearError,
    /// Enable/disable the form
    SetEnabled(bool),
}

/// Login form output messages
#[derive(Debug)]
pub enum LoginFormOutput {
    /// User submitted credentials
    Submit { username: String, password: String },
}

/// Login form state
pub struct LoginForm {
    username: String,
    password: String,
    prompt: String,
    error: Option<String>,
    password_visible: bool,
    enabled: bool,
}

#[relm4::component(pub)]
impl SimpleComponent for LoginForm {
    type Init = Option<String>;
    type Input = LoginFormInput;
    type Output = LoginFormOutput;

    view! {
        gtk4::Box {
            set_orientation: gtk4::Orientation::Vertical,
            set_spacing: 16,
            set_halign: gtk4::Align::Center,
            set_valign: gtk4::Align::Center,
            set_width_request: 350,

            // Title/prompt
            gtk4::Label {
                #[watch]
                set_label: &model.prompt,
                set_css_classes: &["title-2"],
                set_halign: gtk4::Align::Start,
            },

            // Error message
            gtk4::Label {
                #[watch]
                set_visible: model.error.is_some(),
                #[watch]
                set_label: model.error.as_deref().unwrap_or(""),
                set_css_classes: &["error"],
                set_halign: gtk4::Align::Start,
                set_wrap: true,
            },

            // Username entry
            gtk4::Box {
                set_orientation: gtk4::Orientation::Vertical,
                set_spacing: 4,

                gtk4::Label {
                    set_label: "Username",
                    set_halign: gtk4::Align::Start,
                    set_css_classes: &["caption"],
                },

                #[name = "username_entry"]
                gtk4::Entry {
                    set_placeholder_text: Some("Enter username"),
                    set_hexpand: true,
                    #[watch]
                    set_sensitive: model.enabled,
                    set_activates_default: false,

                    // Accessibility
                    update_property: &[gtk4::accessible::Property::Label("Username")],

                    connect_changed[sender] => move |entry| {
                        sender.input(LoginFormInput::SetUsername(entry.text().to_string()));
                    },

                    connect_activate => move |entry| {
                        // Move focus forward (to password field)
                        let _ = entry.child_focus(gtk4::DirectionType::TabForward);
                    },
                },
            },

            // Password entry (sudo-style: no characters shown for security with screen readers)
            gtk4::Box {
                set_orientation: gtk4::Orientation::Vertical,
                set_spacing: 4,

                gtk4::Label {
                    set_label: "Password",
                    set_halign: gtk4::Align::Start,
                    set_css_classes: &["caption"],
                },

                #[name = "password_entry"]
                gtk4::Entry {
                    set_placeholder_text: Some("Enter password (no echo)"),
                    set_hexpand: true,
                    #[watch]
                    set_sensitive: model.enabled,
                    set_visibility: false,
                    set_invisible_char: Some('\0'), // Don't show any character
                    set_input_purpose: gtk4::InputPurpose::Password,
                    set_editable: false, // Prevent GTK from handling text - we do it manually

                    // Accessibility
                    update_property: &[
                        gtk4::accessible::Property::Label("Password, sudo style, no characters will be spoken"),
                        gtk4::accessible::Property::RoleDescription("secure password field"),
                    ],

                    connect_activate[sender] => move |_| {
                        sender.input(LoginFormInput::Submit);
                    },
                },
            },

            // Submit button
            gtk4::Button {
                set_label: "Log In",
                set_css_classes: &["suggested-action", "pill"],
                set_halign: gtk4::Align::End,
                #[watch]
                set_sensitive: model.enabled && !model.username.is_empty(),

                connect_clicked[sender] => move |_| {
                    sender.input(LoginFormInput::Submit);
                },
            },
        }
    }

    fn init(
        initial_username: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let username = initial_username.unwrap_or_default();

        let model = LoginForm {
            username: username.clone(),
            password: String::new(),
            prompt: "Log In".to_string(),
            error: None,
            password_visible: false,
            enabled: true,
        };

        let widgets = view_output!();

        // Set initial username on the widget (only once, not watched)
        if !username.is_empty() {
            widgets.username_entry.set_text(&username);
        }

        // Add key event controller to password entry for sudo-style input
        // We capture keys manually to prevent any text from being stored in the widget
        let key_controller = gtk4::EventControllerKey::new();
        let sender_clone = sender.clone();
        key_controller.connect_key_pressed(move |_controller, keyval, _keycode, _state| {
            // Handle special keys
            if keyval == gdk::Key::BackSpace {
                sender_clone.input(LoginFormInput::PasswordBackspace);
                return gtk4::glib::Propagation::Stop;
            }
            if keyval == gdk::Key::Delete {
                sender_clone.input(LoginFormInput::PasswordClear);
                return gtk4::glib::Propagation::Stop;
            }
            if keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter {
                // Let the activate signal handle this
                return gtk4::glib::Propagation::Proceed;
            }
            if keyval == gdk::Key::Tab || keyval == gdk::Key::ISO_Left_Tab {
                // Let tab navigation work
                return gtk4::glib::Propagation::Proceed;
            }
            if keyval == gdk::Key::Escape {
                return gtk4::glib::Propagation::Proceed;
            }

            // Convert keyval to character if possible
            if let Some(ch) = keyval.to_unicode() {
                // Only accept printable characters
                if !ch.is_control() {
                    sender_clone.input(LoginFormInput::PasswordKeyPress(ch));
                    return gtk4::glib::Propagation::Stop;
                }
            }

            gtk4::glib::Propagation::Proceed
        });
        widgets.password_entry.add_controller(key_controller);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            LoginFormInput::SetUsername(username) => {
                self.username = username;
            }
            LoginFormInput::PasswordKeyPress(ch) => {
                // Append character to password (sudo-style, nothing visible)
                self.password.push(ch);
            }
            LoginFormInput::PasswordBackspace => {
                // Remove last character from password
                self.password.pop();
            }
            LoginFormInput::PasswordClear => {
                // Clear entire password
                self.password.clear();
            }
            LoginFormInput::Submit => {
                if !self.username.is_empty() {
                    let _ = sender.output(LoginFormOutput::Submit {
                        username: self.username.clone(),
                        password: self.password.clone(),
                    });
                }
            }
            LoginFormInput::Clear => {
                self.username.clear();
                self.password.clear();
                self.error = None;
            }
            LoginFormInput::TogglePasswordVisibility => {
                self.password_visible = !self.password_visible;
            }
            LoginFormInput::FocusUsername => {
                // Widget focus is handled by the view macro
            }
            LoginFormInput::FocusPassword => {
                // Widget focus is handled by the view macro
            }
            LoginFormInput::SetPrompt(prompt) => {
                self.prompt = prompt;
            }
            LoginFormInput::ShowError(error) => {
                self.error = Some(error);
            }
            LoginFormInput::ClearError => {
                self.error = None;
            }
            LoginFormInput::SetEnabled(enabled) => {
                self.enabled = enabled;
            }
        }
    }
}
