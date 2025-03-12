use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;
use serde_json::json;
use uuid::Uuid;
use futures::{StreamExt, SinkExt};

#[tokio::test]
async fn test_websocket_connection() -> Result<(), Box<dyn std::error::Error>> {
    // Connect WebSocket client
    let url = Url::parse("ws://localhost:8080/ws/").unwrap();
    let (mut ws_stream, _) = connect_async(url.clone()).await.expect("Failed to connect");
    
    // Generate a test user ID
    let test_user_id = Uuid::new_v4();
    
    // Test sending a message in the correct format
    let message = json!({
        "sender_id": test_user_id.to_string(),
        "event": "message",
        "params": {
            "content": "Test message content",
            "conversation_id": Uuid::new_v4().to_string()
        }
    });
    
    println!("Sending message: {}", message.to_string());
    ws_stream.send(Message::Text(message.to_string())).await?;
    
    // Wait for response
    sleep(Duration::from_secs(1)).await;
    
    // Read response from server
    if let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        match msg {
            Message::Text(text) => {
                println!("Received response: {}", text);
                // Just check if we get a response, not checking content for now
                assert!(!text.is_empty());
            }
            _ => println!("Received non-text message"),
        }
    }
    
    // Test a new conversation event
    let new_conversation_msg = json!({
        "sender_id": test_user_id.to_string(),
        "event": "new_conversation",
        "params": {
            "pet_id": Uuid::new_v4().to_string(),
            "providers": [Uuid::new_v4().to_string()]
        }
    });
    
    println!("Sending new conversation request: {}", new_conversation_msg.to_string());
    ws_stream.send(Message::Text(new_conversation_msg.to_string())).await?;
    
    // Wait for response
    sleep(Duration::from_secs(1)).await;
    
    // Read response from server
    if let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        match msg {
            Message::Text(text) => {
                println!("Received response: {}", text);
                // Just check if we get a response, not checking content for now
                assert!(!text.is_empty());
            }
            _ => println!("Received non-text message"),
        }
    }

    Ok(())
}
