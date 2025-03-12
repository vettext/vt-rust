use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(FromRow, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub phone_number: String,
    pub public_key: String,
    pub scope: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub profile_image_url: Option<String>,
    pub verified: bool,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub updated_at: DateTime<Utc>,
}

#[derive(FromRow, Debug, Serialize, Deserialize)]
pub struct Pet {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub breed: String,
    pub sex: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub birthday: DateTime<Utc>,
    pub pet_image_url: Option<String>,
    pub color: Option<String>,
    pub species: Option<String>,
    pub spayed_neutered: Option<bool>,
    pub weight: Option<i32>,
}

#[derive(FromRow, Debug)]
pub struct RefreshToken {
    pub token: String,
    pub user_id: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
    pub last_used_at: Option<DateTime<Utc>>,
    pub user_agent: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SignedData<T> {
    pub data: T,
    pub signature: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RegisterData {
    pub phone_number: String,
    pub public_key: String,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RequestVerificationCodeData {
    pub phone_number: String,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LoginData {
    pub verification_code: String,
    pub user_id: Uuid,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RefreshData {
    pub refresh_token: String,
    pub user_id: Uuid,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LogoutData {
    pub refresh_token: String,
    pub user_id: Uuid,
    pub timestamp: String,
}

#[derive(Deserialize)]
pub struct UpdateProfileData {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub profile_image_url: Option<String>,
    pub pets: Vec<PetData>,
}

#[derive(Deserialize)]
pub struct PetData {
    pub id: Option<Uuid>,
    pub name: Option<String>,
    pub breed: Option<String>,
    pub sex: Option<String>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub birthday: Option<DateTime<Utc>>,
    pub pet_image_url: Option<String>,
    pub color: Option<String>,
    pub species: Option<String>,
    pub spayed_neutered: Option<bool>,
    pub weight: Option<i32>,
}

#[derive(serde::Deserialize)]
pub struct ProfilesQuery {
    pub user_ids: String,
}

// Define a WebSocket message structure
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WsMessage {
    pub sender_id: Uuid,
    pub event: String,
    pub params: serde_json::Value,
}

#[derive(FromRow, Debug, Serialize, Deserialize, Clone)]
pub struct Conversation {
    pub id: Uuid,
    pub providers: Vec<Uuid>,
    pub client: Uuid,
    pub pet: Uuid,
    pub last_message: Option<String>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub last_updated_timestamp: DateTime<Utc>,
}

#[derive(FromRow, Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "event", content = "data")]
pub enum WsEvent {
    Conversations,
    Message {
        conversation_id: Uuid,
        content: String,
    },
    NewConversation {
        pet_id: Uuid,
        providers: Option<Vec<Uuid>>,
    },
    ConversationHistory {
        conversation_id: Uuid,
        page: i32,
        limit: i32,
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConversationHistoryResponse {
    pub messages: Vec<Message>,
    pub total_count: i32,
    pub has_more: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeleteUserData {
    pub user_id: Uuid,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UploadImageData {
    pub user_id: Uuid,
    pub timestamp: String,
    pub image_type: String, // "profile" or "pet"
}

#[derive(FromRow, Debug, Serialize, Deserialize)]
pub struct Image {
    pub id: Uuid,
    pub user_id: Uuid,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub image_type: String, // "profile" or "pet"
    pub image_url: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct GetImagesQuery {
    pub image_type: Option<String>,
}

#[derive(Deserialize)]
pub struct UploadImageQuery {
    pub image_type: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdatePetData {
    pub id: Option<Uuid>,
    pub name: Option<String>,
    pub breed: Option<String>,
    pub sex: Option<String>,
    pub birthday: Option<DateTime<Utc>>,
    pub pet_image_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DeletePetData {
    pub id: Uuid,  // Required to identify which pet to delete
}
