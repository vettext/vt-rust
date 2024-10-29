use ed25519_dalek::{SigningKey, VerifyingKey};
use once_cell::sync::Lazy;
use std::collections::BTreeMap;
use serde_json::Value;

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

pub fn to_canonical_json(value: &Value) -> String {
    match value {
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