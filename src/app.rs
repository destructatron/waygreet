//! Main application component
//!
//! The root Relm4 component that manages the greeter UI.

use anyhow::Result;
use crate::greetd::MessageType;
use crate::accessibility;
use gtk4::prelude::*;
use gtk4::glib;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;
use std::path::Path;
use tracing::{debug, error, info, warn};

use crate::components::{
    login_form::{LoginForm, LoginFormInput, LoginFormOutput},
    power_menu::{PowerMenu, PowerMenuOutput},
    session_selector::{SessionSelector, SessionSelectorInput, SessionSelectorOutput},
};
use crate::config::{Config, State};
use crate::greetd::{AuthResult, Authenticator, DemoAuthenticator};
use crate::sessions::{discover_sessions, Session};

/// Application input messages
#[derive(Debug)]
pub enum AppInput {
    /// Login form submitted credentials
    LoginSubmit { username: String, password: String },
    /// Session was selected
    SessionSelected(Session),
    /// Power menu action
    PowerAction(PowerMenuOutput),
    /// greetd response received
    GreetdResponse(AuthResult),
    /// greetd error
    GreetdError(String),
    /// Clear error and reset
    #[allow(dead_code)]
    Reset,
}

/// Authentication mode (real or demo)
enum AuthMode {
    Real(Authenticator),
    Demo(DemoAuthenticator),
}

/// Main application state
pub struct App {
    config: Config,
    demo_mode: bool,
    auth: Option<AuthMode>,
    selected_session: Option<Session>,
    #[allow(dead_code)]
    sessions: Vec<Session>,
    authenticating: bool,
    current_username: String,

    // Child components
    login_form: Controller<LoginForm>,
    session_selector: Controller<SessionSelector>,
    power_menu: Controller<PowerMenu>,
}

#[relm4::component(pub async)]
impl AsyncComponent for App {
    type Init = (Config, bool, bool); // (config, demo_mode, no_accessibility)
    type Input = AppInput;
    type Output = ();
    type CommandOutput = AuthResult;

    view! {
        #[name = "window"]
        gtk4::ApplicationWindow {
            set_title: Some("Waygreet"),
            set_default_size: (800, 600),

            gtk4::Box {
                set_orientation: gtk4::Orientation::Vertical,
                set_vexpand: true,
                set_hexpand: true,

                // Main content area
                gtk4::Box {
                    set_orientation: gtk4::Orientation::Vertical,
                    set_vexpand: true,
                    set_hexpand: true,
                    set_valign: gtk4::Align::Center,
                    set_halign: gtk4::Align::Center,
                    set_spacing: 24,
                    set_margin_all: 32,

                    // Greeter title
                    gtk4::Label {
                        set_label: "Welcome",
                        set_css_classes: &["title-1"],
                    },

                    // Login form
                    model.login_form.widget() {},

                    // Session selector
                    model.session_selector.widget() {},
                },

                // Power menu (bottom right)
                gtk4::Box {
                    set_hexpand: true,
                    set_halign: gtk4::Align::End,
                    set_valign: gtk4::Align::End,

                    model.power_menu.widget() {},
                },
            },
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let (config, demo_mode, no_accessibility) = init;

        // Discover available sessions
        let sessions = discover_sessions(&config.sessions);
        info!("Discovered {} sessions", sessions.len());

        // Load previous state
        let state = State::load(Path::new(&config.behavior.state_file)).unwrap_or_default();

        // Initialize authentication
        let auth = if demo_mode {
            info!("Running in demo mode");
            Some(AuthMode::Demo(DemoAuthenticator::new()))
        } else {
            match Authenticator::new().await {
                Ok(auth) => Some(AuthMode::Real(auth)),
                Err(e) => {
                    error!("Failed to connect to greetd: {}", e);
                    // Fall back to demo mode
                    warn!("Falling back to demo mode");
                    Some(AuthMode::Demo(DemoAuthenticator::new()))
                }
            }
        };

        // Create child components
        let login_form = LoginForm::builder()
            .launch(state.last_user.clone())
            .forward(sender.input_sender(), |msg| match msg {
                LoginFormOutput::Submit { username, password } => {
                    AppInput::LoginSubmit { username, password }
                }
            });

        let session_selector = SessionSelector::builder()
            .launch(sessions.clone())
            .forward(sender.input_sender(), |msg| match msg {
                SessionSelectorOutput::SessionSelected(session) => {
                    AppInput::SessionSelected(session)
                }
            });

        let power_menu = PowerMenu::builder()
            .launch(config.commands.clone())
            .forward(sender.input_sender(), AppInput::PowerAction);

        // Set initial session selection based on saved state
        if let Some(ref last_session) = state.last_session {
            if let Some(idx) = sessions.iter().position(|s| &s.name == last_session) {
                session_selector.emit(SessionSelectorInput::SelectByIndex(idx));
            }
        }

        // Clone accessibility config before moving config into model
        let a11y_config = config.accessibility.clone();
        let start_orca = !no_accessibility && a11y_config.start_orca;

        let model = App {
            config,
            demo_mode,
            auth,
            selected_session: sessions.first().cloned(),
            sessions,
            authenticating: false,
            current_username: state.last_user.unwrap_or_default(),
            login_form,
            session_selector,
            power_menu,
        };

        let widgets = view_output!();

        // Set up layer shell for fullscreen
        setup_layer_shell(&widgets.window);

        // Start Orca AFTER the window is shown on screen
        // Orca needs AT-SPI bus which GTK creates when the window is realized
        if start_orca {
            // Use an idle callback to start Orca after the main loop begins
            // This ensures the window is fully displayed and AT-SPI is ready
            glib::idle_add_local_once(move || {
                info!("Starting Orca screen reader (idle callback)");
                let config = a11y_config.clone();
                glib::spawn_future_local(async move {
                    // Delay to ensure AT-SPI bus is fully ready
                    glib::timeout_future(std::time::Duration::from_millis(1000)).await;
                    match accessibility::orca::start_orca(&config).await {
                        Ok(()) => info!("Orca screen reader started successfully"),
                        Err(e) => warn!("Failed to start Orca: {}", e),
                    }
                });
            });
        }

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        message: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AppInput::LoginSubmit { username, password } => {
                self.current_username = username.clone();
                self.authenticating = true;
                self.login_form.emit(LoginFormInput::SetEnabled(false));
                self.login_form.emit(LoginFormInput::ClearError);

                // Start authentication
                let result = self.authenticate(&username, &password).await;

                match result {
                    Ok(auth_result) => {
                        sender.input(AppInput::GreetdResponse(auth_result));
                    }
                    Err(e) => {
                        sender.input(AppInput::GreetdError(e.to_string()));
                    }
                }
            }

            AppInput::SessionSelected(session) => {
                debug!("Session selected: {}", session.name);
                self.selected_session = Some(session);
            }

            AppInput::PowerAction(action) => {
                match action {
                    PowerMenuOutput::ActionCompleted => {
                        info!("Power action completed");
                    }
                    PowerMenuOutput::ActionFailed(msg) => {
                        error!("Power action failed: {}", msg);
                        self.login_form.emit(LoginFormInput::ShowError(msg));
                    }
                }
            }

            AppInput::GreetdResponse(result) => {
                match result {
                    AuthResult::NeedInput { message_type, message } => {
                        // Handle additional auth prompts (e.g., multi-factor)
                        debug!("Auth needs more input: {:?} - {}", message_type, message);
                        self.login_form.emit(LoginFormInput::SetPrompt(message));
                        self.login_form.emit(LoginFormInput::SetEnabled(true));
                        self.login_form.emit(LoginFormInput::FocusPassword);
                    }

                    AuthResult::Success => {
                        info!("Authentication successful, starting session");

                        // Save state
                        if let Err(e) = self.save_state() {
                            warn!("Failed to save state: {}", e);
                        }

                        // Start the session
                        if let Err(e) = self.start_session().await {
                            error!("Failed to start session: {}", e);
                            self.login_form.emit(LoginFormInput::ShowError(e.to_string()));
                            self.login_form.emit(LoginFormInput::SetEnabled(true));
                        }
                    }

                    AuthResult::Failed(msg) => {
                        warn!("Authentication failed: {}", msg);
                        self.login_form.emit(LoginFormInput::ShowError(msg));
                        self.login_form.emit(LoginFormInput::SetEnabled(true));
                        self.authenticating = false;
                    }
                }
            }

            AppInput::GreetdError(msg) => {
                error!("greetd error: {}", msg);
                self.login_form.emit(LoginFormInput::ShowError(msg));
                self.login_form.emit(LoginFormInput::SetEnabled(true));
                self.authenticating = false;
            }

            AppInput::Reset => {
                self.login_form.emit(LoginFormInput::Clear);
                self.login_form.emit(LoginFormInput::SetEnabled(true));
                self.login_form.emit(LoginFormInput::SetPrompt("Log In".to_string()));
                self.authenticating = false;

                // Reconnect to greetd if needed
                if !self.demo_mode {
                    match Authenticator::new().await {
                        Ok(auth) => {
                            self.auth = Some(AuthMode::Real(auth));
                        }
                        Err(e) => {
                            error!("Failed to reconnect to greetd: {}", e);
                        }
                    }
                }
            }
        }
    }
}

impl App {
    /// Perform authentication
    async fn authenticate(&mut self, username: &str, password: &str) -> Result<AuthResult> {
        let auth = self.auth.as_mut().ok_or_else(|| {
            anyhow::anyhow!("No authenticator available")
        })?;

        match auth {
            AuthMode::Real(authenticator) => {
                // Start session for user
                let result = authenticator.start(username).await?;

                match result {
                    AuthResult::NeedInput { message_type, .. } => {
                        // Respond with password
                        if message_type == MessageType::Secret {
                            authenticator.respond(Some(password)).await
                        } else {
                            // Visible prompt - send password anyway
                            authenticator.respond(Some(password)).await
                        }
                    }
                    other => Ok(other),
                }
            }
            AuthMode::Demo(authenticator) => {
                let result = authenticator.start(username).await?;

                match result {
                    AuthResult::NeedInput { .. } => {
                        authenticator.respond(Some(password)).await
                    }
                    other => Ok(other),
                }
            }
        }
    }

    /// Start the selected session
    async fn start_session(&mut self) -> Result<()> {
        let session = self.selected_session.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No session selected"))?;

        let cmd = session.get_command(&self.config.sessions);
        let mut env = session.get_env();

        // Add configured environment variables
        for (key, value) in &self.config.environment.vars {
            env.push(format!("{}={}", key, value));
        }

        info!("Starting session: {:?}", cmd);

        let auth = self.auth.as_mut().ok_or_else(|| {
            anyhow::anyhow!("No authenticator available")
        })?;

        match auth {
            AuthMode::Real(authenticator) => {
                authenticator.start_session(&cmd, &env).await?;
            }
            AuthMode::Demo(authenticator) => {
                authenticator.start_session(&cmd, &env).await?;
            }
        }

        Ok(())
    }

    /// Save current state for next login
    fn save_state(&self) -> Result<()> {
        let state = State {
            last_user: Some(self.current_username.clone()),
            last_session: self.selected_session.as_ref().map(|s| s.name.clone()),
        };

        state.save(Path::new(&self.config.behavior.state_file))
    }
}

/// Set up GTK4 layer shell for fullscreen display
fn setup_layer_shell(window: &gtk4::ApplicationWindow) {
    // Initialize layer shell
    window.init_layer_shell();

    // Set layer (overlay for highest priority)
    window.set_layer(Layer::Overlay);

    // Anchor to all edges for fullscreen
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);

    // Request exclusive keyboard input
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);

    info!("Layer shell configured");
}
