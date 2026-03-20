//! greetd IPC client
//!
//! Handles communication with the greetd login daemon via Unix socket.

use anyhow::{Context, Result};
use greetd_ipc::{AuthMessageType, ErrorType, Request, Response};
use std::env;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tracing::{debug, error, info};

/// greetd IPC client
pub struct GreetdClient {
    stream: UnixStream,
}

impl GreetdClient {
    /// Connect to the greetd socket
    pub async fn connect() -> Result<Self> {
        let socket_path = env::var("GREETD_SOCK")
            .context("GREETD_SOCK environment variable not set")?;

        let socket_path = PathBuf::from(socket_path);

        debug!("Connecting to greetd at {:?}", socket_path);

        let stream = UnixStream::connect(&socket_path)
            .await
            .with_context(|| format!("Failed to connect to greetd socket: {:?}", socket_path))?;

        info!("Connected to greetd");

        Ok(Self { stream })
    }

    /// Create a new session for a user
    pub async fn create_session(&mut self, username: &str) -> Result<Response> {
        debug!("Creating session for user: {}", username);

        let request = Request::CreateSession {
            username: username.to_string(),
        };

        self.send_request(request).await
    }

    /// Respond to an authentication message
    pub async fn post_auth_response(&mut self, response: Option<&str>) -> Result<Response> {
        debug!("Posting auth response");

        let request = Request::PostAuthMessageResponse {
            response: response.map(|s| s.to_string()),
        };

        self.send_request(request).await
    }

    /// Start the session with the given command
    pub async fn start_session(&mut self, cmd: &[String], env: &[String]) -> Result<Response> {
        debug!("Starting session with command: {:?}", cmd);

        let request = Request::StartSession {
            cmd: cmd.to_vec(),
            env: env.to_vec(),
        };

        self.send_request(request).await
    }

    /// Cancel the current session
    #[allow(dead_code)]
    pub async fn cancel_session(&mut self) -> Result<Response> {
        debug!("Cancelling session");

        let request = Request::CancelSession;

        self.send_request(request).await
    }

    /// Send a request and receive the response
    async fn send_request(&mut self, request: Request) -> Result<Response> {
        // Serialize the request to JSON
        let json = serde_json::to_string(&request)
            .context("Failed to serialize request")?;

        // Write length prefix (u32, native byte order) followed by JSON
        let len = json.len() as u32;
        self.stream.write_all(&len.to_ne_bytes()).await
            .context("Failed to write request length")?;
        self.stream.write_all(json.as_bytes()).await
            .context("Failed to write request")?;
        self.stream.flush().await
            .context("Failed to flush request")?;

        // Read response length
        let mut len_bytes = [0u8; 4];
        self.stream.read_exact(&mut len_bytes).await
            .context("Failed to read response length")?;
        let len = u32::from_ne_bytes(len_bytes) as usize;

        // Read response JSON
        let mut buffer = vec![0u8; len];
        self.stream.read_exact(&mut buffer).await
            .context("Failed to read response")?;

        let json = String::from_utf8(buffer)
            .context("Response is not valid UTF-8")?;

        let response: Response = serde_json::from_str(&json)
            .context("Failed to deserialize response")?;

        Ok(response)
    }
}

/// Our own message type enum (since greetd_ipc's doesn't implement Clone/PartialEq)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageType {
    Visible,
    Secret,
    Info,
    Error,
}

impl From<&AuthMessageType> for MessageType {
    fn from(t: &AuthMessageType) -> Self {
        match t {
            AuthMessageType::Visible => MessageType::Visible,
            AuthMessageType::Secret => MessageType::Secret,
            AuthMessageType::Info => MessageType::Info,
            AuthMessageType::Error => MessageType::Error,
        }
    }
}

/// Authentication state machine
#[derive(Debug, Clone)]
pub enum AuthState {
    /// Waiting for user input
    Idle,
    /// Session created, waiting for auth
    AwaitingAuth {
        #[allow(dead_code)]
        message_type: MessageType,
        #[allow(dead_code)]
        message: String,
    },
    /// Authentication successful, ready to start session
    Authenticated,
    /// Session started
    SessionStarted,
    /// Error occurred
    Error(#[allow(dead_code)] String),
}

/// Result of an authentication attempt
#[derive(Debug)]
pub enum AuthResult {
    /// Need more input (password, etc.)
    NeedInput {
        message_type: MessageType,
        message: String,
    },
    /// Authentication successful
    Success,
    /// Authentication failed
    Failed(String),
}

/// High-level authentication flow helper
pub struct Authenticator {
    client: GreetdClient,
    state: AuthState,
}

impl Authenticator {
    /// Create a new authenticator
    pub async fn new() -> Result<Self> {
        let client = GreetdClient::connect().await?;
        Ok(Self {
            client,
            state: AuthState::Idle,
        })
    }

    /// Start authentication for a user
    pub async fn start(&mut self, username: &str) -> Result<AuthResult> {
        let response = self.client.create_session(username).await?;

        match response {
            Response::AuthMessage {
                auth_message_type,
                auth_message,
            } => {
                let message_type = MessageType::from(&auth_message_type);
                self.state = AuthState::AwaitingAuth {
                    message_type: message_type.clone(),
                    message: auth_message.clone(),
                };
                Ok(AuthResult::NeedInput {
                    message_type,
                    message: auth_message,
                })
            }
            Response::Success => {
                self.state = AuthState::Authenticated;
                Ok(AuthResult::Success)
            }
            Response::Error {
                error_type,
                description,
            } => {
                let msg = format_error(error_type, &description);
                self.state = AuthState::Error(msg.clone());
                Ok(AuthResult::Failed(msg))
            }
        }
    }

    /// Submit an authentication response (e.g., password)
    pub async fn respond(&mut self, response: Option<&str>) -> Result<AuthResult> {
        let greetd_response = self.client.post_auth_response(response).await?;

        match greetd_response {
            Response::AuthMessage {
                auth_message_type,
                auth_message,
            } => {
                let message_type = MessageType::from(&auth_message_type);
                self.state = AuthState::AwaitingAuth {
                    message_type: message_type.clone(),
                    message: auth_message.clone(),
                };
                Ok(AuthResult::NeedInput {
                    message_type,
                    message: auth_message,
                })
            }
            Response::Success => {
                self.state = AuthState::Authenticated;
                Ok(AuthResult::Success)
            }
            Response::Error {
                error_type,
                description,
            } => {
                let msg = format_error(error_type, &description);
                self.state = AuthState::Error(msg.clone());
                Ok(AuthResult::Failed(msg))
            }
        }
    }

    /// Start the user's session
    pub async fn start_session(&mut self, cmd: &[String], env: &[String]) -> Result<()> {
        let response = self.client.start_session(cmd, env).await?;

        match response {
            Response::Success => {
                self.state = AuthState::SessionStarted;
                info!("Session started successfully");
                Ok(())
            }
            Response::Error {
                error_type,
                description,
            } => {
                let msg = format_error(error_type, &description);
                error!("Failed to start session: {}", msg);
                anyhow::bail!(msg)
            }
            _ => {
                anyhow::bail!("Unexpected response when starting session")
            }
        }
    }

    /// Cancel the current authentication
    #[allow(dead_code)]
    pub async fn cancel(&mut self) -> Result<()> {
        let _ = self.client.cancel_session().await;
        self.state = AuthState::Idle;
        Ok(())
    }

    /// Get the current authentication state
    #[allow(dead_code)]
    pub fn state(&self) -> &AuthState {
        &self.state
    }
}

/// Format an error message from greetd
fn format_error(error_type: ErrorType, description: &str) -> String {
    match error_type {
        ErrorType::AuthError => {
            if description.is_empty() {
                "Authentication failed".to_string()
            } else {
                format!("Authentication failed: {}", description)
            }
        }
        ErrorType::Error => {
            if description.is_empty() {
                "An error occurred".to_string()
            } else {
                description.to_string()
            }
        }
    }
}

/// Demo/mock authenticator for testing without greetd
pub struct DemoAuthenticator {
    state: AuthState,
    #[allow(dead_code)]
    username: String,
}

impl DemoAuthenticator {
    pub fn new() -> Self {
        Self {
            state: AuthState::Idle,
            username: String::new(),
        }
    }

    pub async fn start(&mut self, username: &str) -> Result<AuthResult> {
        self.username = username.to_string();
        self.state = AuthState::AwaitingAuth {
            message_type: MessageType::Secret,
            message: "Password:".to_string(),
        };
        Ok(AuthResult::NeedInput {
            message_type: MessageType::Secret,
            message: "Password:".to_string(),
        })
    }

    pub async fn respond(&mut self, response: Option<&str>) -> Result<AuthResult> {
        // In demo mode, accept any password
        if response.is_some() {
            self.state = AuthState::Authenticated;
            Ok(AuthResult::Success)
        } else {
            Ok(AuthResult::Failed("No password provided".to_string()))
        }
    }

    pub async fn start_session(&mut self, cmd: &[String], _env: &[String]) -> Result<()> {
        info!("[DEMO] Would start session with: {:?}", cmd);
        self.state = AuthState::SessionStarted;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn cancel(&mut self) -> Result<()> {
        self.state = AuthState::Idle;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn state(&self) -> &AuthState {
        &self.state
    }
}
