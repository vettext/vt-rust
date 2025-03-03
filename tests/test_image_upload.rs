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
const PROD_SERVER_URL: &str = "http://34.83.125.159:8080";

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

#[tokio::test]
async fn test_image_upload() -> Result<(), Box<dyn StdError>> {
    // Load environment variables from .env file
    setup_test_environment();
    
    // Create a test user ID
    let user_id = Uuid::new_v4();
    
    // Generate an access token for the user
    let (access_token, _) = generate_test_token(user_id, "client")
        .expect("Failed to generate test token");
    
    // Create a temporary test image - make it even smaller
    let image_path = "test_image.jpg";
    let mut file = File::create(image_path).await
        .map_err(|e| Box::<dyn StdError>::from(e))?;
    
    // Create an extremely minimal JPEG image
    let test_image_data = [
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
        0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x10, 0x0B, 0x0C, 0x0E, 0x0C, 0x0A, 0x10,
        0x0E, 0x0D, 0x0E, 0x12, 0x11, 0x10, 0x13, 0x18, 0x28, 0x1A, 0x18, 0x16, 0x16, 0x18, 0x31, 0x23,
        0x25, 0x1D, 0x28, 0x3A, 0x33, 0x3D, 0x3C, 0x39, 0x33, 0x38, 0x37, 0x40, 0x48, 0x5C, 0x4E, 0x40,
        0x44, 0x57, 0x45, 0x37, 0x38, 0x50, 0x6D, 0x51, 0x57, 0x5F, 0x62, 0x67, 0x68, 0x67, 0x3E, 0x4D,
        0x71, 0x79, 0x70, 0x64, 0x78, 0x5C, 0x65, 0x67, 0x63, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01,
        0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xC4, 0x00, 0x14,
        0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x00, 0xFF, 0xD9
    ];
    
    file.write_all(&test_image_data).await
        .map_err(|e| Box::<dyn StdError>::from(e))?;
    
    file.flush().await
        .map_err(|e| Box::<dyn StdError>::from(e))?;
    
    println!("Created test image with size: {} bytes", test_image_data.len());
    
    // Create a multipart form using reqwest's multipart
    let file_bytes = tokio::fs::read(image_path).await
        .map_err(|e| Box::<dyn StdError>::from(e))?;
    
    let file_part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name("test_image.jpg")
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
    
    // In local mode, we might need a different endpoint structure
    // For this test, we'll use the same path but acknowledge that the server
    // might not have the same functionality in local mode
    let upload_url = format!("{}/upload-image?image_type=profile", base_url);
    
    println!("Sending request to: {}", upload_url);
    
    // Clean up the test image before sending the request
    // This ensures it gets cleaned up even if the request fails
    tokio::fs::remove_file(image_path).await
        .map_err(|e| Box::<dyn StdError>::from(e))?;
    
    // Conditionally skip the test if we detect the GCS authentication issue
    if is_local_mode() {
        println!("Running in local mode - Google Cloud Storage authentication may not be configured");
        println!("This test might be skipped or adjusted for local development");
        
        // You can choose to return Ok(()) here to skip the test in local mode:
        // return Ok(());
    }
    
    // Try to ping the server first to check connectivity
    match client.get(base_url).send().await {
        Ok(ping_response) => {
            println!("Server ping status: {}", ping_response.status());
            
            // Print the response body if it's not too large
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
            
            // If we get a 404 from the root path, that's actually expected
            // Most APIs don't implement anything at the root path
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