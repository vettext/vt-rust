use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;
use serde_json::json;
use uuid::Uuid;
use futures::{StreamExt, SinkExt};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::env;
use dotenv;

mod testing_utils;
use testing_utils::generate_test_token;

/// Helper function to initialize the test database connection.
async fn setup_test_db() -> PgPool {
    dotenv::dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create test database pool")
}

/// Inserts a test user into the database.
/// Returns the user's UUID.
async fn insert_test_user(pool: &PgPool, phone_number: &str, scope: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    let public_key = "TestPublicKeyBase64==";

    sqlx::query!(
        "INSERT INTO users (id, phone_number, public_key, scope, verified, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        user_id,
        phone_number,
        public_key,
        scope,
        true,
        chrono::Utc::now(),
        chrono::Utc::now(),
    )
    .execute(pool)
    .await
    .expect("Failed to insert test user");

    user_id
}

/// Cleans up the test user from the database.
async fn cleanup_test_user(pool: &PgPool, user_id: Uuid) {
    sqlx::query!(
        "DELETE FROM users WHERE id = $1",
        user_id
    )
    .execute(pool)
    .await
    .expect("Failed to delete test user");
}

#[tokio::test]
async fn test_websocket_connection() -> Result<(), Box<dyn std::error::Error>> {
    // Setup test database
    let pool = setup_test_db().await;
    
    // Create a test user
    let test_user_id = insert_test_user(&pool, "0001231990", "client").await;
    
    // Generate a test token for the user
    let (access_token, _) = generate_test_token(test_user_id, "client")
        .expect("Failed to generate test token");
    
    // Connect WebSocket client with authentication
    let url = Url::parse(&format!("ws://localhost:8080/ws/?token={}", access_token)).unwrap();
    let (mut ws_stream, _) = connect_async(url.clone()).await.expect("Failed to connect");
    
    println!("Connected with user ID: {}", test_user_id);
    
    // Test sending a message in the correct format
    let message = json!({
        "sender_id": test_user_id.to_string(),
        "event": "message",
        "params": {
            "conversation_id": Uuid::new_v4().to_string(),
            "content": "Test message content"
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
                // Check if we get an error response (expected since conversation doesn't exist)
                if text.contains("error") {
                    println!("Received expected error response (conversation doesn't exist)");
                } else {
                    println!("Received response: {}", text);
                }
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
                // Check if we get an error response (expected since pet doesn't exist)
                if text.contains("error") {
                    println!("Received expected error response (pet doesn't exist)");
                } else {
                    println!("Received response: {}", text);
                }
                assert!(!text.is_empty());
            }
            _ => println!("Received non-text message"),
        }
    }

    // Cleanup
    cleanup_test_user(&pool, test_user_id).await;
    
    Ok(())
}
