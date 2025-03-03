# VetText API Server

This is an API for the VetText app, written in Rust with Actix-Web.

## Dependencies
rust/cargo is required to run the server:
```{bash}
curl https://sh.rustup.rs -sSf | sh
```

## API Endpoints

### Authentication

#### POST /register
Register a new user with a phone number and public key.

Request:
```json
{
  "data": {
    "phone_number": "1234567890",
    "public_key": "base64-encoded-public-key",
    "timestamp": "1615482367000"
  },
  "signature": "base64-encoded-signature"
}
```

Response:
```json
{
  "message": "Registration successful",
  "user_id": "user-uuid"
}
```

#### POST /request-verification-code
Request a verification code to be sent to a phone number.

Request:
```json
{
  "data": {
    "phone_number": "1234567890",
    "timestamp": "1615482367000"
  },
  "signature": "base64-encoded-signature"
}
```

Response:
```json
{
  "message": "Verification code sent"
}
```

#### POST /login
Login with a verification code.

Request:
```json
{
  "data": {
    "verification_code": "123456",
    "user_id": "user-uuid",
    "timestamp": "1615482367000"
  },
  "signature": "base64-encoded-signature"
}
```

Response:
```json
{
  "access_token": "jwt-token",
  "refresh_token": "refresh-token",
  "user_id": "user-uuid"
}
```

#### POST /refresh
Refresh an access token using a refresh token.

Request:
```json
{
  "data": {
    "refresh_token": "refresh-token",
    "user_id": "user-uuid",
    "timestamp": "1615482367000"
  },
  "signature": "base64-encoded-signature"
}
```

Response:
```json
{
  "access_token": "new-jwt-token",
  "refresh_token": "new-refresh-token"
}
```

#### POST /logout
Revoke a refresh token.

Request:
```json
{
  "data": {
    "refresh_token": "refresh-token",
    "user_id": "user-uuid",
    "timestamp": "1615482367000"
  },
  "signature": "base64-encoded-signature"
}
```

Response:
```json
{
  "message": "Logged out successfully"
}
```

### User Profiles

#### GET /profiles?user_ids=id1,id2,id3
Get user profiles by IDs.

Headers:
```
Authorization: Bearer jwt-token
```

Response:
```json
[
  {
    "id": "user-uuid",
    "phone_number": "1234567890",
    "public_key": "base64-encoded-public-key",
    "scope": "client",
    "first_name": "John",
    "last_name": "Doe",
    "email": "john@example.com",
    "address": "123 Main St",
    "profile_image_url": "https://example.com/profile.jpg",
    "verified": true,
    "created_at": 1615482367000,
    "updated_at": 1615482367000
  }
]
```

#### POST /profile
Update a user profile.

Headers:
```
Authorization: Bearer jwt-token
```

Request:
```json
{
  "first_name": "John",
  "last_name": "Doe",
  "email": "john@example.com",
  "address": "123 Main St",
  "profile_image_url": "https://example.com/profile.jpg",
  "pets": [
    {
      "id": "pet-uuid",
      "name": "Fluffy",
      "breed": "Golden Retriever",
      "sex": "Male",
      "birthday": 1615482367000,
      "pet_image_url": "https://example.com/pet.jpg",
      "color": "Golden",
      "species": "Dog",
      "spayed_neutered": true,
      "weight": 70
    }
  ]
}
```

Response:
```json
{
  "user": {
    "id": "user-uuid",
    "phone_number": "1234567890",
    "public_key": "base64-encoded-public-key",
    "scope": "client",
    "first_name": "John",
    "last_name": "Doe",
    "email": "john@example.com",
    "address": "123 Main St",
    "profile_image_url": "https://example.com/profile.jpg",
    "verified": true,
    "created_at": 1615482367000,
    "updated_at": 1615482367000
  },
  "pets": [
    {
      "id": "pet-uuid",
      "user_id": "user-uuid",
      "name": "Fluffy",
      "breed": "Golden Retriever",
      "sex": "Male",
      "birthday": 1615482367000,
      "pet_image_url": "https://example.com/pet.jpg",
      "color": "Golden",
      "species": "Dog",
      "spayed_neutered": true,
      "weight": 70
    }
  ]
}
```

#### POST /delete-account
Delete a user account.

Request:
```json
{
  "data": {
    "user_id": "user-uuid",
    "timestamp": "1615482367000"
  },
  "signature": "base64-encoded-signature"
}
```

Response:
```json
{
  "message": "Account deleted successfully"
}
```

### Image Management

#### POST /upload-image?image_type=profile
Upload an image for a user profile or pet.

Headers:
```
Authorization: Bearer jwt-token
```

Query Parameters:
- `image_type`: Required. Either "profile" or "pet"

Request Body:
Multipart form data with a file field named "file"

Response:
```json
{
  "message": "Image uploaded successfully",
  "image_id": "image-uuid",
  "image_url": "https://storage.googleapis.com/bucket/path/to/image.jpg"
}
```

#### GET /images
Get all images for the authenticated user.

Headers:
```
Authorization: Bearer jwt-token
```

Query Parameters:
- `image_type`: Optional. Filter by image type ("profile" or "pet")

Response:
```json
[
  {
    "id": "image-uuid",
    "user_id": "user-uuid",
    "filename": "profile.jpg",
    "content_type": "image/jpeg",
    "image_type": "profile",
    "image_url": "https://storage.googleapis.com/bucket/path/to/image.jpg",
    "created_at": 1615482367000,
    "updated_at": 1615482367000
  }
]
```

## WebSocket API

The WebSocket API provides real-time communication for chat functionality.

### Connection

Connect to the WebSocket server at:
```
ws://your-server-domain/ws/
```

### Message Format

All WebSocket messages follow this format:

```json
{
  "sender_id": "user-uuid",
  "event": "event-name",
  "params": {
    // Event-specific parameters
  }
}
```

### Client to Server Events

#### `subscribe_conversation`

Subscribe to a conversation.

```json
{
  "sender_id": "user-uuid",
  "event": "subscribe_conversation",
  "params": {
    "conversation_id": "conversation-uuid"
  }
}
```

#### `unsubscribe_conversation`

Unsubscribe from a conversation.

```json
{
  "sender_id": "user-uuid",
  "event": "unsubscribe_conversation",
  "params": {
    "conversation_id": "conversation-uuid"
  }
}
```

#### `new_conversation`

Create a new conversation.

```json
{
  "sender_id": "user-uuid",
  "event": "new_conversation",
  "params": {
    "pet_id": "pet-uuid",
    "providers": ["provider-uuid-1", "provider-uuid-2"]
  }
}
```

#### `send_message`

Send a message to a conversation.

```json
{
  "sender_id": "user-uuid",
  "event": "send_message",
  "params": {
    "conversation_id": "conversation-uuid",
    "content": "Message content"
  }
}
```

#### `get_conversations`

Request all conversations the user is part of.

```json
{
  "sender_id": "user-uuid",
  "event": "get_conversations",
  "params": {}
}
```

#### `get_conversation_history`

Request message history for a conversation.

```json
{
  "sender_id": "user-uuid",
  "event": "get_conversation_history",
  "params": {
    "conversation_id": "conversation-uuid",
    "page": 0,
    "limit": 20
  }
}
```

### Server to Client Events

#### `subscribed`

Confirmation that the user has subscribed to a conversation.

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

#### `user_joined`

Notification that a user has joined a conversation. This event is sent to all participants in a conversation when a new user subscribes.

```json
{
  "sender_id": "00000000-0000-0000-0000-000000000000",
  "event": "user_joined",
  "params": {
    "user_id": "user-uuid",
    "display_name": "John Doe",
    "profile_image_url": "https://example.com/profile.jpg",
    "conversation_id": "conversation-uuid",
    "timestamp": 1615482367000
  }
}
```

#### `user_left`

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

#### `unsubscribed`

Confirmation that the user has unsubscribed from a conversation.

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

#### `new_message`

A new message has been sent to a conversation.

```json
{
  "sender_id": "user-uuid",
  "event": "new_message",
  "params": {
    "id": "message-uuid",
    "conversation_id": "conversation-uuid",
    "content": "Message content",
    "timestamp": 1615482367000
  }
}
```

#### `conversations_list`

Response to a `get_conversations` request.

```json
{
  "sender_id": "00000000-0000-0000-0000-000000000000",
  "event": "conversations_list",
  "params": {
    "conversations": [
      {
        "id": "conversation-uuid",
        "providers": ["provider-uuid-1", "provider-uuid-2"],
        "client": "client-uuid",
        "pet": "pet-uuid",
        "last_message": "Last message content",
        "last_updated_timestamp": 1615482367000
      }
    ]
  }
}
```

#### `conversation_history_response`

Response to a `get_conversation_history` request.

```json
{
  "sender_id": "00000000-0000-0000-0000-000000000000",
  "event": "conversation_history_response",
  "params": {
    "messages": [
      {
        "id": "message-uuid",
        "conversation_id": "conversation-uuid",
        "content": "Message content",
        "timestamp": 1615482367000
      }
    ],
    "total_count": 50,
    "has_more": true
  }
}
```

#### `error`

Error response for any failed request.

```json
{
  "sender_id": "00000000-0000-0000-0000-000000000000",
  "event": "error",
  "params": {
    "message": "Error message"
  }
}
```

## User Presence

The WebSocket API automatically handles user presence notifications:

1. When a user connects to the WebSocket server, they are automatically subscribed to all conversations they are part of.
2. When a user subscribes to a conversation (either automatically on connection or manually), all other participants receive a `user_joined` event with the user's profile information.
3. When a user unsubscribes from a conversation, all other participants receive a `user_left` event with the user's profile information.
4. This allows clients to display real-time notifications when users join or leave conversations and to show user profile information without additional API calls.

## Best Practices

1. Always handle connection errors and implement reconnection logic.
2. Subscribe to conversations as soon as the connection is established.
3. Store conversation and message IDs locally to avoid duplicate messages.
4. Use the `page` and `limit` parameters for pagination when fetching conversation history.
5. When uploading images, ensure they are in a supported format (jpg, jpeg, png, gif).
6. Use the image URLs returned from the `/upload-image` endpoint to update profile or pet images.