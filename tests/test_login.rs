use ed25519_dalek::Signer;
use serde_json::{json, Value};
use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use uuid::Uuid;

mod testing_utils;
use testing_utils::{TEST_SIGNING_KEY, to_canonical_json};

#[tokio::test]
async fn test_login_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    // Prepare the data payload
    let user_id = Uuid::parse_str("e1bf84be-0d14-42ec-8f1c-77918c3b9259").unwrap();
    let timestamp = Utc::now().to_rfc3339();
    let verification_code = "649985";

    // Create the data payload
    let data = json!({
        "user_id": user_id.to_string(),
        "timestamp": timestamp,
        "verification_code": verification_code
    });

    // Convert the data to a Value
    let data_value = serde_json::to_value(&data)?;

    // Serialize the data with sorted keys
    let stringified_data = to_canonical_json(&data_value);

    // Sign the stringified data
    let signature = TEST_SIGNING_KEY.sign(stringified_data.as_bytes());

    // Prepare the full payload
    let payload = json!({
        "data": data,
        "signature": general_purpose::STANDARD.encode(signature.to_bytes())
    });

    // Send the request
    let client = reqwest::Client::new();
    let res = client.post("http://localhost:8080/login")
        .json(&payload)
        .send()
        .await?;

    let status = res.status();
    let body = res.text().await?;

    // Assert the response status is successful
    assert!(status.is_success(), "Request failed with status {}: {}", status, body);

    // Parse the response body as JSON
    let response: serde_json::Value = serde_json::from_str(&body)?;

    // Assert that the response contains the expected values
    assert_eq!(response["message"], "Login successful");
    assert_eq!(response["user_id"], user_id.to_string());
    assert!(response["access_token"].is_string(), "Access token not found in response");
    assert!(response["refresh_token"].is_string(), "Refresh token not found in response");
    assert!(response["expires_at"].is_number(), "Expiration time not found in response");
    assert!(response["expires_at"].as_u64().unwrap() > Utc::now().timestamp() as u64, 
           "Expiration time should be in the future");

    Ok(())
}
