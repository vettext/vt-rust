use ed25519_dalek::Signer;
use serde_json::json;
use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use uuid::Uuid;

mod testing_utils;
use testing_utils::{TEST_SIGNING_KEY, TEST_VERIFYING_KEY, to_canonical_json};

#[tokio::test]
async fn test_register_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    // Prepare the data payload
    let public_key = general_purpose::STANDARD.encode(TEST_VERIFYING_KEY.as_bytes());
    let timestamp = Utc::now().to_rfc3339();
    let phone_number = "0001231985";

    // Create the data payload
    let data = json!({
        "phone_number": phone_number,
        "public_key": public_key,
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
    let res = client
        .post("http://localhost:8080/register")
        .json(&payload)
        .send()
        .await?;
    let status = res.status();
    let body = res.text().await?;

    assert!(status.is_success(), "Request failed with status {}: {}", status, body);

    let response: serde_json::Value = serde_json::from_str(&body)?;
    
    assert!(Uuid::parse_str(response["user_id"].as_str().unwrap()).is_ok(), "Response doesn't contain a valid user_id");
    assert_eq!(response["message"], "Test registration data received and verified. Test verification code is 123456.");

    Ok(())
}