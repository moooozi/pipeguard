//! Event-driven server test client
//!
//! This client demonstrates various commands supported by the event-driven server:
//! - ping/pong
//! - echo messages  
//! - status queries
//! - client info
//! - JSON and plain text messages
//!
//! Run with: cargo run --example event_client

use pipeguard::{NamedPipeClientStruct, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

const PIPE_NAME: &str = "event_driven_server";

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

#[tokio::main]
async fn main() -> Result<()> {
    println!("Event-Driven Server Test Client");
    println!("===============================");
    println!("Connecting to pipe: {}", PIPE_NAME);
    println!();

    let mut client = NamedPipeClientStruct::new(PIPE_NAME);

    // Connect with retry
    print!("Connecting... ");
    let mut connected = false;
    for attempt in 1..=5 {
        match client.connect().await {
            Ok(_) => {
                connected = true;
                println!("Connected!");
                break;
            }
            Err(_) if attempt < 5 => {
                println!("failed (attempt {}), retrying...", attempt);
                sleep(Duration::from_millis(500)).await;
            }
            Err(e) => {
                println!("failed: {}", e);
                return Err(e);
            }
        }
    }

    if !connected {
        println!("Could not connect to server");
        return Ok(());
    }

    println!();
    println!("Testing various commands...");
    println!();

    // Test 1: Ping command (JSON)
    println!("1. Testing ping command (JSON)");
    let ping_msg = ClientMessage {
        command: "ping".to_string(),
        data: None,
    };

    client.send_json(&ping_msg).await?;
    let response: ServerResponse = client.receive_json().await?;
    println!("   Response: {:?}", response);
    sleep(Duration::from_millis(500)).await;

    // Test 2: Echo command (JSON)
    println!("\n2. Testing echo command (JSON)");
    let echo_msg = ClientMessage {
        command: "echo".to_string(),
        data: Some("Hello from JSON client!".to_string()),
    };

    client.send_json(&echo_msg).await?;
    let response: ServerResponse = client.receive_json().await?;
    println!("   Response: {:?}", response);
    sleep(Duration::from_millis(500)).await;

    // Test 3: Plain text message (will be treated as echo)
    println!("\n3. Testing plain text message");
    client.send_string("This is a plain text message").await?;
    let response: ServerResponse = client.receive_json().await?;
    println!("   Response: {:?}", response);
    sleep(Duration::from_millis(500)).await;

    // Test 4: Status command
    println!("\n4. Testing server status command");
    let status_msg = ClientMessage {
        command: "status".to_string(),
        data: None,
    };

    client.send_json(&status_msg).await?;
    let response: ServerResponse = client.receive_json().await?;
    println!("   Response: {:?}", response);
    sleep(Duration::from_millis(500)).await;

    // Test 5: Client info command
    println!("\n5. Testing client info command");
    let info_msg = ClientMessage {
        command: "client_info".to_string(),
        data: None,
    };

    client.send_json(&info_msg).await?;
    let response: ServerResponse = client.receive_json().await?;
    println!("   Response: {:?}", response);
    sleep(Duration::from_millis(500)).await;

    // Test 6: Unknown command
    println!("\n6. Testing unknown command");
    let unknown_msg = ClientMessage {
        command: "unknown_command".to_string(),
        data: Some("test data".to_string()),
    };

    client.send_json(&unknown_msg).await?;
    let response: ServerResponse = client.receive_json().await?;
    println!("   Response: {:?}", response);
    sleep(Duration::from_millis(500)).await;

    // Test 7: Multiple rapid messages
    println!("\n7. Testing multiple rapid messages");
    for i in 1..=3 {
        let msg = ClientMessage {
            command: "echo".to_string(),
            data: Some(format!("Rapid message #{}", i)),
        };

        client.send_json(&msg).await?;
        let response: ServerResponse = client.receive_json().await?;
        println!("   Message {}: {:?}", i, response.result);
        sleep(Duration::from_millis(100)).await;
    }

    // Test 8: Final quit
    println!("\n8. Sending quit command");
    let quit_msg = ClientMessage {
        command: "quit".to_string(),
        data: None,
    };

    client.send_json(&quit_msg).await?;
    let response: ServerResponse = client.receive_json().await?;
    println!("   Response: {:?}", response);

    println!("\n✓ All tests completed successfully!");
    println!("Client disconnecting...");

    Ok(())
}
