use crate::error::{NamedPipeError, Result};
use crate::utils::{
    decrypt_message, encrypt_message, format_pipe_name, get_client_pid, verify_same_path,
};
use chacha20poly1305::{aead::KeyInit, ChaCha20Poly1305, Key};
use std::os::windows::prelude::AsRawHandle;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

/// A connection handler for named pipe server
pub struct NamedPipeConnection {
    server: NamedPipeServer,
    id: usize,
    cipher: Option<ChaCha20Poly1305>,
    enforce_same_path_client: bool,
}

impl NamedPipeConnection {
    /// Create a new connection without encryption
    pub fn new(server: NamedPipeServer, id: usize, enforce_same_path_client: bool) -> Self {
        Self {
            server,
            id,
            cipher: None,
            enforce_same_path_client,
        }
    }

    /// Create a new connection with encryption using a pre-shared key
    pub fn new_encrypted(
        server: NamedPipeServer,
        id: usize,
        key: &[u8; 32],
        enforce_same_path_client: bool,
    ) -> Self {
        let key = Key::from_slice(key);
        let cipher = ChaCha20Poly1305::new(key);

        Self {
            server,
            id,
            cipher: Some(cipher),
            enforce_same_path_client,
        }
    }

    /// Get the connection ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Verify that the client has the same executable path as this process, if enforcement is enabled.
    pub fn verify_client_path(&self) -> Result<()> {
        if !self.enforce_same_path_client {
            return Ok(());
        }

        let client_pid = get_client_pid(&self.server)?;
        verify_same_path(client_pid)
    }

    /// Send raw bytes to the client
    pub async fn send_bytes(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref cipher) = self.cipher {
            let encrypted_message = encrypt_message(cipher, data)?;

            // Send length-prefixed encrypted message
            let len = encrypted_message.len() as u32;
            self.server.write_all(&len.to_le_bytes()).await?;
            self.server.write_all(&encrypted_message).await?;
        } else {
            // Send unencrypted data with length prefix
            let len = data.len() as u32;
            self.server.write_all(&len.to_le_bytes()).await?;
            self.server.write_all(data).await?;
        }

        self.server.flush().await?;
        Ok(())
    }

    /// Receive raw bytes from the client
    pub async fn receive_bytes(&mut self) -> Result<Vec<u8>> {
        // Read length first
        let mut len_bytes = [0u8; 4];
        match self.server.read_exact(&mut len_bytes).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(NamedPipeError::ConnectionClosed);
            }
            Err(e) => return Err(NamedPipeError::Io(e)),
        }

        let len = u32::from_le_bytes(len_bytes) as usize;

        // Read data
        let mut buffer = vec![0u8; len];
        match self.server.read_exact(&mut buffer).await {
            Ok(_) => {
                if let Some(ref cipher) = self.cipher {
                    let plaintext = decrypt_message(cipher, &buffer)?;
                    Ok(plaintext)
                } else {
                    Ok(buffer)
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                Err(NamedPipeError::ConnectionClosed)
            }
            Err(e) => Err(NamedPipeError::Io(e)),
        }
    }

    /// Send a string message to the client
    pub async fn send_string(&mut self, message: &str) -> Result<()> {
        let data = message.as_bytes();
        self.send_bytes(data).await
    }

    /// Receive a string message from the client
    pub async fn receive_string(&mut self) -> Result<String> {
        let data = self.receive_bytes().await?;
        String::from_utf8(data).map_err(|e| {
            NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid UTF-8 string: {}", e),
            ))
        })
    }

    /// Send a JSON message to the client
    pub async fn send_json<T: serde::Serialize>(&mut self, message: &T) -> Result<()> {
        let json = serde_json::to_string(message).map_err(|e| {
            NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("JSON serialization failed: {}", e),
            ))
        })?;
        self.send_string(&json).await
    }

    /// Receive a JSON message from the client
    pub async fn receive_json<T: serde::de::DeserializeOwned>(&mut self) -> Result<T> {
        let json = self.receive_string().await?;
        serde_json::from_str(&json).map_err(|e| {
            NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("JSON deserialization failed: {}", e),
            ))
        })
    }
}

/// A named pipe server for Windows
pub struct NamedPipeServerStruct {
    pipe_name: String,
    is_running: Arc<Mutex<bool>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
    server_handle: Option<JoinHandle<Result<()>>>,
    connection_counter: Arc<Mutex<usize>>,
    cipher_key: Option<[u8; 32]>,
    enforce_same_path_client: bool,
}

impl NamedPipeServerStruct {
    /// Create a new named pipe server without encryption
    pub fn new(pipe_name: &str) -> Self {
        Self {
            pipe_name: format_pipe_name(pipe_name),
            is_running: Arc::new(Mutex::new(false)),
            shutdown_tx: None,
            server_handle: None,
            connection_counter: Arc::new(Mutex::new(0)),
            cipher_key: None,
            enforce_same_path_client: false,
        }
    }

    /// Create a new named pipe server with encryption.
    /// If key is None, uses a secure compile-time generated default key.
    /// If key is Some(key), uses the provided custom key.
    pub fn new_encrypted(pipe_name: &str, key: Option<[u8; 32]>) -> Self {
        let key_to_use = key.unwrap_or(crate::DEFAULT_ENCRYPTION_KEY);
        Self {
            pipe_name: format_pipe_name(pipe_name),
            is_running: Arc::new(Mutex::new(false)),
            shutdown_tx: None,
            server_handle: None,
            connection_counter: Arc::new(Mutex::new(0)),
            cipher_key: Some(key_to_use),
            enforce_same_path_client: false,
        }
    }

    /// Enable enforcement that the client must have the same executable path as the server.
    pub fn enforce_same_path_client(&mut self, enforce: bool) {
        self.enforce_same_path_client = enforce;
    }

    /// Create server with proper security attributes to allow all users
    fn create_server_with_security(pipe_name: &str) -> Result<NamedPipeServer> {
        // Create server with proper permissions
        let mut server_options = ServerOptions::new();

        // Enable write_dac to allow setting security information
        server_options.write_dac(true);

        // Create the server
        let server = server_options
            .create(pipe_name)
            .map_err(|e| NamedPipeError::Io(e))?;

        // Set security to allow all users to connect
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
            use windows::Win32::Security::Authorization::{SetSecurityInfo, SE_KERNEL_OBJECT};
            use windows::Win32::Security::DACL_SECURITY_INFORMATION;

            let result = SetSecurityInfo(
                HANDLE(server.as_raw_handle() as *mut std::ffi::c_void),
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                None, // owner
                None, // group
                None, // NULL DACL allows everyone
                None, // sacl
            );

            if result != ERROR_SUCCESS {
                eprintln!("Warning: Failed to set security info: {:?}", result);
                // Continue anyway, might work on some systems
            }
        }

        Ok(server)
    }

    /// Start the server and handle connections with a callback
    pub async fn start<F, Fut>(&mut self, handler: F) -> Result<()>
    where
        F: Fn(NamedPipeConnection) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send,
    {
        let mut is_running = self.is_running.lock().await;
        if *is_running {
            return Err(NamedPipeError::ServerAlreadyRunning(self.pipe_name.clone()));
        }
        *is_running = true;
        drop(is_running);

        let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        let pipe_name = self.pipe_name.clone();
        let connection_counter = Arc::clone(&self.connection_counter);
        let handler = Arc::new(handler);
        let cipher_key = self.cipher_key;
        let enforce_same_path_client = self.enforce_same_path_client;

        let handle = tokio::spawn(async move {
            // Create the first server instance with security attributes
            let mut current_server = match Self::create_server_with_security(&pipe_name) {
                Ok(server) => server,
                Err(e) => return Err(e),
            };

            loop {
                tokio::select! {
                    // Check for shutdown signal
                    _ = shutdown_rx.recv() => {
                        println!("Server received shutdown signal, stopping...");
                        break;
                    }

                    // Wait for connection
                    result = current_server.connect() => {
                        match result {
                            Ok(_) => {
                                // Get connection ID
                                let mut counter = connection_counter.lock().await;
                                *counter += 1;
                                let connection_id = *counter;
                                drop(counter);

                                // Create connection (encrypted if cipher_key is provided)
                                let connection = if let Some(key) = cipher_key {
                                    NamedPipeConnection::new_encrypted(current_server, connection_id, &key, enforce_same_path_client)
                                } else {
                                    NamedPipeConnection::new(current_server, connection_id, enforce_same_path_client)
                                };

                                // Spawn handler for this connection
                                let handler_clone = Arc::clone(&handler);
                                tokio::spawn(async move {
                                    // Verify client path if enforcement is enabled
                                    if let Err(e) = connection.verify_client_path() {
                                        eprintln!("Client path verification failed: {}", e);
                                        return;
                                    }

                                    if let Err(e) = handler_clone(connection).await {
                                        eprintln!("Connection handler error: {}", e);
                                    }
                                });

                                // Create a new server instance for the next connection
                                match Self::create_server_with_security(&pipe_name) {
                                    Ok(server) => {
                                        current_server = server;
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to create new server instance: {}", e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to accept connection: {}", e);
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            }
                        }
                    }
                }
            }

            Ok(())
        });

        // Wait for the server task to complete (this will block until shutdown)
        match handle.await {
            Ok(result) => result,
            Err(e) => Err(NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e,
            ))),
        }
    }

    /// Stop the server
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(());
        }

        if let Some(handle) = self.server_handle.take() {
            handle.await.map_err(|e| {
                NamedPipeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
            })??;
        }

        let mut is_running = self.is_running.lock().await;
        *is_running = false;

        Ok(())
    }

    /// Check if the server is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.lock().await
    }

    /// Get the pipe name
    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }
}

impl Drop for NamedPipeServerStruct {
    fn drop(&mut self) {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(());
        }
    }
}
