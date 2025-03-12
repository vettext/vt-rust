# WebSocket Server API Documentation

This WebSocket server allows clients to establish a persistent connection to manage conversations and messages in real-time. Below is an outline of the server's behavior, event types, and usage instructions.

## Overview

The WebSocket server supports:
- **Role-based access control** for conversations
- **Real-time messaging** in specific conversations
- **Conversation management** (creating, retrieving)
- **Targeted message delivery** to conversation participants only

## WebSocket Connection

To connect to the WebSocket server, initiate a WebSocket connection to:

```
ws://yourserveraddress/ws/
```

On connection, each client is assigned a unique user ID. The server expects structured JSON messages for all interactions.

## Message Format

All messages follow this format:

```json
{
  "sender_id": "uuid-of-sender",
  "event": "event-name",
  "params": { /* event-specific data */ }
}
```

## Role-Based Access

The system enforces role-based access control:
- **Clients** (pet owners) can create conversations and see only their own conversations
- **Providers** (veterinarians/service providers) can only see conversations they've been invited to
- Users can only send messages in conversations they're part of

## Events

### 1. **conversations**
   - **Purpose**: Retrieve all active conversations for the connected user based on their role.
   - **Message Format**:
     ```json
     {
       "sender_id": "user-uuid",
       "event": "conversations",
       "params": {}
     }
     ```
   - **Response**:
     - For clients: List of conversations they created
     - For providers: List of conversations they've been invited to
     ```json
     {
       "sender_id": "00000000-0000-0000-0000-000000000000",
       "event": "conversations",
       "params": [
         {
           "id": "conversation-uuid",
           "providers": ["provider-uuid-1", "provider-uuid-2"],
           "client": "client-uuid",
           "pet": "pet-uuid",
           "last_message": "Last message content",
           "last_updated_timestamp": 1672574400000
         }
       ]
     }
     ```

### 2. **message**
   - **Purpose**: Send a new message in an existing conversation.
   - **Access**: Only users who are part of the conversation can send messages
   - **Message Format**:
     ```json
     {
       "sender_id": "user-uuid",
       "event": "message",
       "params": {
         "conversation_id": "conversation-uuid",
         "content": "Your message text"
       }
     }
     ```
   - **Response**:
     - If successful, all conversation participants receive:
       ```json
       {
         "sender_id": "00000000-0000-0000-0000-000000000000",
         "event": "message_sent",
         "params": {
           "id": "message-uuid",
           "conversation_id": "conversation-uuid",
           "sender_id": "user-uuid",
           "content": "Your message text",
           "timestamp": 1672574400000
         }
       }
       ```
     - If unauthorized or error occurs, sender receives an error event.

### 3. **new_conversation**
   - **Purpose**: Create a new conversation.
   - **Access**: Only clients can create conversations
   - **Message Format**:
     ```json
     {
       "sender_id": "client-uuid",
       "event": "new_conversation",
       "params": {
         "pet_id": "pet-uuid",
         "providers": ["provider-uuid-1", "provider-uuid-2"]
       }
     }
     ```
   - **Response**:
     - Client receives:
       ```json
       {
         "sender_id": "00000000-0000-0000-0000-000000000000",
         "event": "conversation_created",
         "params": {
           "id": "conversation-uuid",
           "providers": ["provider-uuid-1", "provider-uuid-2"],
           "client": "client-uuid",
           "pet": "pet-uuid",
           "last_message": "",
           "last_updated_timestamp": 1672574400000
         }
       }
       ```
     - Providers receive:
       ```json
       {
         "sender_id": "00000000-0000-0000-0000-000000000000",
         "event": "new_conversation_invitation",
         "params": {
           "id": "conversation-uuid",
           "providers": ["provider-uuid-1", "provider-uuid-2"],
           "client": "client-uuid",
           "pet": "pet-uuid",
           "last_message": "",
           "last_updated_timestamp": 1672574400000
         }
       }
       ```

### 4. **conversation_history**
   - **Purpose**: Retrieve message history for a conversation.
   - **Access**: Only users who are part of the conversation can access history
   - **Message Format**:
     ```json
     {
       "sender_id": "user-uuid",
       "event": "conversation_history",
       "params": {
         "conversation_id": "conversation-uuid",
         "page": 0,
         "limit": 20
       }
     }
     ```
   - **Response**:
     ```json
     {
       "sender_id": "00000000-0000-0000-0000-000000000000",
       "event": "conversation_history_response",
       "params": {
         "messages": [
           {
             "id": "message-uuid",
             "conversation_id": "conversation-uuid",
             "sender_id": "user-uuid",
             "content": "Message content",
             "timestamp": 1672574400000
           }
         ],
         "total_count": 45,
         "has_more": true
       }
     }
     ```

### 5. **subscribe_conversation**
   - **Purpose**: Explicitly subscribe to a conversation's updates.
   - **Access**: Only users who are part of the conversation can subscribe
   - **Message Format**:
     ```json
     {
       "sender_id": "user-uuid",
       "event": "subscribe_conversation",
       "params": {
         "conversation_id": "conversation-uuid"
       }
     }
     ```
   - **Response**:
     ```json
     {
       "sender_id": "00000000-0000-0000-0000-000000000000",
       "event": "subscribed",
       "params": {
         "conversation_id": "conversation-uuid",
         "status": "success"
       }
     }
     ```

### 6. **unsubscribe_conversation**
   - **Purpose**: Unsubscribe from a conversation's updates.
   - **Message Format**:
     ```json
     {
       "sender_id": "user-uuid",
       "event": "unsubscribe_conversation",
       "params": {
         "conversation_id": "conversation-uuid",
         "status": "success"
       }
     }
     ```
   - **Response**:
     ```json
     {
       "sender_id": "00000000-0000-0000-0000-000000000000",
       "event": "unsubscribed",
       "params": {
         "conversation_id": "conversation-uuid",
         "status": "success"
       }
     }
     ```

### 7. **user_left**

Notification that a user has left a conversation. This event is sent to all participants in a conversation when a user unsubscribes.

```json
{
  "sender_id": "00000000-0000-0000-0000-000000000000",
  "event": "user_left",
  "params": {
    "user_id": "user-uuid",
    "display_name": "John Doe",
    "profile_image_url": "https://example.com/profile.jpg",
    "conversation_id": "conversation-uuid",
    "timestamp": 1615482367000,
    "reason": "unsubscribed"
  }
}
```

## Error Handling

If any issues are encountered, such as unauthorized access, invalid message formats, or server errors, the server responds with an `error` event:

```json
{
  "sender_id": "00000000-0000-0000-0000-000000000000",
  "event": "error",
  "params": {
    "message": "Error description here"
  }
}
```

## Automatic Subscriptions

Users are automatically subscribed to:
1. All conversations they are part of when they connect (based on their role)
2. Any conversation they send a message to
3. Any conversation they request history for
4. Any new conversation they create or are invited to

## Conversation-Specific Broadcasting

Messages are only broadcast to users who are subscribed to the relevant conversation, ensuring privacy and reducing unnecessary network traffic.

## Disconnection

When a client disconnects, the server automatically:
1. Removes their session from active sessions
2. Removes them from all conversation subscriptions
3. Cleans up any empty conversation subscriptions

## User Presence

The WebSocket API automatically handles user presence notifications:

1. When a user connects to the WebSocket server, they are automatically subscribed to all conversations they are part of.
2. When a user subscribes to a conversation (either automatically on connection or manually), all other participants receive a `user_joined` event with the user's profile information.
3. When a user unsubscribes from a conversation, all other participants receive a `user_left` event with the user's profile information.
4. This allows clients to display real-time notifications when users join or leave conversations and to show user profile information without additional API calls.
