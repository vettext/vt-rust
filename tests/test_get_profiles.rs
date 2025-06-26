use serde_json::{json, Value};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use base64::engine::general_purpose;
use base64::Engine as _;
use reqwest::Client;
use tokio;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::env;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use aes_gcm::aead::Aead;
use dotenv;
mod testing_utils;
use testing_utils::{generate_test_token, TEST_VERIFYING_KEY};

#[derive(FromRow, Debug, Serialize, Deserialize, Clone)]
pub struct UserWithPet {
    // User fields
    pub id: Option<Uuid>,
    pub phone_number: Option<String>,
    pub public_key: Option<String>,
    pub scope: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub profile_image_url: Option<String>,
    pub verified: Option<bool>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub updated_at: Option<DateTime<Utc>>,
    // Pet fields
    pub pet_id: Option<Uuid>,
    pub pet_user_id: Option<Uuid>,
    pub pet_name: Option<String>,
    pub pet_breed: Option<String>,
    pub pet_sex: Option<String>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub pet_birthday: Option<DateTime<Utc>>,
    pub pet_image_url: Option<String>,
    pub pet_color: Option<String>,
    pub pet_species: Option<String>,
    pub pet_spayed_neutered: Option<bool>,
    pub pet_weight: Option<i32>,
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
    let provider_id = insert_test_user(&pool, "0001231986", "provider").await;
    let client_id = insert_test_user(&pool, "0001231987", "client").await;

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
    let profiles: Vec<UserWithPet> = response.json().await?;

    // Assert that we get results (may be multiple rows due to LEFT JOIN with pets)
    assert!(!profiles.is_empty(), "Expected profiles, got empty response");

    // Check that we have data for both users
    let provider_profiles: Vec<_> = profiles.iter().filter(|p| p.id == Some(provider_id)).collect();
    let client_profiles: Vec<_> = profiles.iter().filter(|p| p.id == Some(client_id)).collect();

    assert!(!provider_profiles.is_empty(), "Provider profile not found in response");
    assert!(!client_profiles.is_empty(), "Client profile not found in response");

    // Check that the client has pet data
    let client_with_pet = client_profiles.iter().find(|p| p.pet_id.is_some());
    assert!(client_with_pet.is_some(), "Client pet data not found in response");

    // Cleanup test users.
    cleanup_test_users(&pool, &[provider_id, client_id]).await;

    Ok(())
}

#[tokio::test]
async fn test_get_profiles_endpoint_as_client() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the test database pool.
    let pool = setup_test_db().await;

    // Insert a test provider and a test client.
    let provider_id = insert_test_user(&pool, "0001231988", "provider").await;
    let client_id = insert_test_user(&pool, "0001231989", "client").await;

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
    let profiles: Vec<UserWithPet> = response.json().await?;

    // Assert that we get results (may be multiple rows due to LEFT JOIN with pets)
    assert!(!profiles.is_empty(), "Expected profiles, got empty response");

    // Check that we have data for both users
    let provider_profiles: Vec<_> = profiles.iter().filter(|p| p.id == Some(provider_id)).collect();
    let client_profiles: Vec<_> = profiles.iter().filter(|p| p.id == Some(client_id)).collect();

    assert!(!provider_profiles.is_empty(), "Provider profile not found in response");
    assert!(!client_profiles.is_empty(), "Client profile not found in response");

    // Check that the client has pet data
    let client_with_pet = client_profiles.iter().find(|p| p.pet_id.is_some());
    assert!(client_with_pet.is_some(), "Client pet data not found in response");

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
