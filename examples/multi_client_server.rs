//! Multi-client server example
//! 
//! This example demonstrates:
//! - A server handling multiple clients concurrently
//! - Broadcasting messages to all connected clients
//! - Client management and connection tracking
//! 
//! Start the server: cargo run --example multi_client_server
//! Then connect multiple clients using the basic_client example

use pipeguard::{NamedPipeServerStruct, Result};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{broadcast, Mutex};
use tokio::signal;

const PIPE_NAME: &str = "multi_client_server";

type ClientId = usize;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Multi-Client Server Example");
    println!("===========================");
    println!("Server listening on pipe: {}", PIPE_NAME);
    println!("Press Ctrl+C to shutdown");
    println!();
    
    // Shared state for tracking clients and broadcasting messages
    let clients: Arc<Mutex<HashMap<ClientId, broadcast::Sender<String>>>> = 
        Arc::new(Mutex::new(HashMap::new()));
    
    let mut server = NamedPipeServerStruct::new(PIPE_NAME);
    
    // Start server with client handler
    let clients_clone = Arc::clone(&clients);
    let server_task = server.start(move |mut connection| {
        let clients = Arc::clone(&clients_clone);
        async move {
            let client_id = connection.id();
            println!("[SERVER] Client {} connected", client_id);
            
            // Create a broadcast channel for this client
            let (tx, mut rx) = broadcast::channel(100);
            
            // Add client to the clients map
            {
                let mut clients_guard = clients.lock().await;
                clients_guard.insert(client_id, tx.clone());
                println!("[SERVER] Total clients: {}", clients_guard.len());
            }
            
            // Handle incoming messages from this client
            let clients_for_handler = Arc::clone(&clients);
            let message_handler = tokio::spawn(async move {
                loop {
                    match connection.receive_string().await {
                        Ok(message) => {
                            println!("[SERVER] Client {}: {}", client_id, message);
                            
                            // Check for special commands
                            if message.starts_with("/broadcast ") {
                                let broadcast_msg = message.strip_prefix("/broadcast ").unwrap();
                                let full_msg = format!("[Broadcast from {}]: {}", client_id, broadcast_msg);
                                
                                // Send to all other clients
                                let clients_guard = clients_for_handler.lock().await;
                                for (&other_id, sender) in clients_guard.iter() {
                                    if other_id != client_id {
                                        let _ = sender.send(full_msg.clone());
                                    }
                                }
                                drop(clients_guard); // Explicitly drop the guard
                                
                                // Acknowledge to sender
                                let _ = connection.send_string("Broadcast sent!").await;
                            } else if message == "/clients" {
                                let clients_guard = clients_for_handler.lock().await;
                                let client_list: Vec<String> = clients_guard.keys()
                                    .map(|&id| format!("Client {}", id))
                                    .collect();
                                let response = format!("Connected clients: {}", client_list.join(", "));
                                drop(clients_guard);
                                let _ = connection.send_string(&response).await;
                            } else if message == "/quit" {
                                let _ = connection.send_string("Goodbye!").await;
                                break;
                            } else {
                                // Echo the message back
                                let _ = connection.send_string(&format!("Echo: {}", message)).await;
                            }
                        }
                        Err(_) => {
                            println!("[SERVER] Client {} disconnected (read error)", client_id);
                            break;
                        }
                    }
                }
                
                // Remove client from the map when disconnecting
                {
                    let mut clients_guard = clients_for_handler.lock().await;
                    clients_guard.remove(&client_id);
                    println!("[SERVER] Client {} removed. Total clients: {}", client_id, clients_guard.len());
                }
            });
            
            // Handle outgoing broadcast messages to this client
            let broadcast_handler = tokio::spawn(async move {
                while let Ok(broadcast_msg) = rx.recv().await {
                    // In a real implementation, you'd send this to the client
                    // For this example, we just print it since we don't have access to connection here
                    println!("[SERVER] Would send to client {}: {}", client_id, broadcast_msg);
                }
            });
            
            // Wait for either handler to complete
            tokio::select! {
                _ = message_handler => {},
                _ = broadcast_handler => {},
            }
            
            println!("[SERVER] Client {} handler finished", client_id);
            Ok(())
        }
    });
    
    server_task.await?;
    
    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            println!("\n[SERVER] Shutdown signal received");
        }
    }
    
    println!("[SERVER] Server shutting down...");
    Ok(())
}
