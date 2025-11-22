use pipeguard::{NamedPipeClientStruct, NamedPipeServerStruct};
use tokio::time::{sleep, Duration};

// Custom key for demonstration
const CUSTOM_KEY: [u8; 32] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
    27, 28, 29, 30, 31, 32,
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Encryption Options Demo ===");
    println!("This demo shows different ways to use encryption in the named pipe library");
    println!();

    // Demo 1: Default encryption (uses compile-time generated key)
    println!("1. Testing DEFAULT encryption (compile-time generated key)");
    test_encryption_mode("default_pipe", None, None).await?;

    // Demo 2: Custom key encryption
    println!("\n2. Testing CUSTOM KEY encryption");
    test_encryption_mode("custom_pipe", Some(CUSTOM_KEY), Some(&CUSTOM_KEY)).await?;

    println!("\n=== All Encryption Modes Working! ===");
    Ok(())
}

async fn test_encryption_mode(
    pipe_name: &str,
    server_key: Option<[u8; 32]>,
    client_key: Option<&[u8; 32]>,
) -> Result<(), Box<dyn std::error::Error>> {
    let key_description = match server_key {
        Some(_) => "custom key",
        None => "default key",
    };

    // Start server
    let server_pipe_name = pipe_name.to_string();
    let _server_handle = tokio::spawn(async move {
        println!(
            "   [SERVER] Starting encrypted server with {}...",
            key_description
        );
        let mut server = NamedPipeServerStruct::new_encrypted(&server_pipe_name, server_key);

        server
            .start(|mut connection| async move {
                println!("   [SERVER] Client connected with ID: {}", connection.id());

                while let Ok(data) = connection.receive_bytes().await {
                    let message = String::from_utf8_lossy(&data);
                    println!("   [SERVER] Received: {}", message);

                    let response = format!("Echo: {}", message);
                    if let Err(e) = connection.send_bytes(response.as_bytes()).await {
                        println!("   [SERVER] Failed to send response: {}", e);
                        break;
                    }
                }

                println!("   [SERVER] Client disconnected");
                Ok(())
            })
            .await
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Create and test client
    println!("   [CLIENT] Connecting with {}...", key_description);
    let mut client = NamedPipeClientStruct::new_encrypted(pipe_name, client_key);
    client.connect().await?;
    println!("   [CLIENT] Connected successfully!");

    // Send test messages
    let messages = ["Hello!", "This is encrypted!", "Testing complete!"];

    for message in &messages {
        println!("   [CLIENT] Sending: {}", message);
        client.send_bytes(message.as_bytes()).await?;

        let response_data = client.receive_bytes().await?;
        let response = String::from_utf8_lossy(&response_data);
        println!("   [CLIENT] Received: {}", response);

        sleep(Duration::from_millis(100)).await;
    }

    client.disconnect();
    println!("   [CLIENT] Test completed for {}", key_description);

    // Give server time to cleanup
    sleep(Duration::from_millis(200)).await;

    Ok(())
}
