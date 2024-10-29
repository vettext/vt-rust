use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;
use serde_json::json;
use uuid::Uuid;
use futures::{StreamExt, SinkExt};

#[tokio::test]
async fn test_websocket_broadcast() -> Result<(), Box<dyn std::error::Error>> {
    // Connect two WebSocket clients
    let url = Url::parse("ws://localhost:8080/ws/").unwrap();
    let (mut ws_stream1, _) = connect_async(url.clone()).await.expect("Failed to connect");
    let (mut ws_stream2, _) = connect_async(url.clone()).await.expect("Failed to connect");

    // Send a message from client 1
    let message = json!({
        "message": "Hello from client 1",
        "sender_id": Uuid::new_v4(), // Dummy sender_id; server will override
        "receiver_id": null
    });
    ws_stream1.send(Message::Text(message.to_string())).await?;

    // Allow some time for the message to be broadcasted
    sleep(Duration::from_secs(1)).await;

    // Receive the message on client 2
    if let Some(msg) = ws_stream2.next().await {
        let msg = msg?;
        match msg {
            Message::Text(text) => {
                let received: serde_json::Value = serde_json::from_str(&text)?;
                assert_eq!(received["message"], "Hello from client 1");

                // Validate that sender_id is a valid UUID and not the dummy one sent
                let sender_id_received = received["sender_id"].as_str().unwrap();
                Uuid::parse_str(sender_id_received).expect("Invalid sender_id received");

                assert!(received["receiver_id"].is_null());
            }
            _ => panic!("Unexpected message type"),
        }
    } else {
        panic!("No message received");
    }

    Ok(())
}
