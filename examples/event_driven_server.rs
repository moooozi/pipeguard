//! Event-driven Named Pipe Server Example
//!
//! This example demonstrates:
//! - Event-driven architecture with callbacks
//! - Proper shutdown handling with shared state
//! - Connection lifecycle events
//! - Message processing events
//! - Graceful cleanup and resource management
//!
//! Run with: cargo run --example event_driven_server

use pipeguard::{NamedPipeConnection, NamedPipeServerStruct, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

const PIPE_NAME: &str = "event_driven_server";

// Event types for the server
#[derive(Debug, Clone)]
pub enum ServerEvent {
    ClientConnected {
        client_id: usize,
        timestamp: u64,
    },
    ClientDisconnected {
        client_id: usize,
        timestamp: u64,
    },
    MessageReceived {
        client_id: usize,
        message: String,
        timestamp: u64,
    },
    ServerStarted {
        pipe_name: String,
        timestamp: u64,
    },
    ServerShutdown {
        timestamp: u64,
    },
    Error {
        client_id: Option<usize>,
        error: String,
        timestamp: u64,
    },
}

#[derive(Serialize, Deserialize, Debug)]
struct ClientMessage {
    command: String,
    data: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ServerResponse {
    status: String,
    result: Option<String>,
    timestamp: u64,
}

// Event handler trait
pub trait EventHandler: Send + Sync {
    fn handle_event(&self, event: ServerEvent);
}

// Server state to track connections and handle shutdown
#[derive(Clone)]
pub struct ServerState {
    pub is_running: Arc<AtomicBool>,
    pub clients: Arc<Mutex<HashMap<usize, ClientInfo>>>,
    pub event_handler: Arc<dyn EventHandler>,
}

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: usize,
    pub connected_at: u64,
    pub message_count: u32,
}

impl ServerState {
    pub fn new(event_handler: Arc<dyn EventHandler>) -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
            clients: Arc::new(Mutex::new(HashMap::new())),
            event_handler,
        }
    }

    pub fn shutdown(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        self.event_handler
            .handle_event(ServerEvent::ServerShutdown {
                timestamp: get_timestamp(),
            });
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }
}

// Console event handler implementation
pub struct ConsoleEventHandler;

impl EventHandler for ConsoleEventHandler {
    fn handle_event(&self, event: ServerEvent) {
        match event {
            ServerEvent::ClientConnected {
                client_id,
                timestamp,
            } => {
                println!(
                    "[EVENT] Client {} connected at {}",
                    client_id,
                    format_timestamp(timestamp)
                );
            }
            ServerEvent::ClientDisconnected {
                client_id,
                timestamp,
            } => {
                println!(
                    "[EVENT] Client {} disconnected at {}",
                    client_id,
                    format_timestamp(timestamp)
                );
            }
            ServerEvent::MessageReceived {
                client_id,
                message,
                timestamp,
            } => {
                println!(
                    "[EVENT] Message from client {}: '{}' at {}",
                    client_id,
                    message,
                    format_timestamp(timestamp)
                );
            }
            ServerEvent::ServerStarted {
                pipe_name,
                timestamp,
            } => {
                println!(
                    "[EVENT] Server started on pipe '{}' at {}",
                    pipe_name,
                    format_timestamp(timestamp)
                );
            }
            ServerEvent::ServerShutdown { timestamp } => {
                println!(
                    "[EVENT] Server shutdown initiated at {}",
                    format_timestamp(timestamp)
                );
            }
            ServerEvent::Error {
                client_id,
                error,
                timestamp,
            } => {
                if let Some(id) = client_id {
                    println!(
                        "[EVENT] Error from client {}: {} at {}",
                        id,
                        error,
                        format_timestamp(timestamp)
                    );
                } else {
                    println!(
                        "[EVENT] Server error: {} at {}",
                        error,
                        format_timestamp(timestamp)
                    );
                }
            }
        }
    }
}

// Event-driven server
pub struct EventDrivenServer {
    state: ServerState,
    server: Option<NamedPipeServerStruct>,
}

impl EventDrivenServer {
    pub fn new(event_handler: Arc<dyn EventHandler>) -> Self {
        Self {
            state: ServerState::new(event_handler),
            server: None,
        }
    }

    pub async fn start(&mut self, pipe_name: &str) -> Result<()> {
        self.state.is_running.store(true, Ordering::SeqCst);
        self.state
            .event_handler
            .handle_event(ServerEvent::ServerStarted {
                pipe_name: pipe_name.to_string(),
                timestamp: get_timestamp(),
            });

        let mut server = NamedPipeServerStruct::new(pipe_name);
        let state = self.state.clone();

        server
            .start(move |connection| {
                let state = state.clone();
                async move { handle_client_connection(connection, state).await }
            })
            .await?;

        self.server = Some(server);
        Ok(())
    }

    pub fn get_state(&self) -> &ServerState {
        &self.state
    }

    pub async fn get_client_count(&self) -> usize {
        self.state.clients.lock().await.len()
    }

    pub async fn get_client_info(&self, client_id: usize) -> Option<ClientInfo> {
        self.state.clients.lock().await.get(&client_id).cloned()
    }
}

async fn handle_client_connection(
    mut connection: NamedPipeConnection,
    state: ServerState,
) -> Result<()> {
    let client_id = connection.id();
    let connected_at = get_timestamp();

    // Add client to state
    {
        let mut clients = state.clients.lock().await;
        clients.insert(
            client_id,
            ClientInfo {
                id: client_id,
                connected_at,
                message_count: 0,
            },
        );
    }

    // Fire connection event
    state
        .event_handler
        .handle_event(ServerEvent::ClientConnected {
            client_id,
            timestamp: connected_at,
        });

    // Handle messages while server is running and client is connected
    while state.is_running() {
        match connection.receive_string().await {
            Ok(raw_message) => {
                // Update message count
                {
                    let mut clients = state.clients.lock().await;
                    if let Some(client_info) = clients.get_mut(&client_id) {
                        client_info.message_count += 1;
                    }
                }

                // Fire message received event
                state
                    .event_handler
                    .handle_event(ServerEvent::MessageReceived {
                        client_id,
                        message: raw_message.clone(),
                        timestamp: get_timestamp(),
                    });

                // Process the message
                let response = match process_message(&raw_message, client_id, &state).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        state.event_handler.handle_event(ServerEvent::Error {
                            client_id: Some(client_id),
                            error: format!("Message processing error: {}", e),
                            timestamp: get_timestamp(),
                        });

                        ServerResponse {
                            status: "error".to_string(),
                            result: Some(format!("Processing error: {}", e)),
                            timestamp: get_timestamp(),
                        }
                    }
                };

                // Send response
                if let Err(e) = connection.send_json(&response).await {
                    state.event_handler.handle_event(ServerEvent::Error {
                        client_id: Some(client_id),
                        error: format!("Failed to send response: {}", e),
                        timestamp: get_timestamp(),
                    });
                    break;
                }

                // Check for quit command
                if raw_message.trim() == "quit" {
                    break;
                }
            }
            Err(e) => {
                state.event_handler.handle_event(ServerEvent::Error {
                    client_id: Some(client_id),
                    error: format!("Failed to receive message: {}", e),
                    timestamp: get_timestamp(),
                });
                break;
            }
        }
    }

    // Remove client from state
    {
        let mut clients = state.clients.lock().await;
        clients.remove(&client_id);
    }

    // Fire disconnection event
    state
        .event_handler
        .handle_event(ServerEvent::ClientDisconnected {
            client_id,
            timestamp: get_timestamp(),
        });

    Ok(())
}

async fn process_message(
    raw_message: &str,
    client_id: usize,
    state: &ServerState,
) -> Result<ServerResponse> {
    // Try to parse as JSON first, fall back to plain string
    let message = match serde_json::from_str::<ClientMessage>(raw_message) {
        Ok(msg) => msg,
        Err(_) => ClientMessage {
            command: "echo".to_string(),
            data: Some(raw_message.to_string()),
        },
    };

    let response = match message.command.as_str() {
        "ping" => ServerResponse {
            status: "success".to_string(),
            result: Some("pong".to_string()),
            timestamp: get_timestamp(),
        },
        "echo" => ServerResponse {
            status: "success".to_string(),
            result: Some(format!("Echo: {}", message.data.unwrap_or_default())),
            timestamp: get_timestamp(),
        },
        "status" => {
            let client_count = state.clients.lock().await.len();
            ServerResponse {
                status: "success".to_string(),
                result: Some(format!("Server running with {} clients", client_count)),
                timestamp: get_timestamp(),
            }
        }
        "client_info" => {
            let clients = state.clients.lock().await;
            if let Some(client_info) = clients.get(&client_id) {
                ServerResponse {
                    status: "success".to_string(),
                    result: Some(format!(
                        "Client {}: connected at {}, {} messages sent",
                        client_info.id,
                        format_timestamp(client_info.connected_at),
                        client_info.message_count
                    )),
                    timestamp: get_timestamp(),
                }
            } else {
                ServerResponse {
                    status: "error".to_string(),
                    result: Some("Client info not found".to_string()),
                    timestamp: get_timestamp(),
                }
            }
        }
        "quit" => ServerResponse {
            status: "goodbye".to_string(),
            result: Some("Connection closing".to_string()),
            timestamp: get_timestamp(),
        },
        _ => ServerResponse {
            status: "error".to_string(),
            result: Some(format!("Unknown command: {}", message.command)),
            timestamp: get_timestamp(),
        },
    };

    Ok(response)
}

fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn format_timestamp(timestamp: u64) -> String {
    // Simple timestamp formatting (you could use chrono for better formatting)
    format!("{}", timestamp)
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Event-Driven Named Pipe Server Example");
    println!("======================================");
    println!("Server will listen on pipe: {}", PIPE_NAME);
    println!("Send 'quit' to disconnect a client");
    println!("Press Ctrl+C or send SIGTERM to shutdown server");
    println!();

    // Create event handler
    let event_handler = Arc::new(ConsoleEventHandler);

    // Create and start server
    let mut server = EventDrivenServer::new(event_handler);
    let server_state = server.get_state().clone();

    // Start server in background
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start(PIPE_NAME).await {
            eprintln!("Server error: {}", e);
        }
    });

    // Simulate some server monitoring and shutdown
    let monitoring_state = server_state.clone();
    tokio::spawn(async move {
        let mut last_client_count = 0;

        loop {
            if !monitoring_state.is_running() {
                break;
            }

            let client_count = monitoring_state.clients.lock().await.len();
            if client_count != last_client_count {
                println!(
                    "[MONITOR] Client count changed: {} -> {}",
                    last_client_count, client_count
                );
                last_client_count = client_count;
            }

            sleep(Duration::from_secs(2)).await;
        }
    });

    // Wait for Ctrl+C to shutdown
    tokio::signal::ctrl_c().await.unwrap();
    println!("\n[MAIN] Shutdown signal received");

    // Shutdown the server using the state we captured earlier
    println!("[MAIN] Shutting down server...");
    server_state.shutdown();

    // Wait a bit for cleanup
    sleep(Duration::from_millis(500)).await;

    // Show final statistics
    let final_client_count = server_state.clients.lock().await.len();
    println!(
        "[MAIN] Server shutdown complete. Final client count: {}",
        final_client_count
    );
    // Shutdown the server after 5 seconds
    tokio::spawn(async move {
        sleep(Duration::from_secs(5)).await;
        server_state.shutdown();
    });
    // Cancel server task
    server_handle.abort();

    Ok(())
}
