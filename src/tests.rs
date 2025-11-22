#[cfg(test)]
mod tests {
    use crate::{NamedPipeClientStruct, NamedPipeServerStruct};
    use std::time::Duration;
    use tokio::time::sleep;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestMessage {
        id: u32,
        content: String,
    }

    #[tokio::test]
    async fn test_basic_string_communication() {
        let pipe_name = "test_basic_string";
        
        // Start server
        let mut server = NamedPipeServerStruct::new(pipe_name);
        let server_handle = tokio::spawn(async move {
            server.start(|mut connection| async move {
                let message = connection.receive_string().await?;
                connection.send_string(&format!("Received: {}", message)).await?;
                Ok(())
            }).await
        });
        
        // Give server time to start
        sleep(Duration::from_millis(100)).await;
        
        // Connect client and test
        let mut client = NamedPipeClientStruct::new(pipe_name);
        client.connect().await.unwrap();
        
        client.send_string("Hello, Test!").await.unwrap();
        let response = client.receive_string().await.unwrap();
        
        assert_eq!(response, "Received: Hello, Test!");
        
        // Clean up
        server_handle.abort();
    }

    #[tokio::test]
    async fn test_json_communication() {
        let pipe_name = "test_json_comm";
        
        // Start server
        let mut server = NamedPipeServerStruct::new(pipe_name);
        let server_handle = tokio::spawn(async move {
            server.start(|mut connection| async move {
                let message: TestMessage = connection.receive_json().await?;
                let response = TestMessage {
                    id: message.id + 1,
                    content: format!("Processed: {}", message.content),
                };
                connection.send_json(&response).await?;
                Ok(())
            }).await
        });
        
        // Give server time to start
        sleep(Duration::from_millis(100)).await;
        
        // Connect client and test
        let mut client = NamedPipeClientStruct::new(pipe_name);
        client.connect().await.unwrap();
        
        let request = TestMessage {
            id: 42,
            content: "Test message".to_string(),
        };
        
        client.send_json(&request).await.unwrap();
        let response: TestMessage = client.receive_json().await.unwrap();
        
        assert_eq!(response.id, 43);
        assert_eq!(response.content, "Processed: Test message");
        
        // Clean up
        server_handle.abort();
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let pipe_name = "test_multiple";
        
        // Start server that echoes messages until "quit"
        let mut server = NamedPipeServerStruct::new(pipe_name);
        let server_handle = tokio::spawn(async move {
            server.start(|mut connection| async move {
                loop {
                    match connection.receive_string().await {
                        Ok(message) => {
                            if message == "quit" {
                                connection.send_string("goodbye").await?;
                                break;
                            }
                            connection.send_string(&format!("Echo: {}", message)).await?;
                        }
                        Err(_) => break,
                    }
                }
                Ok(())
            }).await
        });
        
        // Give server time to start
        sleep(Duration::from_millis(100)).await;
        
        // Connect client and test multiple messages
        let mut client = NamedPipeClientStruct::new(pipe_name);
        client.connect().await.unwrap();
        
        for i in 1..=3 {
            let message = format!("Message {}", i);
            client.send_string(&message).await.unwrap();
            let response = client.receive_string().await.unwrap();
            assert_eq!(response, format!("Echo: {}", message));
        }
        
        // Send quit message
        client.send_string("quit").await.unwrap();
        let response = client.receive_string().await.unwrap();
        assert_eq!(response, "goodbye");
        
        // Clean up
        server_handle.abort();
    }

    #[tokio::test]
    async fn test_client_connection_state() {
        let mut client = NamedPipeClientStruct::new("test_connection_state");
        
        assert!(!client.is_connected());
        
        // Note: This will fail since no server is running, but we test the state
        assert!(client.connect().await.is_err());
        assert!(!client.is_connected());
        
        client.disconnect();
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_pipe_name_formatting() {
        let client1 = NamedPipeClientStruct::new("test_pipe");
        assert_eq!(client1.pipe_name(), "\\\\.\\pipe\\test_pipe");
        
        let client2 = NamedPipeClientStruct::new("\\\\.\\pipe\\already_formatted");
        assert_eq!(client2.pipe_name(), "\\\\.\\pipe\\already_formatted");
    }
}
