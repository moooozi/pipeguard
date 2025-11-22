//! Basic communication example demonstrating simple string messaging
//! 
//! This example shows:
//! - How to create a server that echoes messages
//! - How to connect a client and send messages
//! - Basic error handling
//! 
//! Run this example with: cargo run --example basic_communication

use pipeguard::{NamedPipeClientStruct, NamedPipeServerStruct, Result};
use std::time::Duration;
use tokio::time::sleep;

const PIPE_NAME: &str = "basic_communication_example";

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting basic communication example...");
    
    // Start server in background
    let server_handle = tokio::spawn(run_server());
    
    // Give server time to start
    sleep(Duration::from_millis(500)).await;
    
    // Run client
    let client_result = run_client().await;
    
    // Wait for server (it will run indefinitely, but we'll stop it after client finishes)
    server_handle.abort();
    
    client_result
}

async fn run_server() -> Result<()> {
    println!("[SERVER] Starting echo server on pipe '{}'", PIPE_NAME);
    
    let mut server = NamedPipeServerStruct::new(PIPE_NAME);
    
    server.start(|mut connection| async move {
        println!("[SERVER] Client connected (ID: {})", connection.id());
        
        loop {
            match connection.receive_string().await {
                Ok(message) => {
                    println!("[SERVER] Received: '{}'", message);
                    
                    let response = format!("Echo: {}", message);
                    if let Err(e) = connection.send_string(&response).await {
                        println!("[SERVER] Failed to send response: {}", e);
                        break;
                    }
                    
                    // Exit condition
                    if message.trim().to_lowercase() == "quit" {
                        println!("[SERVER] Client requested quit");
                        break;
                    }
                }
                Err(e) => {
                    println!("[SERVER] Error receiving message: {}", e);
                    break;
                }
            }
        }
        
        println!("[SERVER] Client disconnected (ID: {})", connection.id());
        Ok(())
    }).await?;
    
    Ok(())
}

async fn run_client() -> Result<()> {
    println!("[CLIENT] Connecting to pipe '{}'", PIPE_NAME);
    
    let mut client = NamedPipeClientStruct::new(PIPE_NAME);
    
    // Retry connection a few times in case server isn't ready yet
    let mut connected = false;
    for attempt in 1..=5 {
        match client.connect().await {
            Ok(_) => {
                connected = true;
                break;
            }
            Err(e) => {
                println!("[CLIENT] Connection attempt {} failed: {}", attempt, e);
                if attempt < 5 {
                    sleep(Duration::from_millis(200)).await;
                }
            }
        }
    }
    
    if !connected {
        return Err(pipeguard::NamedPipeError::NotConnected);
    }
    
    println!("[CLIENT] Connected successfully!");
    
    // Send some test messages
    let messages = vec![
        "Hello, Server!",
        "How are you?",
        "This is a test message",
        "quit",
    ];
    
    for message in messages {
        println!("[CLIENT] Sending: '{}'", message);
        client.send_string(message).await?;
        
        let response = client.receive_string().await?;
        println!("[CLIENT] Received: '{}'", response);
        
        sleep(Duration::from_millis(100)).await;
    }
    
    println!("[CLIENT] Communication completed");
    Ok(())
}
