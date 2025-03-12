# VetText API Documentation

This document describes the REST API endpoints available in the VetText API.

## Authentication

### POST /register
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

### POST /request-verification-code
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

### POST /login
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

### POST /refresh
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

### POST /logout
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

## User Management

### GET /profiles?user_ids=id1,id2,id3
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
    "email": "john.doe@example.com",
    "address": "123 Main St, Anytown, USA",
    "profile_image_url": "https://example.com/profile.jpg",
    "verified": true,
    "created_at": 1615482367000,
    "updated_at": 1615482367000
  }
]
```

### POST /profile
Update user profile information and manage pets.

Headers:
```
Authorization: Bearer jwt-token
```

Request:
```json
{
  "first_name": "John",
  "last_name": "Doe",
  "email": "john.doe@example.com",
  "address": "123 Main St, Anytown, USA",
  "profile_image_url": "https://example.com/profile.jpg",
  "pets": [
    {
      "id": "pet-uuid", // Include for existing pets
      "name": "Buddy",
      "breed": "Golden Retriever",
      "sex": "M",
      "birthday": 1546300800000,
      "pet_image_url": "https://example.com/buddy.jpg",
      "color": "Golden",
      "species": "Dog",
      "spayed_neutered": true,
      "weight": 65
    }
  ]
}
```

Response:
```json
{
  "message": "Profile updated successfully",
  "user": {
    "id": "user-uuid",
    "phone_number": "1234567890",
    "public_key": "base64-encoded-public-key",
    "scope": "client",
    "first_name": "John",
    "last_name": "Doe",
    "email": "john.doe@example.com",
    "address": "123 Main St, Anytown, USA",
    "profile_image_url": "https://example.com/profile.jpg",
    "verified": true,
    "created_at": 1615482367000,
    "updated_at": 1615482367000
  },
  "pets": [
    {
      "id": "pet-uuid",
      "user_id": "user-uuid",
      "name": "Buddy",
      "breed": "Golden Retriever",
      "sex": "M",
      "birthday": 1546300800000,
      "pet_image_url": "https://example.com/buddy.jpg",
      "color": "Golden",
      "species": "Dog",
      "spayed_neutered": true,
      "weight": 65
    }
  ]
}
```

### POST /delete-account
Delete a user account and all associated data.

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

## Pet Management

### POST /pet
Create a new pet or update an existing pet.

Headers:
```
Authorization: Bearer jwt-token
```

Create a new pet:
```json
{
  "name": "Max",
  "breed": "Golden Retriever",
  "sex": "M",
  "birthday": 1579046400000,
  "pet_image_url": "https://example.com/pet_image.jpg"
}
```

Update an existing pet:
```json
{
  "id": "e1bf84be-0d14-42ec-8f1c-77918c3b9259",
  "name": "Updated Pet Name",
  "breed": "Updated Breed",
  "sex": "M"
}
```

Response (Creating):
```json
{
  "message": "Pet created successfully",
  "pet": {
    "id": "e1bf84be-0d14-42ec-8f1c-77918c3b9259",
    "user_id": "a1b2c3d4-e5f6-4a5b-8c9d-0e1f2a3b4c5d",
    "name": "Max",
    "breed": "Golden Retriever",
    "sex": "M",
    "birthday": 1579046400000,
    "pet_image_url": "https://example.com/pet_image.jpg",
    "color": null,
    "species": null,
    "spayed_neutered": null,
    "weight": null
  }
}
```

Response (Updating):
```json
{
  "message": "Pet updated successfully",
  "pet": {
    "id": "e1bf84be-0d14-42ec-8f1c-77918c3b9259",
    "user_id": "a1b2c3d4-e5f6-4a5b-8c9d-0e1f2a3b4c5d",
    "name": "Updated Pet Name",
    "breed": "Updated Breed",
    "sex": "M",
    "birthday": 1579046400000,
    "pet_image_url": "https://example.com/pet_image.jpg",
    "color": "Brown",
    "species": "Dog",
    "spayed_neutered": true,
    "weight": 25
  }
}
```

### DELETE /pet
Delete a pet from the user's account.

Headers:
```
Authorization: Bearer jwt-token
```

Request Body:
```json
{
  "id": "e1bf84be-0d14-42ec-8f1c-77918c3b9259"
}
```

Response:
```json
{
  "message": "Pet deleted successfully",
  "pet_id": "e1bf84be-0d14-42ec-8f1c-77918c3b9259"
}
```

## Image Management

### POST /upload-image
Upload an image file.

Headers:
```
Authorization: Bearer jwt-token
```

Query Parameters:
- `image_type`: Type of image (profile or pet)

Request:
Multipart form data with a file field.

Response:
```json
{
  "message": "Image uploaded successfully",
  "image": {
    "id": "image-uuid",
    "user_id": "user-uuid",
    "filename": "image.jpg",
    "content_type": "image/jpeg",
    "image_type": "profile",
    "image_url": "https://storage.googleapis.com/bucket/image.jpg",
    "created_at": 1615482367000,
    "updated_at": 1615482367000
  }
}
```

### GET /images
Get images for the authenticated user.

Headers:
```
Authorization: Bearer jwt-token
```

Query Parameters:
- `image_type` (optional): Filter by image type (profile or pet)

Response:
```json
[
  {
    "id": "image-uuid",
    "user_id": "user-uuid",
    "filename": "image.jpg",
    "content_type": "image/jpeg",
    "image_type": "profile",
    "image_url": "https://storage.googleapis.com/bucket/image.jpg",
    "created_at": 1615482367000,
    "updated_at": 1615482367000
  }
]
```

## WebSocket API

A full description of the WebSocket API can be found in [websockets.md](websockets.md).

### GET /ws
Establish a WebSocket connection for real-time messaging.

The WebSocket API supports the following event types:
- `conversations`: Get list of conversations
- `message`: Send a message to a conversation
- `new_conversation`: Create a new conversation
- `conversation_history`: Get message history for a conversation
- `user_joined`: User joined a conversation
- `user_left`: User left a conversation
- `unsubscribed`: Confirmation of unsubscribing from a conversation
- `new_message`: Notification of a new message
- `conversations_list`: Response with list of conversations
- `conversation_history_response`: Response with conversation history
- `error`: Error message

## Best Practices

1. Always handle connection errors and implement reconnection logic.
2. Subscribe to conversations as soon as the connection is established.
3. Store conversation and message IDs locally to avoid duplicate messages.
4. Use the `page` and `limit` parameters for pagination when fetching conversation history.
5. When uploading images, ensure they are in a supported format (jpg, jpeg, png, gif).
6. Use the image URLs returned from the `/upload-image` endpoint to update profile or pet images. 

## API DateTime Format

All datetime fields in requests and responses use Unix millisecond timestamps (milliseconds since the Unix epoch - January 1, 1970 00:00:00 UTC).

Examples:
- January 1, 2023 00:00:00 UTC: `1672531200000`
- June 15, 2023 12:30:45 UTC: `1686833445000`

## Pet Endpoints

### Create/Update Pet

POST `/pet`

**Request Body:**
```json
{
  "name": "Fluffy",
  "breed": "Persian",
  "sex": "F",
  "birthday": 1577836800000,  // January 1, 2020 00:00:00 UTC
  "pet_image_url": "https://example.com/images/fluffy.jpg"
}
```

**Response:**
```json
{
  "message": "Pet created successfully",
  "pet": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "user_id": "550e8400-e29b-41d4-a716-446655440001",
    "name": "Fluffy",
    "breed": "Persian",
    "sex": "F", 
    "birthday": 1577836800000,  // January 1, 2020 00:00:00 UTC
    "pet_image_url": "https://example.com/images/fluffy.jpg"
  }
}
```

## Conversation Messages

Messages in conversation history use millisecond timestamps:

```json
{
  "messages": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440002",
      "conversation_id": "550e8400-e29b-41d4-a716-446655440003",
      "sender_id": "550e8400-e29b-41d4-a716-446655440001",
      "content": "Hello, how can I help with your pet?",
      "timestamp": 1686833445000  // June 15, 2023 12:30:45 UTC
    }
  ]
}
```