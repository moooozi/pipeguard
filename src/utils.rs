use crate::error::{NamedPipeError, Result};
use chacha20poly1305::{
    aead::{Aead, AeadCore, OsRng},
    ChaCha20Poly1305, Nonce,
};
use std::os::windows::io::AsRawHandle;
use windows::core::PWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION,
    PROCESS_VM_READ,
};

/// Encrypt data using ChaCha20Poly1305 and prepend nonce
pub fn encrypt_message(cipher: &ChaCha20Poly1305, data: &[u8]) -> Result<Vec<u8>> {
    // Generate a random nonce
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);

    // Encrypt the data
    let ciphertext = cipher.encrypt(&nonce, data).map_err(|e| {
        NamedPipeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Encryption failed: {}", e),
        ))
    })?;

    // Prepare encrypted message: nonce (12 bytes) + ciphertext
    let mut encrypted_message = Vec::with_capacity(12 + ciphertext.len());
    encrypted_message.extend_from_slice(&nonce);
    encrypted_message.extend_from_slice(&ciphertext);

    Ok(encrypted_message)
}

/// Decrypt data using ChaCha20Poly1305, expecting nonce prepended
pub fn decrypt_message(cipher: &ChaCha20Poly1305, data: &[u8]) -> Result<Vec<u8>> {
    // For encrypted data: first 12 bytes are nonce, rest is ciphertext
    if data.len() < 12 {
        return Err(NamedPipeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Encrypted message too short",
        )));
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    // Decrypt the data
    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|e| {
        NamedPipeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Decryption failed: {}", e),
        ))
    })?;

    Ok(plaintext)
}

/// Get the executable path of a process by its PID
pub fn get_process_path(pid: u32) -> Result<String> {
    // Open process
    let process = unsafe {
        OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).map_err(|_| {
            NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Cannot open process",
            ))
        })?
    };

    // Get path
    let mut buffer = [0u16; 260]; // MAX_PATH
    let mut length = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .map_err(|_| {
            NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to query process image name",
            ))
        })?;
    }
    Ok(String::from_utf16_lossy(&buffer[..length as usize]))
}

/// Get the PID of the server process from a client handle
pub fn get_server_pid<H: AsRawHandle>(handle: &H) -> Result<u32> {
    let mut server_pid: u32 = 0;
    unsafe {
        if GetNamedPipeServerProcessId(
            HANDLE(handle.as_raw_handle() as *mut std::ffi::c_void),
            &mut server_pid,
        )
        .is_err()
        {
            return Err(NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to get server PID",
            )));
        }
    }
    Ok(server_pid)
}

/// Get the PID of the client process from a server handle
pub fn get_client_pid<H: AsRawHandle>(handle: &H) -> Result<u32> {
    let mut client_pid: u32 = 0;
    unsafe {
        if GetNamedPipeClientProcessId(
            HANDLE(handle.as_raw_handle() as *mut std::ffi::c_void),
            &mut client_pid,
        )
        .is_err()
        {
            return Err(NamedPipeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to get client PID",
            )));
        }
    }
    Ok(client_pid)
}

/// Verify that the other process has the same executable path as this process
pub fn verify_same_path(other_pid: u32) -> Result<()> {
    let other_path = get_process_path(other_pid)?;
    let self_path = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Compare paths (case-insensitive on Windows)
    if !other_path.eq_ignore_ascii_case(&self_path) {
        return Err(NamedPipeError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Process path does not match",
        )));
    }

    Ok(())
}

/// Format pipe name to Windows named pipe format
pub fn format_pipe_name(name: &str) -> String {
    if name.starts_with("\\\\.\\pipe\\") {
        name.to_string()
    } else {
        format!("\\\\.\\pipe\\{}", name)
    }
}
