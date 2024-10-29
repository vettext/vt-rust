# VetText API Server

This is an API for the VetText app, written in Rust with Actix-Web.

## dependencies
rust/cargo is required to run the server:
```{bash}
curl https://sh.rustup.rs -sSf | sh
```

## development
to run the dev server:
```{bash}
cargo run
```

## testing
to test registration locally:
```{bash}
curl -d '{"data": {"phone_number":"5555551234", "public_key": "test", "timestamp":"2024-09-02"}, "signature": "1234"}' -H 'Content-Type: application/json' http://127.0.0.1:8080/register -v
```

## database
to add a migration:
```{bash}
sqlx migrate add -r migration_name
```

to run migrations:
```{bash}
sqlx migrate run
```

to run migrations in rust:
```{rust}
sqlx::migrate!
```

to revert migrations:
```{bash}
sqlx migrate revert
```

## todo
- update handler functions to return Result<HttpResponse, Error> ?
- make UserScope an enum
- make function for auth header parsing and token verification
- restructure project with routes/ handlers/, utils/, and services/ dirs
- add code style formatting (rustfmt)
- move DB stuff to service layer
- add this function: let public_key = get_public_key_for_user(data.user_id, &pool).await;
- for prod, log detailed error info and return generic message to client