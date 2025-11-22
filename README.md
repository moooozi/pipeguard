# PipeGuard

A secure, async Rust library for Windows named pipe IPC with optional ChaCha20Poly1305 encryption.

## Features

- **Async I/O**: Built on Tokio for high-performance async communication
- **Encryption**: Optional ChaCha20Poly1305 AEAD encryption with automatic key management
- **Type Safety**: Strongly typed APIs with comprehensive error handling
- **Connection Management**: Automatic lifecycle management for multiple concurrent connections
- **Path Enforcement**: Optional verification that clients are the same executable

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
pipeguard = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }  # Optional, for JSON serialization
serde_json = "1.0"  # Optional, for JSON serialization
```

## Quick Start

### Basic Server

```rust
use pipeguard::{NamedPipeServerStruct, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let mut server = NamedPipeServerStruct::new("my_pipe");

    server.start(|mut connection| async move {
        while let Ok(data) = connection.receive_bytes().await {
            println!("Received: {:?}", data);
            connection.send_bytes(b"Response").await?;
        }
        Ok(())
    }).await?;

    Ok(())
}
```

### Basic Client

```rust
use pipeguard::{NamedPipeClientStruct, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let mut client = NamedPipeClientStruct::new("my_pipe");
    client.connect().await?;
    client.send_bytes(b"Hello!").await?;
    let response = client.receive_bytes().await?;
    println!("Response: {:?}", response);
    Ok(())
}
```

### Encrypted Communication

```rust
// Server with default encryption
let mut server = NamedPipeServerStruct::new_encrypted("secure_pipe", None);

// Client with default encryption (same key as server)
let mut client = NamedPipeClientStruct::new_encrypted("secure_pipe", None);

// Custom key encryption
let key = [1u8; 32];
let mut server = NamedPipeServerStruct::new_encrypted("secure_pipe", Some(key));
let mut client = NamedPipeClientStruct::new_encrypted("secure_pipe", Some(&key));
```

### JSON Communication

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Message { content: String }

// Send JSON
client.send_json(&Message { content: "Hello".to_string() }).await?;

// Receive JSON
let response: Message = client.receive_json().await?;
```

## API Overview

### Server
- `NamedPipeServerStruct::new(name)` - Create unencrypted server
- `NamedPipeServerStruct::new_encrypted(name, key)` - Create encrypted server
- `server.start(handler)` - Start server with connection handler

### Client
- `NamedPipeClientStruct::new(name)` - Create unencrypted client
- `NamedPipeClientStruct::new_encrypted(name, key)` - Create encrypted client
- `client.connect()` - Connect to server
- `client.send_bytes(data)` / `client.receive_bytes()` - Raw byte communication
- `client.send_json(data)` / `client.receive_json()` - JSON communication

### Connection
- `connection.send_bytes(data)` / `connection.receive_bytes()` - Raw byte communication
- `connection.send_json(data)` / `connection.receive_json()` - JSON communication

## Examples

Run included examples:

```bash
# Basic communication
cargo run --example basic_communication

# Encryption options
cargo run --example encryption_options

# Path enforcement (same-executable verification)
cargo run --example path_enforcement

# Multi-client server
cargo run --example multi_client_server

# Event-driven server with callbacks
cargo run --example event_driven_server
```

## Platform Support

**Windows only** - Uses Windows Named Pipes API. Cross-platform support may be added in future versions.

## License

MIT
