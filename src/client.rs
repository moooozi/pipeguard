use crate::error::{NamedPipeError, Result};
use crate::utils::{encrypt_message, format_pipe_name, get_server_pid, verify_same_path};
use chacha20poly1305::{aead::KeyInit, ChaCha20Poly1305, Key};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

/// A named pipe client for Windows
pub struct NamedPipeClientStruct {
    client: Option<NamedPipeClient>,
    pipe_name: String,
    cipher: Option<ChaCha20Poly1305>,
    enforce_same_path_server: bool,
}

impl NamedPipeClientStruct {
    /// Create a new named pipe client
    pub fn new(pipe_name: &str) -> Self {
        Self {
            client: None,
            pipe_name: format_pipe_name(pipe_name),
            cipher: None,
            enforce_same_path_server: false,
        }
    }

    /// Create a new named pipe client with encryption.
    /// If key is None, uses a secure compile-time generated default key.
    /// If key is Some(key), uses the provided custom key.
    pub fn new_encrypted(pipe_name: &str, key: Option<&[u8; 32]>) -> Self {
        let key_to_use = key.unwrap_or(&crate::DEFAULT_ENCRYPTION_KEY);
        let key = Key::from_slice(key_to_use);
        let cipher = ChaCha20Poly1305::new(key);

        Self {
            client: None,
            pipe_name: format_pipe_name(pipe_name),
            cipher: Some(cipher),
            enforce_same_path_server: false,
        }
    }

    /// Enable enforcement that the server must have the same executable path as this process.
    pub fn enforce_same_path_server(&mut self, enforce: bool) {
        self.enforce_same_path_server = enforce;
    }
    /// Connect to the named pipe server
    pub async fn connect(&mut self) -> Result<()> {
        let client = ClientOptions::new()
            .open(&self.pipe_name)
            .map_err(NamedPipeError::Io)?;

        self.client = Some(client);

        // Verify server path if enforcement is enabled
        self.verify_server_path()?;

        Ok(())
    }

    /// Send raw bytes to the server
    pub async fn send_bytes(&mut self, data: &[u8]) -> Result<()> {
        let client = self.client.as_mut().ok_or(NamedPipeError::NotConnected)?;

        if let Some(ref cipher) = self.cipher {
            let encrypted_message = encrypt_message(cipher, data)?;

            // Send length-prefixed encrypted message
            let len = encrypted_message.len() as u32;
            client.write_all(&len.to_le_bytes()).await?;
            client.write_all(&encrypted_message).await?;
        } else {
            // Send unencrypted data with length prefix
            let len = data.len() as u32;
            client.write_all(&len.to_le_bytes()).await?;
            client.write_all(data).await?;
        }

        client.flush().await?;
        Ok(())
    }

    /// Receive raw bytes from the server
    pub async fn receive_bytes(&mut self) -> Result<Vec<u8>> {
        let client = self.client.as_mut().ok_or(NamedPipeError::NotConnected)?;

        // Read length first
        let mut len_bytes = [0u8; 4];
        client.read_exact(&mut len_bytes).await?;
        let len = u32::from_le_bytes(len_bytes) as usize;

        // Read data
        let mut buffer = vec![0u8; len];
        client.read_exact(&mut buffer).await?;

        if let Some(ref cipher) = self.cipher {
            let plaintext = crate::utils::decrypt_message(cipher, &buffer)?;
            Ok(plaintext)
        } else {
            Ok(buffer)
        }
    }

    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    /// Verify that the server has the same executable path as this process, if enforcement is enabled.
    pub fn verify_server_path(&self) -> Result<()> {
        if !self.enforce_same_path_server {
            return Ok(());
        }

        let client = self.client.as_ref().ok_or(NamedPipeError::NotConnected)?;
        let server_pid = get_server_pid(client)?;
        verify_same_path(server_pid)
    }

    /// Send a string message to the server
    pub async fn send_string(&mut self, message: &str) -> Result<()> {
        let data = message.as_bytes();
        self.send_bytes(data).await
    }

    /// Receive a string message from the server
    pub async fn receive_string(&mut self) -> Result<String> {
        let data = self.receive_bytes().await?;
        String::from_utf8(data).map_err(|e| {
            NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid UTF-8 string: {}", e),
            ))
        })
    }

    /// Send a JSON message to the server
    pub async fn send_json<T: serde::Serialize>(&mut self, message: &T) -> Result<()> {
        let json = serde_json::to_string(message).map_err(|e| {
            NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("JSON serialization failed: {}", e),
            ))
        })?;
        self.send_string(&json).await
    }

    /// Receive a JSON message from the server
    pub async fn receive_json<T: serde::de::DeserializeOwned>(&mut self) -> Result<T> {
        let json = self.receive_string().await?;
        serde_json::from_str(&json).map_err(|e| {
            NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("JSON deserialization failed: {}", e),
            ))
        })
    }

    /// Disconnect from the server
    pub fn disconnect(&mut self) {
        self.client = None;
    }

    /// Get the pipe name
    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }
}

impl Drop for NamedPipeClientStruct {
    fn drop(&mut self) {
        self.disconnect();
    }
}
