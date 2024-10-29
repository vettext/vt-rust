use uuid::Uuid;
use sqlx::PgPool;
use crate::models::Conversation;
use chrono::{DateTime, Utc};
use crate::models::Message;

pub struct ConversationService;

impl ConversationService {
    pub async fn get_conversations_by_client_id(pool: &PgPool, client_id: Uuid) -> Result<Vec<Conversation>, sqlx::Error> {
        sqlx::query_as!(
            Conversation,
            "
            SELECT id, providers, client, pet, last_message, last_updated_timestamp
            FROM conversations
            WHERE client = $1
            ORDER BY last_updated_timestamp DESC
            ",
            client_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn get_conversations_by_provider_id(pool: &PgPool, provider_id: Uuid) -> Result<Vec<Conversation>, sqlx::Error> {
        sqlx::query_as!(
            Conversation,
            "
            SELECT id, providers, client, pet, last_message, last_updated_timestamp
            FROM conversations
            WHERE $1 = ANY(providers)
            ORDER BY last_updated_timestamp DESC
            ",
            provider_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn create_conversation(pool: &PgPool, providers: Vec<Uuid>, client: Uuid, pet: Uuid) -> Result<Conversation, sqlx::Error> {
        sqlx::query_as!(
            Conversation,
            "
            INSERT INTO conversations (providers, client, pet, last_message, last_updated_timestamp)
            VALUES ($1, $2, $3, '', CURRENT_TIMESTAMP)
            RETURNING id, providers, client, pet, last_message, last_updated_timestamp
            ",
            &providers,
            client,
            pet
        )
        .fetch_one(pool)
        .await
    }

    pub async fn send_message(
        pool: &PgPool,
        sender_id: Uuid,
        conversation_id: Uuid,
        content: String,
        timestamp: DateTime<Utc>
    ) -> Result<Message, sqlx::Error> {
        // First insert the message
        let message = sqlx::query_as!(
            Message,
            r#"
            INSERT INTO messages (conversation_id, sender_id, content, timestamp)
            VALUES ($1, $2, $3, $4)
            RETURNING id, conversation_id, content, timestamp
            "#,
            conversation_id,
            sender_id,
            content,
            timestamp
        )
        .fetch_one(pool)
        .await?;

        // Update the conversation's last_message and last_updated_timestamp
        sqlx::query!(
            r#"
            UPDATE conversations
            SET last_message = $1,
                last_updated_timestamp = $2
            WHERE id = $3
            "#,
            content,
            timestamp,
            conversation_id
        )
        .execute(pool)
        .await?;

        Ok(message)
    }
}

