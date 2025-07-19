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

/// Inserts a test pet for a user.
/// Returns the pet's UUID.
async fn insert_test_pet(pool: &PgPool, user_id: Uuid) -> Uuid {
    let pet_id = Uuid::new_v4();
    
    sqlx::query!(
        "INSERT INTO pets (id, user_id, name, breed, sex, birthday, color, species, spayed_neutered, weight) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        pet_id,
        user_id,
        "Test Pet",
        "Test Breed", 
        "M",
        chrono::Utc::now(),
        "Brown",
        "Dog",
        true,
        25
    )
    .execute(pool)
    .await
    .expect("Failed to insert test pet");

    pet_id
}

/// Cleans up the test data from the database.
async fn cleanup_test_data(pool: &PgPool, user_ids: &[Uuid]) {
    // Delete pets first due to foreign key constraint
    sqlx::query!(
        "DELETE FROM pets WHERE user_id = ANY($1)",
        user_ids
    )
    .execute(pool)
    .await
    .expect("Failed to delete test pets");

    // Then delete users
    sqlx::query!(
        "DELETE FROM users WHERE id = ANY($1)",
        user_ids
    )
    .execute(pool)
    .await
    .expect("Failed to delete test users");
}

#[tokio::test]
async fn test_websocket_connection() -> Result<(), Box<dyn std::error::Error>> {
    // Setup test database
    let pool = setup_test_db().await;
    
    // Create test users
    let client_id = insert_test_user(&pool, "0001231734", "client").await;
    let provider_id = insert_test_user(&pool, "0001231735", "provider").await;
    
    // Create a test pet for the client
    let pet_id = insert_test_pet(&pool, client_id).await;
    
    // Generate a test token for the client
    let (access_token, _) = generate_test_token(client_id, "client")
        .expect("Failed to generate test token");
    
    // Connect WebSocket client with authentication
    let url = Url::parse(&format!("ws://localhost:8080/ws/?token={}", access_token)).unwrap();
    let (mut ws_stream, _) = connect_async(url.clone()).await.expect("Failed to connect");
    
    println!("Connected with user ID: {}", client_id);
    
    // Test 1: Try to send a message to a non-existent conversation (should get error)
    let message = json!({
        "sender_id": client_id.to_string(),
        "event": "message",
        "params": {
            "conversation_id": Uuid::new_v4().to_string(),
            "content": "Test message content"
        }
    });
    
    println!("Test 1: Sending message to non-existent conversation");
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
                    println!("✅ Received expected error response (conversation doesn't exist)");
                } else {
                    println!("Received response: {}", text);
                }
                assert!(!text.is_empty());
            }
            _ => println!("Received non-text message"),
        }
    }
    
    // Test 2: Create a new conversation
    let new_conversation_msg = json!({
        "sender_id": client_id.to_string(),
        "event": "new_conversation",
        "params": {
            "pet_id": pet_id.to_string(),
            "providers": [provider_id.to_string()]
        }
    });
    
    println!("Test 2: Creating new conversation");
    ws_stream.send(Message::Text(new_conversation_msg.to_string())).await?;
    
    // Wait for response
    sleep(Duration::from_secs(1)).await;
    
    // Read response from server
    let mut conversation_id = None;
    if let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        match msg {
            Message::Text(text) => {
                println!("Received response: {}", text);
                // Parse the response to get the conversation ID
                if let Ok(response_json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(params) = response_json.get("params") {
                        if let Some(id) = params.get("id") {
                            conversation_id = Some(id.as_str().unwrap().to_string());
                            println!("✅ Created conversation with ID: {}", conversation_id.as_ref().unwrap());
                        }
                    }
                }
                assert!(!text.is_empty());
            }
            _ => println!("Received non-text message"),
        }
    }
    
    // Test 3: Send a message to the created conversation (this should test the trigger fix)
    if let Some(conv_id) = conversation_id {
        let message = json!({
            "sender_id": client_id.to_string(),
            "event": "message",
            "params": {
                "conversation_id": conv_id,
                "content": "Hello, this is a test message to verify the trigger fix!"
            }
        });
        
        println!("Test 3: Sending message to created conversation");
        ws_stream.send(Message::Text(message.to_string())).await?;
        
        // Wait for response
        sleep(Duration::from_secs(1)).await;
        
        // Read responses until we find message_sent
        let mut message_sent_found = false;
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 10;
        
        while !message_sent_found && attempts < MAX_ATTEMPTS {
            if let Some(msg) = ws_stream.next().await {
                let msg = msg?;
                match msg {
                    Message::Text(text) => {
                        println!("Received response: {}", text);
                        
                        if text.contains("message_sent") {
                            println!("✅ Message sent successfully! Trigger fix is working.");
                            message_sent_found = true;
                            
                            // Parse the response to verify the message data
                            if let Ok(response_json) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Some(params) = response_json.get("params") {
                                    // Handle nested params structure
                                    let message_params = if params.get("params").is_some() {
                                        params.get("params").unwrap()
                                    } else {
                                        params
                                    };
                                    
                                    // Verify the message contains expected fields
                                    assert!(message_params.get("id").is_some(), "Message response missing 'id' field");
                                    assert!(message_params.get("conversation_id").is_some(), "Message response missing 'conversation_id' field");
                                    assert!(message_params.get("sender_id").is_some(), "Message response missing 'sender_id' field");
                                    assert!(message_params.get("content").is_some(), "Message response missing 'content' field");
                                    assert!(message_params.get("timestamp").is_some(), "Message response missing 'timestamp' field");
                                    
                                    // Verify the content matches what we sent
                                    let content = message_params.get("content").unwrap().as_str().unwrap();
                                    assert_eq!(content, "Hello, this is a test message to verify the trigger fix!", "Message content doesn't match");
                                    
                                    // Verify the conversation_id matches
                                    let response_conv_id = message_params.get("conversation_id").unwrap().as_str().unwrap();
                                    assert_eq!(response_conv_id, conv_id, "Conversation ID doesn't match");
                                    
                                    println!("✅ Message response contains all expected fields and data");
                                } else {
                                    panic!("Message response missing 'params' field");
                                }
                            } else {
                                panic!("Failed to parse message response as JSON");
                            }
                        } else if text.contains("error") {
                            println!("❌ Error sending message: {}", text);
                            panic!("Failed to send message: {}", text);
                        } else {
                            // This is likely a conversation_created or new_conversation_invitation response
                            // Just continue reading until we find message_sent
                            println!("Received other response, continuing to look for message_sent...");
                        }
                    }
                    _ => println!("Received non-text message"),
                }
            } else {
                break;
            }
            attempts += 1;
        }
        
        if !message_sent_found {
            panic!("Did not receive message_sent response after {} attempts", MAX_ATTEMPTS);
        }
    } else {
        println!("❌ Failed to create conversation, skipping message test");
        panic!("Failed to create conversation");
    }

    // Cleanup
    cleanup_test_data(&pool, &[client_id, provider_id]).await;
    
    Ok(())
}
