-- Drop trigger
DROP TRIGGER IF EXISTS update_messages_updated_at ON messages;

-- Remove updated_at column from messages table
ALTER TABLE messages 
DROP COLUMN IF EXISTS updated_at; 