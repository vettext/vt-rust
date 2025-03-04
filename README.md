# VetText API Server

This is an API for the VetText app, written in Rust with Actix-Web.

## Dependencies
rust/cargo is required to run the server:
```{bash}
curl https://sh.rustup.rs -sSf | sh
```

## Documentation

- [API Documentation](docs/api.md) - Detailed information about all REST API endpoints
- [WebSocket Documentation](docs/websockets.md) - Information about the WebSocket API
- [OpenAPI Specification](docs/openapi.txt) - OpenAPI specification file

## Running the Server

```bash
cargo run
```

The server will start on port 8080.

## Running Tests

```bash
cargo test
```

## Environment Variables

- `DATABASE_URL`: PostgreSQL connection string
- `JWT_SECRET`: Secret key for JWT tokens
- `GCS_BUCKET_NAME`: Google Cloud Storage bucket name