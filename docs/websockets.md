# WebSocket Server API Documentation

This WebSocket server allows clients to establish a persistent connection to manage conversations and messages in real-time. Below is an outline of the server's behavior, event types, and usage instructions.

## Overview
This server supports three main functionalities over WebSocket:
- **Broadcasting Messages** to all connected clients
- **Real-time Messaging** in specific conversations
- **Managing Conversations** (e.g., retrieving or creating conversations)

## WebSocket Connection

To connect to the WebSocket server, initiate a WebSocket connection to:

```
ws://yourserveraddress/ws/
```

On connection, each client is assigned a unique user ID. The server expects structured JSON messages for all interactions.

## Events

The server supports the following events:

### 1. **conversations**
   - **Purpose**: Retrieve all active conversations for the connected client.
   - **Message Format**:
     ```json
     {
       "event": "conversations",
       "data": {}
     }
     ```
   - **Response**:
     - The server will respond with a list of conversations associated with the client.

### 2. **message**
   - **Purpose**: Send a new message in an existing conversation.
   - **Message Format**:
     ```json
     {
       "event": "message",
       "data": {
         "conversation_id": "<UUID of the conversation>",
         "content": "Your message text"
       }
     }
     ```
   - **Response**:
     - If the message is successfully sent, the server responds with:
       ```json
       {
         "event": "message_sent",
         "data": {
           "conversation_id": "<UUID>",
           "content": "Your message text",
           "timestamp": "<timestamp>"
         }
       }
       ```
     - If an error occurs, the server will send an error event.

### 3. **new_conversation**
   - **Purpose**: Create a new conversation.
   - **Message Format**:
     ```json
     {
       "event": "new_conversation",
       "data": {
         "pet_id": "<UUID of the pet>",
         "providers": ["<UUID of provider 1>", "<UUID of provider 2>"]
       }
     }
     ```
   - **Response**:
     - If successful, the server responds with:
       ```json
       {
         "event": "conversation_created",
         "data": {
           "conversation_id": "<UUID>",
           "pet_id": "<UUID>",
           "providers": ["<UUID>", "<UUID>"],
           "client_id": "<UUID of the client>"
         }
       }
       ```
     - If there is an error, an error message will be returned.

## Error Handling
If any issues are encountered, such as invalid message formats or missing data, the server responds with an `error` event.

Example:
```json
{
  "event": "error",
  "data": {
    "message": "Error description here"
  }
}
```

## Example Usage

1. **Retrieve Conversations**
   ```json
   {
     "event": "conversations",
     "data": {}
   }
   ```

2. **Send a Message**
   ```json
   {
     "event": "message",
     "data": {
       "conversation_id": "123e4567-e89b-12d3-a456-426614174000",
       "content": "Hello, this is a message!"
     }
   }
   ```

3. **Create a New Conversation**
   ```json
   {
     "event": "new_conversation",
     "data": {
       "pet_id": "123e4567-e89b-12d3-a456-426614174000",
       "providers": ["123e4567-e89b-12d3-a456-426614174001", "123e4567-e89b-12d3-a456-426614174002"]
     }
   }
   ```

## Disconnection

When a client disconnects, the server automatically removes their session from the active sessions.
