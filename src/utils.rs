use reqwest::Client as ReqwestClient;
use chrono::{DateTime, Utc, Duration};
use jsonwebtoken::{encode, decode, EncodingKey, Header, Algorithm, DecodingKey, Validation};
use base64::{Engine as _, engine::general_purpose};
use serde::{Serialize, Deserialize};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::Aead;
use std::env;
use aes_gcm::KeyInit;
use rand::{thread_rng, Rng};
use uuid::Uuid;
use ed25519_dalek::{VerifyingKey, Signature};
use serde_json::Value;
use anyhow;
use actix_web::HttpRequest;
use std::collections::BTreeMap;

pub async fn send_verification_request(phone_number: &str) -> Result<(), Box<dyn std::error::Error>> {
    let account_sid = std::env::var("TWILIO_ACCOUNT_SID")?;
    let auth_token = std::env::var("TWILIO_AUTH_TOKEN")?;
    let service_sid = std::env::var("TWILIO_SERVICE_SID")?;

    let client = ReqwestClient::new();
    let url = format!("https://verify.twilio.com/v2/Services/{}/Verifications", service_sid);

    let response = client.post(&url)
        .basic_auth(&account_sid, Some(&auth_token))
        .form(&[
            ("To", format!("+1{}", phone_number)),
            ("Channel", "sms".to_string())
        ])
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Failed to send verification: {:?}", response.text().await?).into())
    }
}

pub async fn check_verification_code(phone_number: &str, code: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let account_sid = std::env::var("TWILIO_ACCOUNT_SID")?;
    let auth_token = std::env::var("TWILIO_AUTH_TOKEN")?;
    let service_sid = std::env::var("TWILIO_SERVICE_SID")?;

    let client = ReqwestClient::new();
    let url = format!("https://verify.twilio.com/v2/Services/{}/VerificationCheck", service_sid);

    let response = client.post(&url)
        .basic_auth(&account_sid, Some(&auth_token))
        .form(&[
            ("To", format!("+1{}", phone_number)),
            ("Code", code.to_string())
        ])
        .send()
        .await?;

    if response.status().is_success() {
        let body: serde_json::Value = response.json().await?;
        Ok(body["status"] == "approved")
    } else {
        Err(format!("Failed to check verification: {:?}", response.text().await?).into())
    }
}

pub fn is_timestamp_valid(timestamp: &str) -> bool {
    let now = Utc::now();
    match DateTime::parse_from_rfc3339(timestamp) {
        Ok(request_time) => {
            let time_diff = now.signed_duration_since(request_time);
            time_diff > Duration::seconds(-5) && time_diff < Duration::minutes(1)
        },
        Err(_) => false,
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,  // user id
    pub iss: String,  // issuer
    pub aud: String,  // audience
    pub exp: usize,   // expiration time
    pub iat: usize,   // issued at
    pub scope: String, // user scope (client or provider)
}

impl Claims {
    pub fn get_scope(&self) -> &str {
        &self.scope
    }

    pub fn get_sub(&self) -> &str {
        &self.sub
    }
}

pub fn generate_signed_encrypted_token(user_id: Uuid, user_scope: &str) -> Result<(String, usize), Box<dyn std::error::Error>> {
    // Load keys from environment variables
    let jwt_private_key_pem_base64 = env::var("JWT_PRIVATE_KEY")
        .map_err(|e| format!("Failed to get JWT_PRIVATE_KEY from env: {}", e))?;
    let encryption_key_base64 = env::var("ENCRYPTION_KEY")
        .map_err(|e| format!("Failed to get ENCRYPTION_KEY from env: {}", e))?;

    // Base64 decode the PEM key
    let jwt_private_key_pem_bytes = general_purpose::STANDARD.decode(&jwt_private_key_pem_base64)
        .map_err(|e| format!("Failed to base64 decode JWT_PRIVATE_KEY: {}", e))?;

    let jwt_private_key_pem = String::from_utf8(jwt_private_key_pem_bytes)
        .map_err(|e| format!("Failed to convert JWT_PRIVATE_KEY to string: {}", e))?;

    // Base64 decode the encryption key
    let encryption_key_bytes = general_purpose::STANDARD.decode(&encryption_key_base64)
        .map_err(|e| format!("Failed to base64 decode ENCRYPTION_KEY: {}", e))?;

    // Define expiration time
    let expiration = (Utc::now() + Duration::days(1)).timestamp() as usize;

    // Create the claims
    let claims = Claims {
        sub: user_id.to_string(),
        iss: "VeterinaryText".to_string(),
        aud: "VeterinaryText".to_string(),
        exp: expiration,
        iat: Utc::now().timestamp() as usize,
        scope: user_scope.to_string(),
    };

    // Sign the JWT
    let header = Header::new(Algorithm::ES256);
    let encoding_key = EncodingKey::from_ec_pem(jwt_private_key_pem.as_bytes())
        .map_err(|e| format!("Failed to create encoding key from JWT_PRIVATE_KEY: {}", e))?;
    let token = encode(&header, &claims, &encoding_key)
        .map_err(|e| format!("Failed to encode JWT: {}", e))?;

    // Encrypt the signed token
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&encryption_key_bytes));
    let nonce = Nonce::from_slice(&[0u8; 12]); // For testing, fixed nonce is acceptable
    let ciphertext = cipher.encrypt(nonce, token.as_bytes())
        .map_err(|e| format!("Encryption error: {:?}", e))?;

    // Base64 encode the encrypted token and return with expiration
    Ok((general_purpose::URL_SAFE_NO_PAD.encode(ciphertext), expiration))
}

pub fn generate_refresh_token() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789";
    const TOKEN_LENGTH: usize = 64;

    let mut rng = thread_rng();

    (0..TOKEN_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

pub fn verify_signature<T: Serialize>(
    data: &T,
    signature: &str,
    public_key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Convert data to serde_json::Value
    let data_value = serde_json::to_value(data)?;

    // Serialize the data with sorted keys
    let stringified_data = to_canonical_json(&data_value);
    
    // Debug logging
    println!("DEBUG: Canonical JSON: {}", stringified_data);
    println!("DEBUG: Signature: {}", signature);
    println!("DEBUG: Public key: {}", public_key);

    // Decode the base64 signature
    let signature_bytes = base64::engine::general_purpose::STANDARD.decode(signature)?;

    // Create Signature from signature bytes
    let signature = Signature::from_slice(&signature_bytes)?;

    // Decode the public key
    let public_key_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD.decode(public_key)?
        .try_into()
        .map_err(|_| "Invalid public key length")?;

    // Create VerifyingKey from public key bytes
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)?;

    // Verify the signature
    verifying_key.verify_strict(stringified_data.as_bytes(), &signature)?;

    Ok(())
}

pub fn to_canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut btree_map = BTreeMap::new();
            for (k, v) in map {
                btree_map.insert(k.clone(), v.clone());
            }
            serde_json::to_string(&btree_map).unwrap()
        }
        Value::Array(arr) => {
            let serialized_arr: Vec<Value> = arr.iter().cloned().collect();
            serde_json::to_string(&serialized_arr).unwrap()
        }
        _ => serde_json::to_string(value).unwrap(),
    }
}

pub fn verify_and_decode_token(
    encrypted_token: &str,
) -> Result<Claims, Box<dyn std::error::Error>> {
    // Load keys from environment variables
    let jwt_public_key_pem_base64 = env::var("JWT_PUBLIC_KEY")
        .map_err(|e| format!("Failed to get JWT_PUBLIC_KEY from env: {}", e))?;
    let encryption_key_base64 = env::var("ENCRYPTION_KEY")
        .map_err(|e| format!("Failed to get ENCRYPTION_KEY from env: {}", e))?;

    // Base64 decode the PEM key
    let jwt_public_key_pem_bytes = general_purpose::STANDARD.decode(&jwt_public_key_pem_base64)
        .map_err(|e| format!("Failed to base64 decode JWT_PUBLIC_KEY: {}", e))?;

    let jwt_public_key_pem = String::from_utf8(jwt_public_key_pem_bytes)
        .map_err(|e| format!("Failed to convert JWT_PUBLIC_KEY to string: {}", e))?;

    // Base64 decode the encryption key
    let encryption_key_bytes = general_purpose::STANDARD.decode(&encryption_key_base64)
        .map_err(|e| format!("Failed to base64 decode ENCRYPTION_KEY: {}", e))?;

    // Base64 decode the encrypted token
    let ciphertext = general_purpose::URL_SAFE_NO_PAD.decode(encrypted_token)?;

    // Decrypt the token
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&encryption_key_bytes));
    let nonce = Nonce::from_slice(&[0u8; 12]); // Use the same fixed nonce as in encryption
    let token = cipher.decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| format!("Decryption error: {:?}", e))?;
    let token = String::from_utf8(token)?;

    // Decode and verify the JWT
    let decoding_key = DecodingKey::from_ec_pem(jwt_public_key_pem.as_bytes())?;
    let validation = Validation::new(Algorithm::ES256);
    let token_data = decode::<Claims>(&token, &decoding_key, &validation)?;

    Ok(token_data.claims)
}

pub fn extract_user_id_from_token(req: &HttpRequest) -> Result<Uuid, anyhow::Error> {
    // Extract the token from the Authorization header
    let token = match req.headers().get("Authorization") {
        Some(value) => {
            let parts: Vec<&str> = value.to_str().unwrap_or("").split_whitespace().collect();
            if parts.len() == 2 && parts[0] == "Bearer" {
                parts[1]
            } else {
                return Err(anyhow::anyhow!("Invalid Authorization header"));
            }
        }
        None => return Err(anyhow::anyhow!("Missing Authorization header")),
    };

    // Verify and decode the token
    let claims = match verify_and_decode_token(token) {
        Ok(claims) => claims,
        Err(e) => return Err(anyhow::anyhow!("Token verification failed: {}", e)),
    };
    
    // Extract the user_id from the token
    let user_id = Uuid::parse_str(claims.get_sub())
        .map_err(|_| anyhow::anyhow!("Invalid user ID in token"))?;
    
    Ok(user_id)
}
