-- Drop the old trigger
DROP TRIGGER IF EXISTS update_conversations_last_updated_timestamp ON conversations;

-- Create a new trigger function specifically for conversations
CREATE OR REPLACE FUNCTION update_conversations_last_updated_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.last_updated_timestamp = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Create the correct trigger for conversations
CREATE TRIGGER update_conversations_last_updated_timestamp
    BEFORE UPDATE ON conversations
    FOR EACH ROW
    EXECUTE FUNCTION update_conversations_last_updated_timestamp(); 