-- Drop triggers
DROP TRIGGER IF EXISTS update_conversations_last_updated_timestamp ON conversations;
DROP TRIGGER IF EXISTS update_pets_updated_at ON pets;
DROP TRIGGER IF EXISTS update_users_updated_at ON users;

-- Drop indexes
DROP INDEX IF EXISTS idx_conversations_providers;
DROP INDEX IF EXISTS idx_conversations_pet;
DROP INDEX IF EXISTS idx_conversations_client;
DROP INDEX IF EXISTS idx_refresh_tokens_user_id;
DROP INDEX IF EXISTS idx_pets_user_id;
DROP INDEX IF EXISTS idx_messages_conversation_id;
DROP INDEX IF EXISTS idx_messages_sender_id;

-- Drop tables (in reverse order of creation)
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS conversations;
DROP TABLE IF EXISTS refresh_tokens;
DROP TABLE IF EXISTS pets;
DROP TABLE IF EXISTS users;

-- Drop constraints
ALTER TABLE IF EXISTS users DROP CONSTRAINT IF EXISTS check_valid_email;
ALTER TABLE IF EXISTS users DROP CONSTRAINT IF EXISTS check_valid_phone;

-- Drop functions
DROP FUNCTION IF EXISTS update_updated_at_column();
