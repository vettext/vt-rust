use actix::prelude::*; // Import Actix prelude for common traits and functionalities
use actix_web::{post, web, App, HttpRequest, HttpResponse, HttpServer, Responder, get};
use serde_json::json;
use sqlx::Arguments;
use sqlx::postgres::{PgPoolOptions, PgArguments}; // Re-added PgArguments
use chrono::{Utc};
use uuid::Uuid;

mod utils;
mod models;
mod services;
mod websockets; // Import the websockets module

use crate::utils::{
    is_timestamp_valid, send_verification_request, check_verification_code,
    verify_signature, generate_refresh_token, generate_signed_encrypted_token,
    verify_and_decode_token
};
use crate::models::{
    SignedData, RegisterData, RequestVerificationCodeData, LoginData,
    RefreshData, LogoutData, RefreshToken, UpdateProfileData, ProfilesQuery, User
};
use crate::websockets::websocket_route; // Import the WebSocket route handler

#[post("/register")]
async fn register(
    signed_data: web::Json<SignedData<RegisterData>>,
    pool: web::Data<sqlx::PgPool>
) -> impl Responder {
    println!("Register endpoint hit!");

    // Check timestamp
    if !is_timestamp_valid(&signed_data.data.timestamp) {
        return HttpResponse::BadRequest().body("Invalid timestamp");
    }

    // Verify signature
    if let Err(e) = verify_signature(
        &signed_data.data,
        &signed_data.signature,
        &signed_data.data.public_key
    ) {
        println!("Signature verification failed: {}", e);
        return HttpResponse::BadRequest().body("Invalid signature");
    }

    // Insert new user into the database
    let record = match sqlx::query!(
        "INSERT INTO users (phone_number, public_key, scope) VALUES ($1, $2, $3) RETURNING id",
        &signed_data.data.phone_number,
        &signed_data.data.public_key,
        "client"
    )
    .fetch_one(&**pool)
    .await {
        Ok(record) => record,
        Err(e) => {
            if e.to_string().contains("users_phone_number_key") {
                return HttpResponse::BadRequest().json(json!({
                    "message": "Phone number already registered"
                }));
            }
            return HttpResponse::InternalServerError().body(format!("Failed to insert user: {}", e));
        }
    };

    println!("Generated user_id: {:?}", record.id);

    // If phone number starts with "000123" then it is a test phone number
    if signed_data.data.phone_number.starts_with("000123") {
        return HttpResponse::Ok().json(json!({
            "message": "Test registration data received and verified. Test verification code is 123456.",
            "user_id": record.id
        }));
    }

    // Send Twilio verification code
    match send_verification_request(&signed_data.data.phone_number).await {
        Ok(_) => HttpResponse::Ok().json(json!({
            "message": "Registration data received and verified. Verification code sent.",
            "user_id": record.id
        })),
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed to send verification: {}", e)),
    }
}

#[post("/request-verification-code")]
async fn request_verification_code(
    signed_data: web::Json<SignedData<RequestVerificationCodeData>>,
    pool: web::Data<sqlx::PgPool>
) -> impl Responder {
    println!("Request verification code endpoint hit!");

    // Check timestamp
    if !is_timestamp_valid(&signed_data.data.timestamp) {
        return HttpResponse::BadRequest().body("Invalid timestamp");
    }

    // Look up the user's public key and phone number by phone number
    let user_data = match sqlx::query!(
        "SELECT id, public_key FROM users WHERE phone_number = $1",
        &signed_data.data.phone_number
    )
    .fetch_optional(&**pool)
    .await {
        Ok(Some(record)) => record,
        Ok(None) => return HttpResponse::NotFound().body(format!("User not found for phone number: {}", signed_data.data.phone_number)),
        Err(e) => return HttpResponse::InternalServerError().body(format!("Database error: {}", e)),
    };

    // Verify signature using the retrieved public key
    if let Err(e) = verify_signature(
        &signed_data.data,
        &signed_data.signature,
        &user_data.public_key
    ) {
        println!("Signature verification failed: {}", e);
        return HttpResponse::BadRequest().body("Invalid signature");
    }

    // If phone number starts with "000123" then it is a test phone number
    if signed_data.data.phone_number.starts_with("000123") {
        return HttpResponse::Ok().json(json!({
            "message": "Test registration data received and verified. Test verification code is 123456.",
            "user_id": user_data.id
        }));
    }

    // Send Twilio verification code
    match send_verification_request(&signed_data.data.phone_number).await {
        Ok(_) => HttpResponse::Ok().json(json!({
            "message": "Verification code sent",
            "user_id": user_data.id
        })),
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed to send verification: {}", e)),
    }
}

#[post("/login")]
async fn login(
    signed_data: web::Json<SignedData<LoginData>>,
    pool: web::Data<sqlx::PgPool>
) -> impl Responder {
    println!("Login endpoint hit!");

    // Check timestamp
    if !is_timestamp_valid(&signed_data.data.timestamp) {
        return HttpResponse::BadRequest().body("Invalid timestamp");
    }

    // Look up the user's public key and verified status by user_id
    let user_data = match sqlx::query!(
        "SELECT public_key, verified, phone_number, scope FROM users WHERE id = $1",
        &signed_data.data.user_id
    )
    .fetch_optional(&**pool)
    .await {
        Ok(Some(record)) => record,
        Ok(None) => return HttpResponse::NotFound().body(format!("User not found for id: {}", signed_data.data.user_id)),
        Err(e) => return HttpResponse::InternalServerError().body(format!("Database error: {}", e)),
    };

    // Verify signature using the retrieved public key
    if let Err(e) = verify_signature(
        &signed_data.data,
        &signed_data.signature,
        &user_data.public_key
    ) {
        println!("Signature verification failed: {}", e);
        return HttpResponse::BadRequest().body("Invalid signature");
    }

    // If phone number starts with "000123" then it is a test phone number
    if user_data.phone_number.starts_with("000123") {
        if signed_data.data.verification_code != "123456" {
            return HttpResponse::BadRequest().json(json!({
                "message": "Invalid verification code"
            }));
        }
    } else {
        // Check Twilio verification code
        let is_valid = match check_verification_code(&user_data.phone_number, &signed_data.data.verification_code).await {
            Ok(is_valid) => is_valid,
            Err(e) => return HttpResponse::InternalServerError().body(format!("Failed to check verification: {}", e)),
        };

        if !is_valid {
            return HttpResponse::BadRequest().json(json!({
                "message": "Invalid verification code"
            }));
        }
    }

    // Update user to verified=true if not already verified
    if !user_data.verified {
        if let Err(e) = sqlx::query!(
            "UPDATE users SET verified = true WHERE id = $1",
            &signed_data.data.user_id
        )
        .execute(&**pool)
        .await {
            return HttpResponse::InternalServerError().body(format!("Failed to update user: {}", e));
        }
    }

    // Delete existing non-invalidated refresh tokens
    if let Err(e) = sqlx::query!(
        "DELETE FROM refresh_tokens WHERE user_id = $1 AND is_revoked = false",
        &signed_data.data.user_id
    )
    .execute(&**pool)
    .await {
        println!("Failed to delete existing tokens: {}", e);
    }

    // Generate new refresh token
    let refresh_token = generate_refresh_token();

    // Save refresh token to database
    // TODO: add user_agent
    if let Err(e) = sqlx::query!(
        "INSERT INTO refresh_tokens (token, user_id) VALUES ($1, $2)",
        refresh_token,
        &signed_data.data.user_id
    )
    .execute(&**pool)
    .await {
        return HttpResponse::InternalServerError().body(format!("Failed to save refresh token: {}", e));
    }

    // Generate access token
    let access_token = match generate_signed_encrypted_token(signed_data.data.user_id, &user_data.scope) {
        Ok(token) => token,
        Err(e) => return HttpResponse::InternalServerError().body(format!("Failed to generate access token: {}", e)),
    };

    HttpResponse::Ok().json(json!({
        "message": "Login successful",
        "user_id": &signed_data.data.user_id,
        "access_token": access_token,
        "refresh_token": refresh_token
    }))
}

#[post("/refresh")]
async fn refresh(
    signed_data: web::Json<SignedData<RefreshData>>,
    pool: web::Data<sqlx::PgPool>
) -> impl Responder {
    println!("Refresh endpoint hit!");

    // Check timestamp
    if !is_timestamp_valid(&signed_data.data.timestamp) {
        return HttpResponse::BadRequest().body("Invalid timestamp");
    }

    // Look up the refresh token
    let refresh_token_record = match sqlx::query_as!(
        RefreshToken,
        "SELECT * FROM refresh_tokens WHERE token = $1 AND user_id = $2",
        &signed_data.data.refresh_token,
        &signed_data.data.user_id
    )
    .fetch_optional(&**pool)
    .await {
        Ok(Some(token)) => token,
        Ok(None) => return HttpResponse::Unauthorized().body("Refresh token not found"),
        Err(e) => return HttpResponse::InternalServerError().body(format!("Database error: {}", e)),
    };

    if refresh_token_record.is_revoked {
        return HttpResponse::Unauthorized().body("Invalid refresh token");
    }

    // Look up the user's info by user_id
    let user_data = match sqlx::query!(
        "SELECT public_key, scope FROM users WHERE id = $1",
        refresh_token_record.user_id
    )
    .fetch_optional(&**pool)
    .await {
        Ok(Some(record)) => record,
        Ok(None) => return HttpResponse::NotFound().body(format!("User not found for id: {}", refresh_token_record.user_id)),
        Err(e) => return HttpResponse::InternalServerError().body(format!("Database error: {}", e)),
    };

    // Verify signature
    if let Err(e) = verify_signature(
        &signed_data.data,
        &signed_data.signature,
        &user_data.public_key
    ) {
        println!("Signature verification failed: {}", e);
        return HttpResponse::BadRequest().body("Invalid signature");
    }

    // Update last_used_at
    let now = Utc::now();
    if let Err(e) = sqlx::query!(
        "UPDATE refresh_tokens SET last_used_at = $1 WHERE token = $2",
        now,
        &signed_data.data.refresh_token
    )
    .execute(&**pool)
    .await {
        return HttpResponse::InternalServerError().body(format!("Failed to update refresh token: {}", e));
    }

    // Generate new access token
    let access_token = match generate_signed_encrypted_token(refresh_token_record.user_id, &user_data.scope) {
        Ok(token) => token,
        Err(e) => return HttpResponse::InternalServerError().body(format!("Failed to generate access token: {}", e)),
    };

    HttpResponse::Ok().json(json!({
        "message": "Token refreshed successfully",
        "access_token": access_token
    }))
}

#[post("/logout")]
async fn logout(
    signed_data: web::Json<SignedData<LogoutData>>,
    pool: web::Data<sqlx::PgPool>
) -> impl Responder {
    println!("Logout endpoint hit!");

    // Check timestamp
    if !is_timestamp_valid(&signed_data.data.timestamp) {
        return HttpResponse::BadRequest().body("Invalid timestamp");
    }

    // Look up the user's public key by user_id
    let public_key = match sqlx::query!(
        "SELECT public_key FROM users WHERE id = $1",
        &signed_data.data.user_id
    )
    .fetch_optional(&**pool)
    .await {
        Ok(Some(record)) => record.public_key,
        Ok(None) => return HttpResponse::NotFound().body(format!("User not found for id: {}", &signed_data.data.user_id)),
        Err(e) => return HttpResponse::InternalServerError().body(format!("Database error: {}", e)),
    };

    // Verify signature
    if let Err(e) = verify_signature(
        &signed_data.data,
        &signed_data.signature,
        &public_key
    ) {
        println!("Signature verification failed: {}", e);
        return HttpResponse::BadRequest().body("Invalid signature");
    }

    // Delete the refresh token
    match sqlx::query!(
        "DELETE FROM refresh_tokens WHERE token = $1 AND user_id = $2",
        &signed_data.data.refresh_token,
        &signed_data.data.user_id
    )
    .execute(&**pool)
    .await {
        Ok(result) => {
            if result.rows_affected() > 0 {
                HttpResponse::Ok().json(json!({
                    "message": "Logged out successfully"
                }))
            } else {
                HttpResponse::NotFound().json(json!({
                    "message": "Refresh token not found for this user"
                }))
            }
        },
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed to delete refresh token: {}", e)),
    }
}

#[get("/profiles")]
async fn get_profiles(
    req: HttpRequest,
    query: web::Query<ProfilesQuery>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    // Extract and verify the token from the Authorization header
    let token = match req.headers().get("Authorization") {
        Some(value) => {
            let parts: Vec<&str> = value.to_str().unwrap_or("").split_whitespace().collect();
            if parts.len() == 2 && parts[0] == "Bearer" {
                parts[1]
            } else {
                return HttpResponse::Unauthorized().body("Invalid Authorization header");
            }
        }
        None => return HttpResponse::Unauthorized().body("Missing Authorization header"),
    };

    // Verify and decode the token
    let claims = match verify_and_decode_token(token) {
        Ok(claims) => claims,
        Err(_) => return HttpResponse::Unauthorized().body("Invalid token"),
    };

    // Parse the user_ids from the query string
    let user_ids: Vec<Uuid> = query.user_ids
        .split(',')
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect();

    // Execute the query based on the authenticated user's scope
    let rows = if claims.get_scope() == "provider" {
        sqlx::query_as::<_, User>(
            "SELECT id, phone_number, public_key, scope, first_name, last_name, email, address, profile_image_url, verified, created_at, updated_at FROM users WHERE id = ANY($1)"
        )
        .bind(&user_ids)
        .fetch_all(&**pool)
        .await
    } else {
        sqlx::query_as::<_, User>(
            "SELECT id, phone_number, public_key, scope, first_name, last_name, email, address, profile_image_url, verified, created_at, updated_at FROM users WHERE (id = ANY($1) AND (scope = 'provider' OR id = $2))"
        )
        .bind(&user_ids)
        .bind(Uuid::parse_str(claims.get_sub()).unwrap())
        .fetch_all(&**pool)
        .await
    };

    match rows {
        Ok(users) => HttpResponse::Ok().json(users),
        Err(e) => HttpResponse::InternalServerError().body(format!("Database error: {}", e)),
    }
}

#[post("/profile")]
async fn update_profile(
    req: HttpRequest,
    data: web::Json<UpdateProfileData>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    // Extract and verify the token from the Authorization header
    let token = match req.headers().get("Authorization") {
        Some(value) => {
            let parts: Vec<&str> = value.to_str().unwrap_or("").split_whitespace().collect();
            if parts.len() == 2 && parts[0] == "Bearer" {
                parts[1]
            } else {
                return HttpResponse::Unauthorized().body("Invalid Authorization header");
            }
        }
        None => return HttpResponse::Unauthorized().body("Missing Authorization header"),
    };

    // Verify and decode the token
    let claims = match verify_and_decode_token(token) {
        Ok(claims) => claims,
        Err(_) => return HttpResponse::Unauthorized().body("Invalid token"),
    };

    let user_id = match Uuid::parse_str(claims.get_sub()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::Unauthorized().body("Invalid user ID in token"),
    };

    // Collect fields to update
    let mut update_fields = Vec::new();
    let mut args = PgArguments::default();
    let mut param_count = 1;

    if let Some(first_name) = &data.first_name {
        update_fields.push(format!("first_name = ${}", param_count));
        args.add(first_name.clone());
        param_count += 1;
    }
    if let Some(last_name) = &data.last_name {
        update_fields.push(format!("last_name = ${}", param_count));
        args.add(last_name.clone());
        param_count += 1;
    }
    if let Some(email) = &data.email {
        update_fields.push(format!("email = ${}", param_count));
        args.add(email.clone());
        param_count += 1;
    }
    if let Some(address) = &data.address {
        update_fields.push(format!("address = ${}", param_count));
        args.add(address.clone());
        param_count += 1;
    }
    if let Some(profile_image_url) = &data.profile_image_url {
        update_fields.push(format!("profile_image_url = ${}", param_count));
        args.add(profile_image_url.clone());
        param_count += 1;
    }

    if !update_fields.is_empty() {
        let query = format!(
            "UPDATE users SET {} WHERE id = ${}",
            update_fields.join(", "),
            param_count
        );
        args.add(user_id);

        let result = sqlx::query_with(&query, args)
            .execute(&**pool)
            .await;

        if let Err(e) = result {
            return HttpResponse::InternalServerError().body(format!("Failed to update user profile: {}", e));
        }
    }

    // Update or create pets
    for pet in &data.pets {
        if let Some(pet_id) = pet.id {
            // Update existing pet  
            let mut pet_update_fields = Vec::new();
            let mut pet_args = PgArguments::default();
            let mut pet_param_count = 1;

            if let Some(name) = &pet.name {
                pet_update_fields.push(format!("name = ${}", pet_param_count));
                pet_args.add(name.clone());
                pet_param_count += 1;
            }
            if let Some(breed) = &pet.breed {
                pet_update_fields.push(format!("breed = ${}", pet_param_count));
                pet_args.add(breed.clone());
                pet_param_count += 1;
            }
            if let Some(sex) = &pet.sex {
                pet_update_fields.push(format!("sex = ${}", pet_param_count));
                pet_args.add(sex.clone());
                pet_param_count += 1;
            }
            if let Some(birthday) = &pet.birthday {
                pet_update_fields.push(format!("birthday = ${}", pet_param_count));
                pet_args.add(*birthday);
                pet_param_count += 1;
            }
            if let Some(pet_image_url) = &pet.pet_image_url {
                pet_update_fields.push(format!("pet_image_url = ${}", pet_param_count));
                pet_args.add(pet_image_url.clone());
                pet_param_count += 1;
            }

            if !pet_update_fields.is_empty() {
                let pet_query = format!(
                    "UPDATE pets SET {} WHERE id = ${} AND user_id = ${}",
                    pet_update_fields.join(", "),
                    pet_param_count,
                    pet_param_count + 1
                );
                pet_args.add(pet_id);
                pet_args.add(user_id);

                if let Err(e) = sqlx::query_with(&pet_query, pet_args)
                    .execute(&**pool)
                    .await 
                {
                    return HttpResponse::InternalServerError().body(format!("Failed to update pet: {}", e));
                }
            }
        } else {
            // Create new pet
            // Assign to variables to avoid temporary value issues
            let name = pet.name.clone().unwrap_or_else(|| "".to_string());
            let breed = pet.breed.clone().unwrap_or_else(|| "".to_string());
            let sex = pet.sex.clone().unwrap_or_else(|| "".to_string());
            let birthday = pet.birthday.unwrap_or(Utc::now());
            let pet_image_url = pet.pet_image_url.clone().unwrap_or_else(|| "".to_string());

            if let Err(e) = sqlx::query!(
                    "INSERT INTO pets (user_id, name, breed, sex, birthday, pet_image_url) VALUES ($1, $2, $3, $4, $5, $6)",
                    user_id,
                    name,
                    breed,
                    sex,
                    birthday,
                    pet_image_url
                )
                .execute(&**pool)
                .await
            {
                return HttpResponse::InternalServerError().body(format!("Failed to create new pet: {}", e));
            }
        }
    }

    HttpResponse::Ok().json(json!({
        "message": "Profile updated successfully"
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create pool");

    // Start the WebSocket server actor
    let ws_server = websockets::WsServer::new().start();

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(ws_server.clone())) // Share the server address with handlers
            .service(register)
            .service(request_verification_code)
            .service(login)
            .service(refresh)
            .service(logout)
            .service(get_profiles)
            .service(update_profile)
            .service(websocket_route) // Register the WebSocket route
    })
    // .bind(("127.0.0.1", 8080))?
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
