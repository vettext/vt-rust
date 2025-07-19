-- Drop the new trigger
DROP TRIGGER IF EXISTS update_conversations_last_updated_timestamp ON conversations;

-- Drop the new function
DROP FUNCTION IF EXISTS update_conversations_last_updated_timestamp();

-- Recreate the old trigger
CREATE TRIGGER update_conversations_last_updated_timestamp
    BEFORE UPDATE ON conversations
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column(); 