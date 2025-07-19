use uuid::Uuid;
use chrono::{DateTime, Utc};
use reqwest::Client;
use tokio;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::env;
use serde::{Deserialize, Serialize};
use dotenv;
mod testing_utils;
use testing_utils::generate_test_token;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserProfile {
    pub id: Uuid,
    pub phone_number: String,
    pub public_key: String,
    pub scope: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub profile_image_url: Option<String>,
    pub verified: bool,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub updated_at: DateTime<Utc>,
    pub pets: Vec<Pet>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pet {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub breed: String,
    pub sex: String,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub birthday: Option<DateTime<Utc>>,
    pub pet_image_url: Option<String>,
    pub color: Option<String>,
    pub species: Option<String>,
    pub spayed_neutered: Option<bool>,
    pub weight: Option<i32>,
}

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

/// Inserts a test pet for a user.
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
        Utc::now(),
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

/// Cleans up the test users and their pets from the database.
async fn cleanup_test_users(pool: &PgPool, user_ids: &[Uuid]) {
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
async fn test_get_profiles_endpoint_as_provider() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the test database pool.
    let pool = setup_test_db().await;

    // Insert a test provider and a test client.
    let provider_id = insert_test_user(&pool, "0001231990", "provider").await;
    let client_id = insert_test_user(&pool, "0001231991", "client").await;

    // Add a pet to the client
    let _pet_id = insert_test_pet(&pool, client_id).await;

    // Generate an access token for the provider.
    let (access_token, _) = generate_test_token(provider_id, "provider")
        .expect("Failed to generate test token");

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
    let profiles: Vec<UserProfile> = response.json().await?;

    // Assert that we get results (one user per user, with pets grouped)
    assert!(!profiles.is_empty(), "Expected profiles, got empty response");

    // Check that we have data for both users
    let provider_profile = profiles.iter().find(|p| p.id == provider_id);
    let client_profile = profiles.iter().find(|p| p.id == client_id);

    assert!(provider_profile.is_some(), "Provider profile not found in response");
    assert!(client_profile.is_some(), "Client profile not found in response");

    // Check that the client has pet data
    let client_profile = client_profile.unwrap();
    assert!(!client_profile.pets.is_empty(), "Client pet data not found in response");
    assert_eq!(client_profile.pets.len(), 1, "Expected 1 pet for client");
    assert_eq!(client_profile.pets[0].name, "Test Pet", "Pet name mismatch");

    // Cleanup test users.
    cleanup_test_users(&pool, &[provider_id, client_id]).await;

    Ok(())
}

#[tokio::test]
async fn test_get_profiles_endpoint_as_client() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the test database pool.
    let pool = setup_test_db().await;

    // Insert a test provider and a test client.
    let provider_id = insert_test_user(&pool, "0001231720", "provider").await;
    let client_id = insert_test_user(&pool, "0001231721", "client").await;

    // Add a pet to the client
    let _pet_id = insert_test_pet(&pool, client_id).await;

    // Generate an access token for the client.
    let (access_token, _) = generate_test_token(client_id, "client")
        .expect("Failed to generate test token");

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
    let profiles: Vec<UserProfile> = response.json().await?;

    // Assert that we get results (one user per user, with pets grouped)
    assert!(!profiles.is_empty(), "Expected profiles, got empty response");

    // Check that we have data for both users
    let provider_profile = profiles.iter().find(|p| p.id == provider_id);
    let client_profile = profiles.iter().find(|p| p.id == client_id);

    assert!(provider_profile.is_some(), "Provider profile not found in response");
    assert!(client_profile.is_some(), "Client profile not found in response");

    // Check that the client has pet data
    let client_profile = client_profile.unwrap();
    assert!(!client_profile.pets.is_empty(), "Client pet data not found in response");
    assert_eq!(client_profile.pets.len(), 1, "Expected 1 pet for client");
    assert_eq!(client_profile.pets[0].name, "Test Pet", "Pet name mismatch");

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
