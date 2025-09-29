use uuid::Uuid;
use sqlx::PgPool;
use crate::models::Conversation;
use chrono::{DateTime, Utc};
use crate::models::Message;
use anyhow::Result;

pub struct ConversationService;

impl ConversationService {
    pub async fn get_conversations_by_client_id(pool: &PgPool, client_id: Uuid) -> Result<Vec<Conversation>> {
        let result = sqlx::query_as!(
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
        .await;

        match result {
            Ok(conversations) => Ok(conversations),
            Err(e) => {
                eprintln!("Database error: {:?}", e);
                Err(anyhow::anyhow!("Failed to fetch conversations: {}", e))
            }
        }
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
            INSERT INTO messages (conversation_id, sender_id, content, timestamp, updated_at)
            VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP)
            RETURNING id, conversation_id, sender_id, content, timestamp, updated_at
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

    pub async fn get_conversation_messages(
        pool: &PgPool, 
        conversation_id: Uuid, 
        page: i32, 
        limit: i32
    ) -> Result<(Vec<Message>, i32, bool), sqlx::Error> {
        // Validate input parameters
        if page < 1 {
            return Err(sqlx::Error::Protocol("Invalid page number: must be >= 1".to_string()));
        }
        if limit < 1 || limit > 100 {
            return Err(sqlx::Error::Protocol("Invalid limit: must be between 1 and 100".to_string()));
        }
        
        // Calculate offset - FIX: Use (page - 1) * limit for 1-based pagination
        let offset = (page - 1) * limit;
        
        // Debug logging
        println!("Fetching conversation history: conversation_id={}, page={}, limit={}, offset={}", 
                 conversation_id, page, limit, offset);
        
        // Get total count
        let total_count = sqlx::query!(
            "SELECT COUNT(*) as count FROM messages WHERE conversation_id = $1",
            conversation_id
        )
        .fetch_one(pool)
        .await?
        .count
        .unwrap_or(0) as i32;
        
        // Get messages with pagination
        let messages = sqlx::query_as!(
            Message,
            "SELECT id, conversation_id, sender_id, content, timestamp, updated_at
             FROM messages 
             WHERE conversation_id = $1 
             ORDER BY timestamp DESC 
             LIMIT $2 OFFSET $3",
            conversation_id,
            limit as i64,
            offset as i64
        )
        .fetch_all(pool)
        .await?;
        
        // Calculate if there are more messages
        let has_more = (offset + limit) < total_count;
        
        Ok((messages, total_count, has_more))
    }
}

