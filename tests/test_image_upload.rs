use reqwest::Client;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use serde_json::Value;
use std::error::Error as StdError;

mod testing_utils;
use testing_utils::generate_test_token;

fn setup_test_environment() {
    // Try to load from the .env file in the project root
    dotenv::dotenv().ok();
}

// Change this to toggle between local and production servers
const USE_LOCAL_SERVER: bool = false;
const LOCAL_SERVER_URL: &str = "http://localhost:8080";
const PROD_SERVER_URL: &str = "http://34.145.29.219:8080";

fn get_server_url() -> &'static str {
    if USE_LOCAL_SERVER {
        LOCAL_SERVER_URL
    } else {
        PROD_SERVER_URL
    }
}

// Added to detect if running in local development mode
fn is_local_mode() -> bool {
    USE_LOCAL_SERVER
}

async fn create_test_user() -> Result<Uuid, Box<dyn StdError>> {
    
    let existing_user_id = Uuid::parse_str("e1bf84be-0d14-42ec-8f1c-77918c3b9259")?;
    Ok(existing_user_id)
}

#[tokio::test]
async fn test_image_upload() -> Result<(), Box<dyn StdError>> {
    // Load environment variables from .env file
    setup_test_environment();
    
    // Create a test user in the database first
    let user_id = create_test_user().await?;
    
    // Generate an access token for the user
    let (access_token, _) = generate_test_token(user_id, "client")
        .expect("Failed to generate test token");
    
    // Use a real image file for testing
    let image_path = "me_and_millie_at_manzanita.jpeg";
    let file_bytes = tokio::fs::read(image_path).await
        .map_err(|e| Box::<dyn StdError>::from(e))?;

    println!("Read test image '{}' with size: {} bytes", image_path, file_bytes.len());

    let file_part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name("me_and_millie_at_manzanita.jpeg")
        .mime_str("image/jpeg")
        .map_err(|e| Box::<dyn StdError>::from(e))?;

    let form = reqwest::multipart::Form::new()
        .part("file", file_part);

    // Initialize the HTTP client with a longer timeout
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Box::<dyn StdError>::from(e))?;

    // Modified: Use a different endpoint for local testing
    let base_url = get_server_url();
    let upload_url = format!("{}/upload-image?image_type=profile", base_url);

    println!("Sending request to: {}", upload_url);

    // Try to ping the server first to check connectivity
    match client.get(base_url).send().await {
        Ok(ping_response) => {
            println!("Server ping status: {}", ping_response.status());
            match ping_response.text().await {
                Ok(text) => {
                    if text.len() < 1000 {
                        println!("Server ping response: {}", text);
                    } else {
                        println!("Server ping response: [too large to display]");
                    }
                },
                Err(e) => println!("Couldn't read ping response: {}", e)
            }
        },
        Err(e) => {
            println!("Server ping failed: {}", e);
        }
    }

    // Send the POST request to upload the image
    let response_result = client
        .post(&upload_url)
        .header("Authorization", format!("Bearer {}", access_token))
        .multipart(form)
        .send()
        .await;
    
    // Check if the request was successful
    let response = match response_result {
        Ok(resp) => resp,
        Err(e) => {
            println!("Request failed: {:?}", e);
            
            // Try to determine if it's a connectivity issue
            if e.is_timeout() {
                println!("The request timed out");
            } else if e.is_connect() {
                println!("Failed to connect to the server");
            } else if e.is_request() {
                println!("Error occurred while sending the request");
            } else if e.is_body() {
                println!("Error occurred while reading the response body");
            }
            
            return Err(Box::<dyn StdError>::from(e));
        }
    };
    
    // Check the response status
    let status = response.status();
    println!("Response status: {}", status);
    
    // Get the response body
    let body = match response.text().await {
        Ok(body) => body,
        Err(e) => {
            println!("Failed to read response body: {:?}", e);
            return Err(Box::<dyn StdError>::from(e));
        }
    };
    
    println!("Response body: {}", body);
    
    // Assert that the response is successful
    assert!(status.is_success(), "Request failed with status {}: {}", status, body);
    
    // Parse the response body as JSON
    let response_json: Value = serde_json::from_str(&body)
        .map_err(|e| Box::<dyn StdError>::from(e))?;
    
    // Assert that the response contains the expected fields
    assert!(response_json["message"].is_string(), "Response missing 'message' field");
    assert!(response_json["image_id"].is_string(), "Response missing 'image_id' field");
    assert!(response_json["image_url"].is_string(), "Response missing 'image_url' field");
    
    // Verify that the image_id is a valid UUID
    let image_id = response_json["image_id"].as_str().unwrap();
    assert!(Uuid::parse_str(image_id).is_ok(), "image_id is not a valid UUID");
    
    // Only verify GCS URL in production mode
    if !is_local_mode() {
        // Verify that the image_url points to the correct location
        let image_url = response_json["image_url"].as_str().unwrap();
        assert!(image_url.contains("storage.googleapis.com"), "image_url does not point to Google Cloud Storage");

        // Log the image_url if it starts with the expected prefix
        let expected_prefix = "https://storage.googleapis.com/vet-text-1/";
        if image_url.starts_with(expected_prefix) {
            println!("@{}", image_url);
        } else {
            println!("Image uploaded to unexpected URL: {}", image_url);
        }
        
        // With the new URL-encoded ACL implementation, objects should be publicly accessible
        println!("Image uploaded successfully with public access: {}", image_url);
    } else {
        println!("Skipping Google Cloud Storage URL verification in local mode");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_get_images() -> Result<(), Box<dyn StdError>> {
    // Load environment variables from .env file
    setup_test_environment();
    
    // Create a test user ID
    let user_id = Uuid::new_v4();
    
    // Generate an access token for the user
    let (access_token, _) = generate_test_token(user_id, "client")
        .expect("Failed to generate test token");
    
    // Initialize the HTTP client
    let client = Client::new();
    
    let base_url = get_server_url();
    let images_url = format!("{}/images?image_type=profile", base_url);
    
    // Send the GET request to retrieve images
    let response = client
        .get(&images_url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| Box::<dyn StdError>::from(e))?;
    
    // Check the response status
    let status = response.status();
    let body = response.text().await
        .map_err(|e| Box::<dyn StdError>::from(e))?;
    
    println!("Get images response status: {}", status);
    println!("Get images response body: {}", body);
    
    // Assert that the response is successful
    assert!(status.is_success(), "Request failed with status {}: {}", status, body);
    
    // Parse the response body as JSON
    let response_json: Value = serde_json::from_str(&body)
        .map_err(|e| Box::<dyn StdError>::from(e))?;
    
    // Assert that the response is an array (even if empty for a new user)
    assert!(response_json.is_array(), "Response is not an array");
    
    Ok(())
} 