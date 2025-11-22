//! Path enforcement example demonstrating same-binary verification
//!
//! This example shows:
//! - How to enable path enforcement on client and server
//! - Successful communication when executable paths match
//! - Connection failure when paths don't match
//!
//! Path enforcement ensures that only processes running the same executable
//! can communicate, providing an additional security layer.
//!
//! Run this example with: cargo run --example path_enforcement

use pipeguard::{NamedPipeClientStruct, NamedPipeServerStruct, Result};
use std::env;
use std::fs;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

const PIPE_NAME: &str = "path_enforcement_example";

#[tokio::main]
async fn main() -> Result<()> {
    // Check if we're running as a subprocess
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "test_same_path" => return run_subprocess_test(true).await,
            "test_different_path" => return run_subprocess_test(false).await,
            _ => {}
        }
    }

    println!("=== Path Enforcement Example ===");
    println!("This example demonstrates same-binary path verification");
    println!();

    // Test 1: Successful communication with same path subprocess
    println!("1. Testing SUCCESSFUL communication with same path subprocess");
    test_successful_communication().await?;

    // Test 2: Failed connection attempt from different path subprocess
    println!("\n2. Testing FAILED connection from different path subprocess");
    test_failed_connection().await?;

    println!("\n=== Path Enforcement Demo Complete ===");
    Ok(())
}

async fn test_successful_communication() -> Result<()> {
    println!("   Starting server and spawning same-path subprocess...");

    // Start server with path enforcement enabled
    let server_handle = tokio::spawn(run_enforced_server());

    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    // Spawn a subprocess that runs the same executable with same path
    let current_exe = env::current_exe().unwrap();
    let subprocess_result = Command::new(&current_exe)
        .arg("test_same_path")
        .status();

    // Clean up server
    server_handle.abort();

    match subprocess_result {
        Ok(status) if status.success() => {
            println!("   SUCCESS: Same-path subprocess connected successfully!");
            Ok(())
        }
        Ok(status) => {
            println!("   ERROR: Same-path subprocess failed with code: {}", status.code().unwrap_or(-1));
            Err(pipeguard::NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Same-path test failed unexpectedly",
            )))
        }
        Err(e) => {
            println!("   ERROR: Failed to spawn same-path subprocess: {}", e);
            Err(pipeguard::NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Subprocess spawn failed: {}", e),
            )))
        }
    }
}

async fn test_failed_connection() -> Result<()> {
    println!("   Starting server and spawning different-path subprocess...");

    // Start server with path enforcement enabled
    let server_handle = tokio::spawn(run_enforced_server());

    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    // Copy executable to temp directory
    let current_exe = env::current_exe().unwrap();
    let temp_dir = env::temp_dir();
    let copied_exe = temp_dir.join("path_enforcement_test.exe");

    // Copy the executable
    if let Err(e) = fs::copy(&current_exe, &copied_exe) {
        println!("   ERROR: Failed to copy executable: {}", e);
        server_handle.abort();
        return Err(pipeguard::NamedPipeError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Copy failed: {}", e),
        )));
    }

    // Spawn subprocess with different path
    let subprocess_result = Command::new(&copied_exe)
        .arg("test_different_path")
        .status();

    // Clean up copied file
    let _ = fs::remove_file(&copied_exe);

    // Clean up server
    server_handle.abort();

    match subprocess_result {
        Ok(status) if status.success() => {
            println!("   SUCCESS: Different-path subprocess correctly failed to connect!");
            Ok(())
        }
        Ok(status) => {
            println!("   ERROR: Different-path subprocess should have returned success (0) but got code: {}", status.code().unwrap_or(-1));
            Err(pipeguard::NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Different-path test subprocess returned wrong exit code",
            )))
        }
        Err(e) => {
            println!("   ERROR: Failed to spawn different-path subprocess: {}", e);
            Err(pipeguard::NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Subprocess spawn failed: {}", e),
            )))
        }
    }
}

async fn run_enforced_server() -> Result<()> {
    println!("   [SERVER] Starting server with path enforcement enabled");

    let mut server = NamedPipeServerStruct::new(PIPE_NAME);

    // Enable path enforcement - only allow connections from same executable
    server.enforce_same_path_client(true);

    server.start(|mut connection| async move {
        println!("   [SERVER] Client connected (ID: {}) - path verified!", connection.id());

        // Wait for a message
        match connection.receive_string().await {
            Ok(message) => {
                println!("   [SERVER] Received: '{}'", message);

                let response = format!("Path-verified echo: {}", message);
                if let Err(e) = connection.send_string(&response).await {
                    println!("   [SERVER] Failed to send response: {}", e);
                }
            }
            Err(e) => {
                println!("   [SERVER] Error receiving message: {}", e);
            }
        }

        println!("   [SERVER] Client disconnected (ID: {})", connection.id());
        Ok(())
    }).await?;

    Ok(())
}

async fn run_enforced_client() -> Result<()> {
    println!("   [CLIENT] Connecting with path enforcement enabled");

    let mut client = NamedPipeClientStruct::new(PIPE_NAME);

    // Enable path enforcement - verify server has same executable path
    client.enforce_same_path_server(true);

    // Attempt connection
    client.connect().await?;

    println!("   [CLIENT] Connected successfully - server path verified!");

    // Send a test message
    let message = "Hello from verified client!";
    println!("   [CLIENT] Sending: '{}'", message);
    client.send_string(message).await?;

    let response = client.receive_string().await?;
    println!("   [CLIENT] Received: '{}'", response);

    println!("   [CLIENT] Communication completed successfully");
    Ok(())
}

async fn run_subprocess_test(same_path: bool) -> Result<()> {
    if same_path {
        println!("   [SUBPROCESS] Running same-path test...");
        match run_enforced_client().await {
            Ok(_) => {
                println!("   [SUBPROCESS] Same-path test succeeded!");
                std::process::exit(0);
            }
            Err(e) => {
                println!("   [SUBPROCESS] ERROR: Same-path test failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        println!("   [SUBPROCESS] Running different-path test (should fail)...");
        match run_enforced_client().await {
            Ok(_) => {
                println!("   [SUBPROCESS] ERROR: Different-path test succeeded when it should have failed!");
                std::process::exit(1);
            }
            Err(e) => {
                println!("   [SUBPROCESS] Different-path test correctly failed: {}", e);
                std::process::exit(0); // Exit 0 means the test passed (failure was expected)
            }
        }
    }
}
