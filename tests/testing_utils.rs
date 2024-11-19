use ed25519_dalek::{SigningKey, VerifyingKey};
use once_cell::sync::Lazy;
use std::collections::BTreeMap;
use serde_json::Value;
use uuid::Uuid;
use chrono::{DateTime, Utc, Duration};
use std::env;
use base64::engine::general_purpose;
use base64::Engine as _;
use jsonwebtoken::{encode, Header, Algorithm, EncodingKey};
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use aes_gcm::aead::{Aead, Payload};
use serde::{Serialize, Deserialize};

pub static TEST_SIGNING_KEY: Lazy<SigningKey> = Lazy::new(|| {
    // This is a hard-coded private key for testing purposes only.
    // Never use this in production!
    let secret_key_bytes = [
        157, 97, 177, 157, 239, 253, 90, 96,
        186, 132, 74, 244, 146, 236, 44, 196,
        68, 73, 197, 105, 123, 50, 105, 25,
        112, 59, 172, 3, 28, 174, 127, 96,
    ];
    SigningKey::from_bytes(&secret_key_bytes)
});

pub static TEST_VERIFYING_KEY: Lazy<VerifyingKey> = Lazy::new(|| TEST_SIGNING_KEY.verifying_key());

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

pub fn to_canonical_json(value: &Value) -> String {
    match value {
        Value::String(s) => {
            s.clone()
        }
        Value::Object(map) => {
            let mut btree_map = BTreeMap::new();
            for (k, v) in map {
                btree_map.insert(k, to_canonical_json(v));
            }
            let serialized = serde_json::to_string(&btree_map).unwrap();
            serialized
        }
        Value::Array(arr) => {
            let serialized_arr: Vec<String> = arr.iter().map(|v| to_canonical_json(v)).collect();
            serde_json::to_string(&serialized_arr).unwrap()
        }
        _ => serde_json::to_string(value).unwrap(),
    }
}

pub fn generate_test_token(user_id: Uuid, user_scope: &str) -> Result<String, Box<dyn std::error::Error>> {
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

    // Create the claims
    let claims = Claims {
        sub: user_id.to_string(),
        iss: "VeterinaryText".to_string(),
        aud: "VeterinaryText".to_string(),
        exp: (Utc::now() + Duration::days(1)).timestamp() as usize,
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

    // Base64 encode the encrypted token
    Ok(general_purpose::URL_SAFE_NO_PAD.encode(ciphertext))
}
