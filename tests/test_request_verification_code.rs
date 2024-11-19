use ed25519_dalek::Signer;
use serde_json::{json, Value};
use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use uuid::Uuid;

mod testing_utils;
use testing_utils::{TEST_SIGNING_KEY, to_canonical_json};

#[tokio::test]
async fn test_request_verification_code_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    let phone_number = "0001231985";
    let timestamp = Utc::now().to_rfc3339();
    
    // Create the data payload
    let data = json!({
        "phone_number": phone_number,
        "timestamp": timestamp
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
    let res = client.post("http://localhost:8080/request-verification-code")
        .json(&payload)
        .send()
        .await?;

    let status = res.status();
    let body = res.text().await?;

    // Assert that the request was successful
    assert!(status.is_success(), "Request failed with status {}: {}", status, body);

    // Parse the response body as JSON
    let response: serde_json::Value = serde_json::from_str(&body)?;
    
    // Assert that the response contains the expected message
    assert_eq!(response["message"], "Verification code sent");

    Ok(())
}
