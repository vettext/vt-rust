use reqwest::Client;
use uuid::Uuid;
use serde_json::{json, Value};
use std::error::Error as StdError;
use chrono::NaiveDate;

mod testing_utils;
use testing_utils::generate_test_token;

fn setup_test_environment() {
    // Try to load from the .env file in the project root
    dotenv::dotenv().ok();
}

// Change this to toggle between local and production servers
const USE_LOCAL_SERVER: bool = true;
const LOCAL_SERVER_URL: &str = "http://localhost:8080";
// Replace with your server URL
const PROD_SERVER_URL: &str = "http://34.145.29.219:8080";

fn get_server_url() -> &'static str {
    if USE_LOCAL_SERVER {
        LOCAL_SERVER_URL
    } else {
        PROD_SERVER_URL
    }
}

// For testing, we'll create a random user ID (needs to exist in the database)
// In a real test environment, you would create a test user first or use a known test user
const USER_ID: &str = "e1bf84be-0d14-42ec-8f1c-77918c3b9259"; // Replace with a real user ID for testing

#[tokio::test]
async fn test_create_and_update_pet() -> Result<(), Box<dyn StdError>> {
    // Load environment variables from .env file
    setup_test_environment();
    
    // Parse the user ID
    let user_id = Uuid::parse_str(USER_ID)?;
    
    // Generate an access token for the user
    let (access_token, _) = generate_test_token(user_id, "client")
        .expect("Failed to generate test token");
    
    // Initialize the HTTP client
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    
    // Construct the URL
    let url = format!("{}/pet", get_server_url());
    
    println!("Step 1: Creating a new pet...");
    
    // Step 1: Create a new pet
    let create_data = json!({
        "name": "Test Pet",
        "breed": "Test Breed",
        "sex": "F",
        "birthday": "2020-01-01"
    });
    
    // Send the POST request to create pet
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&create_data)
        .send()
        .await?;
    
    // Get the status and body
    let status = response.status();
    let body = response.text().await?;
    
    println!("Create response status: {}", status);
    println!("Create response body: {}", body);
    
    // Assert that the response is successful (201 Created)
    assert!(status.is_success(), "Pet creation failed with status {}: {}", status, body);
    
    // Parse the response as JSON
    let create_response: Value = serde_json::from_str(&body)?;
    
    // Extract the pet ID from the creation response
    let pet_id = create_response["pet"]["id"].as_str().unwrap();
    println!("Created pet with ID: {}", pet_id);
    
    // Step 2: Update the pet we just created
    println!("Step 2: Updating the pet...");
    
    // Create the update data
    let update_data = json!({
        "id": pet_id,
        "name": "Updated Test Pet",
        "breed": "Updated Test Breed",
        "sex": "M"
        // Omit other fields to test partial updates
    });
    
    // Send the POST request to update pet
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&update_data)
        .send()
        .await?;
    
    // Get the status and body
    let status = response.status();
    let body = response.text().await?;
    
    println!("Update response status: {}", status);
    println!("Update response body: {}", body);
    
    // Assert that the response is successful
    assert!(status.is_success(), "Pet update failed with status {}: {}", status, body);
    
    // Parse the response as JSON
    let update_response: Value = serde_json::from_str(&body)?;
    
    // Verify the response contains expected fields
    assert!(update_response["message"].is_string(), "Response missing 'message' field");
    assert!(update_response["pet"].is_object(), "Response missing 'pet' object");
    
    // Verify the pet was actually updated
    let pet = &update_response["pet"];
    assert_eq!(pet["id"].as_str().unwrap(), pet_id, "Pet ID in response doesn't match request");
    assert_eq!(pet["name"], "Updated Test Pet", "Pet name wasn't updated correctly");
    assert_eq!(pet["breed"], "Updated Test Breed", "Pet breed wasn't updated correctly");
    assert_eq!(pet["sex"], "M", "Pet sex wasn't updated correctly");
    
    // Step 3: Cleanup - delete the pet
    println!("Step 3: Cleaning up by deleting the pet...");
    let delete_data = json!({
        "id": pet_id
    });
    
    let response = client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&delete_data)
        .send()
        .await?;
    
    let status = response.status();
    assert!(status.is_success(), "Pet deletion failed with status {}", status);
    
    Ok(())
} 