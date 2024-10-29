use serde_json::{json, Value};
use uuid::Uuid;
use chrono::Utc;
use base64::engine::general_purpose;
use base64::Engine as _;
use reqwest::Client;
use tokio;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::env;

mod testing_utils;
use testing_utils::{generate_test_token, TEST_VERIFYING_KEY};

use crate::models::{User};

/// Helper function to initialize the test database connection.
async fn setup_test_db() -> PgPool {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
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
    let public_key = "TestPublicKeyBase64=="; // Replace with a valid base64-encoded public key if necessary.

    sqlx::query!(
        "INSERT INTO users (id, phone_number, public_key, scope, verified, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        user_id,
        phone_number,
        public_key,
        scope,
        true, // Assuming the user is verified for testing purposes.
        Utc::now(),
        Utc::now(),
    )
    .execute(pool)
    .await
    .expect("Failed to insert test user");

    user_id
}

/// Cleans up the test users from the database.
async fn cleanup_test_users(pool: &PgPool, user_ids: &[Uuid]) {
    sqlx::query!(
        "DELETE FROM users WHERE id = ANY($1)",
        user_ids
    )
    .execute(pool)
    .await
    .expect("Failed to delete test users");
}

#[tokio::test]
async fn test_get_profiles_endpoint_as_provider() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the test database pool.
    let pool = setup_test_db().await;

    // Insert a test provider and a test client.
    let provider_id = insert_test_user(&pool, "+1234567890", "provider").await;
    let client_id = insert_test_user(&pool, "+0987654321", "client").await;

    // Generate an access token for the provider.
    let access_token = generate_test_token(provider_id, "provider");

    // Prepare the user_ids query parameter.
    let user_ids = format!("{},{}", provider_id, client_id);

    // Initialize the HTTP client.
    let client = Client::new();

    // Send the GET request to /profiles.
    let response = client
        .get("http://localhost:8080/profiles")
        .header("Authorization", format!("Bearer {}", access_token))
        .query(&[("user_ids", user_ids.clone())])
        .send()
        .await?;

    // Assert that the response status is 200 OK.
    assert!(response.status().is_success(), "Expected 200 OK, got {}", response.status());

    // Parse the response body as JSON.
    let profiles: Vec<User> = response.json().await?;

    // Assert that both provider and client are returned.
    assert_eq!(profiles.len(), 2, "Expected 2 profiles, got {}", profiles.len());

    let returned_provider = profiles.iter().find(|u| u.id == provider_id);
    let returned_client = profiles.iter().find(|u| u.id == client_id);

    assert!(returned_provider.is_some(), "Provider profile not found in response");
    assert!(returned_client.is_some(), "Client profile not found in response");

    // Cleanup test users.
    cleanup_test_users(&pool, &[provider_id, client_id]).await;

    Ok(())
}

#[tokio::test]
async fn test_get_profiles_endpoint_as_client() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the test database pool.
    let pool = setup_test_db().await;

    // Insert a test provider and a test client.
    let provider_id = insert_test_user(&pool, "+1234567890", "provider").await;
    let client_id = insert_test_user(&pool, "+0987654321", "client").await;

    // Generate an access token for the client.
    let access_token = generate_test_token(client_id, "client");

    // Prepare the user_ids query parameter.
    let user_ids = format!("{},{}", provider_id, client_id);

    // Initialize the HTTP client.
    let client = Client::new();

    // Send the GET request to /profiles.
    let response = client
        .get("http://localhost:8080/profiles")
        .header("Authorization", format!("Bearer {}", access_token))
        .query(&[("user_ids", user_ids.clone())])
        .send()
        .await?;

    // Assert that the response status is 200 OK.
    assert!(response.status().is_success(), "Expected 200 OK, got {}", response.status());

    // Parse the response body as JSON.
    let profiles: Vec<User> = response.json().await?;

    // Since the client can see providers and themselves, both should be returned.
    assert_eq!(profiles.len(), 2, "Expected 2 profiles, got {}", profiles.len());

    let returned_provider = profiles.iter().find(|u| u.id == provider_id);
    let returned_client = profiles.iter().find(|u| u.id == client_id);

    assert!(returned_provider.is_some(), "Provider profile not found in response");
    assert!(returned_client.is_some(), "Client profile not found in response");

    // Cleanup test users.
    cleanup_test_users(&pool, &[provider_id, client_id]).await;

    Ok(())
}

#[tokio::test]
async fn test_get_profiles_endpoint_unauthorized() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the HTTP client.
    let client = Client::new();

    // Prepare the user_ids query parameter with random UUIDs.
    let user_ids = format!("{},{}", Uuid::new_v4(), Uuid::new_v4());

    // Send the GET request to /profiles without Authorization header.
    let response = client
        .get("http://localhost:8080/profiles")
        .query(&[("user_ids", user_ids.clone())])
        .send()
        .await?;

    // Assert that the response status is 401 Unauthorized.
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED, "Expected 401 Unauthorized, got {}", response.status());

    Ok(())
}

#[tokio::test]
async fn test_get_profiles_endpoint_invalid_token() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the HTTP client.
    let client = Client::new();

    // Prepare the user_ids query parameter with random UUIDs.
    let user_ids = format!("{},{}", Uuid::new_v4(), Uuid::new_v4());

    // Send the GET request to /profiles with an invalid token.
    let response = client
        .get("http://localhost:8080/profiles")
        .header("Authorization", "Bearer InvalidToken123")
        .query(&[("user_ids", user_ids.clone())])
        .send()
        .await?;

    // Assert that the response status is 401 Unauthorized.
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED, "Expected 401 Unauthorized, got {}", response.status());

    Ok(())
}
