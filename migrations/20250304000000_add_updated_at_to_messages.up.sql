-- Add updated_at column to messages table
ALTER TABLE messages 
ADD COLUMN updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP;

-- Drop ALL existing triggers that might be interfering
DROP TRIGGER IF EXISTS update_messages_updated_at ON messages;
DROP TRIGGER IF EXISTS update_updated_at_column ON messages;
DROP TRIGGER IF EXISTS update_messages_updated_at_column ON messages;

-- Drop the old function if it exists and create our new one
DROP FUNCTION IF EXISTS update_messages_updated_at_column();

-- Create a new trigger function that handles both INSERT and UPDATE
CREATE OR REPLACE FUNCTION update_messages_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    -- For INSERT operations, the field should already have a default value
    -- For UPDATE operations, update the timestamp
    IF TG_OP = 'UPDATE' THEN
        NEW.updated_at = CURRENT_TIMESTAMP;
    END IF;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Create trigger to automatically update updated_at column
CREATE TRIGGER update_messages_updated_at
    BEFORE INSERT OR UPDATE ON messages
    FOR EACH ROW
    EXECUTE FUNCTION update_messages_updated_at_column();