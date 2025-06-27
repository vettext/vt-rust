use actix::prelude::*; // Import Actix prelude for common traits and functionalities
use actix_web::{post, web, App, HttpRequest, HttpResponse, HttpServer, Responder, get, delete};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use chrono::{Utc, DateTime};
use uuid::Uuid;
use actix_multipart::Multipart;
use futures::{StreamExt, TryStreamExt};
use std::path::Path;
use google_cloud_storage::client::{Client as GcsClient, ClientConfig};
use sqlx::FromRow;
use serde::Serialize;
use serde::Deserialize;
use mime;
use google_cloud_storage::http::objects::upload::{UploadObjectRequest, UploadType, Media};
use std::borrow::Cow;

mod utils;
mod models;
mod services;
mod websockets; // Import the websockets module

use crate::utils::{
    is_timestamp_valid, send_verification_request, check_verification_code,
    verify_signature, generate_refresh_token, generate_signed_encrypted_token,
    verify_and_decode_token, extract_user_id_from_token
};
use crate::models::{
    SignedData, RegisterData, RequestVerificationCodeData, LoginData,
    RefreshData, LogoutData, RefreshToken, UpdateProfileData, ProfilesQuery, DeleteUserData,
    Pet, GetImagesQuery, UploadImageQuery, UpdatePetData, DeletePetData
};
use crate::websockets::websocket_route; // Import the WebSocket route handler

#[derive(FromRow, Debug, Serialize, Deserialize)]
struct UserWithPet {
    // User fields
    id: Option<Uuid>,
    phone_number: Option<String>,
    public_key: Option<String>,
    scope: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    email: Option<String>,
    address: Option<String>,
    profile_image_url: Option<String>,
    verified: Option<bool>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    created_at: Option<DateTime<Utc>>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    updated_at: Option<DateTime<Utc>>,
    // Pet fields
    pet_id: Option<Uuid>,
    pet_user_id: Option<Uuid>,
    pet_name: Option<String>,
    pet_breed: Option<String>,
    pet_sex: Option<String>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pet_birthday: Option<DateTime<Utc>>,
    pet_image_url: Option<String>,
    pet_color: Option<String>,
    pet_species: Option<String>,
    pet_spayed_neutered: Option<bool>,
    pet_weight: Option<i32>,
}

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
    let (access_token, expiration) = match generate_signed_encrypted_token(signed_data.data.user_id, &user_data.scope) {
        Ok((token, exp)) => (token, exp),
        Err(e) => return HttpResponse::InternalServerError().body(format!("Failed to generate access token: {}", e)),
    };

    HttpResponse::Ok().json(json!({
        "message": "Login successful",
        "user_id": &signed_data.data.user_id,
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_at": expiration
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
    let (access_token, expiration) = match generate_signed_encrypted_token(refresh_token_record.user_id, &user_data.scope) {
        Ok((token, exp)) => (token, exp),
        Err(e) => return HttpResponse::InternalServerError().body(format!("Failed to generate access token: {}", e)),
    };

    HttpResponse::Ok().json(json!({
        "message": "Token refreshed successfully",
        "access_token": access_token,
        "expires_at": expiration
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
        sqlx::query_as!(
            UserWithPet,
            r#"
            SELECT 
                u.id, u.phone_number, u.public_key, u.scope, 
                u.first_name, u.last_name, u.email, u.address, 
                u.profile_image_url, u.verified, u.created_at, u.updated_at,
                p.id as "pet_id?", p.user_id as "pet_user_id?", 
                p.name as "pet_name?", p.breed as "pet_breed?",
                p.sex as "pet_sex?", p.birthday as "pet_birthday?", 
                p.pet_image_url as "pet_image_url?",
                p.color as "pet_color?", p.species as "pet_species?", 
                p.spayed_neutered as "pet_spayed_neutered?",
                p.weight as "pet_weight?"
            FROM users u
            LEFT JOIN pets p ON u.id = p.user_id
            WHERE u.id = ANY($1)
            "#,
            &user_ids
        )
        .fetch_all(&**pool)
        .await
    } else {
        sqlx::query_as!(
            UserWithPet,
            r#"
            SELECT 
                u.id, u.phone_number, u.public_key, u.scope, 
                u.first_name, u.last_name, u.email, u.address, 
                u.profile_image_url, u.verified, u.created_at, u.updated_at,
                p.id as "pet_id?", p.user_id as "pet_user_id?", 
                p.name as "pet_name?", p.breed as "pet_breed?",
                p.sex as "pet_sex?", p.birthday as "pet_birthday?", 
                p.pet_image_url as "pet_image_url?",
                p.color as "pet_color?", p.species as "pet_species?", 
                p.spayed_neutered as "pet_spayed_neutered?",
                p.weight as "pet_weight?"
            FROM users u
            LEFT JOIN pets p ON u.id = p.user_id
            WHERE (u.id = ANY($1) AND (u.scope = 'provider' OR u.id = $2))
            "#,
            &user_ids,
            Uuid::parse_str(claims.get_sub()).unwrap()
        )
        .fetch_all(&**pool)
        .await
    };

    match rows {
        Ok(rows) => HttpResponse::Ok().json(rows),
        Err(e) => HttpResponse::InternalServerError().body(format!("Database error: {}", e)),
    }
}

#[post("/profile")]
async fn update_profile(
    req: HttpRequest,
    data: web::Json<UpdateProfileData>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    // Extract the user_id from the token
    let user_id = match extract_user_id_from_token(&req) {
        Ok(id) => id,
        Err(e) => return HttpResponse::Unauthorized().body(e.to_string()),
    };

    // Start a transaction
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => return HttpResponse::InternalServerError().body(format!("Failed to start transaction: {}", e)),
    };

    // Update user profile fields
    if let Err(e) = sqlx::query!(
        "UPDATE users SET 
            first_name = COALESCE($1, first_name), 
            last_name = COALESCE($2, last_name), 
            email = COALESCE($3, email), 
            address = COALESCE($4, address), 
            profile_image_url = COALESCE($5, profile_image_url), 
            updated_at = CURRENT_TIMESTAMP 
        WHERE id = $6",
        data.first_name,
        data.last_name,
        data.email,
        data.address,
        data.profile_image_url,
        user_id
    )
    .execute(&mut *tx)
    .await {
        let _ = tx.rollback().await;
        return HttpResponse::InternalServerError().body(format!("Failed to update user: {}", e));
    }

    // Handle pets
    let mut updated_pets = Vec::new();
    
    for pet_data in &data.pets {
        let pet_result = if let Some(pet_id) = pet_data.id {
            // Update existing pet  
            let updated_pet = match sqlx::query_as!(
                Pet,
                r#"
                UPDATE pets
                SET 
                    name = COALESCE($1, name),
                    breed = COALESCE($2, breed),
                    sex = COALESCE($3, sex),
                    birthday = COALESCE($4, birthday),
                    pet_image_url = COALESCE($5, pet_image_url),
                    color = COALESCE($6, color),
                    species = COALESCE($7, species),
                    spayed_neutered = COALESCE($8, spayed_neutered),
                    weight = COALESCE($9, weight),
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $10 AND user_id = $11
                RETURNING id, user_id, name, breed, sex, birthday, pet_image_url, color, species, spayed_neutered, weight
                "#,
                pet_data.name,
                pet_data.breed,
                pet_data.sex,
                pet_data.birthday,
                pet_data.pet_image_url,
                pet_data.color,
                pet_data.species,
                pet_data.spayed_neutered,
                pet_data.weight,
                pet_id,
                user_id
            )
            .fetch_optional(&mut *tx)
            .await {
                Ok(pet) => pet,
                Err(e) => {
                    let _ = tx.rollback().await;
                    return HttpResponse::InternalServerError().body(format!("Failed to update pet: {}", e));
                }
            };
            
            // Convert Option<Pet> to Result<Pet, Error>
            match updated_pet {
                Some(pet) => Ok(pet),
                None => Err(sqlx::Error::RowNotFound)
            }
        } else {
            // Create new pet
            sqlx::query_as!(
                Pet,
                r#"
                INSERT INTO pets (user_id, name, breed, sex, birthday, pet_image_url, color, species, spayed_neutered, weight)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                RETURNING id, user_id, name, breed, sex, birthday, pet_image_url, color, species, spayed_neutered, weight
                "#,
                user_id,
                pet_data.name.clone().unwrap_or_else(|| "".to_string()),
                pet_data.breed.clone().unwrap_or_else(|| "".to_string()),
                pet_data.sex.clone().unwrap_or_else(|| "".to_string()),
                pet_data.birthday,
                pet_data.pet_image_url,
                pet_data.color,
                pet_data.species,
                pet_data.spayed_neutered,
                pet_data.weight
            )
            .fetch_one(&mut *tx)
            .await
        };

        match pet_result {
            Ok(pet) => {
                updated_pets.push(pet);
            }
            Err(e) => {
                let _ = tx.rollback().await;
                return HttpResponse::InternalServerError().body(format!("Failed to update pet: {}", e));
            }
        }
    }

    // Commit the transaction
    if let Err(e) = tx.commit().await {
        return HttpResponse::InternalServerError().body(format!("Failed to commit transaction: {}", e));
    }

    // Return success response with updated pets
    HttpResponse::Ok().json(json!({
        "message": "Profile updated successfully",
        "pets": updated_pets
    }))
}

#[post("/delete-account")]
async fn delete_account(
    signed_data: web::Json<SignedData<DeleteUserData>>,
    pool: web::Data<sqlx::PgPool>
) -> impl Responder {
    println!("Delete account endpoint hit!");

    // Check timestamp
    if !is_timestamp_valid(&signed_data.data.timestamp) {
        return HttpResponse::BadRequest().body("Invalid timestamp");
    }

    // Look up the user's public key by user_id
    let user_data = match sqlx::query!(
        "SELECT public_key FROM users WHERE id = $1",
        &signed_data.data.user_id
    )
    .fetch_optional(&**pool)
    .await {
        Ok(Some(record)) => record,
        Ok(None) => return HttpResponse::NotFound().body(format!("User not found for id: {}", &signed_data.data.user_id)),
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

    // Start a transaction to ensure all deletions succeed or fail together
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => return HttpResponse::InternalServerError().body(format!("Failed to start transaction: {}", e)),
    };

    // Delete refresh tokens
    if let Err(e) = sqlx::query!(
        "DELETE FROM refresh_tokens WHERE user_id = $1",
        &signed_data.data.user_id
    )
    .execute(&mut *tx)
    .await {
        let _ = tx.rollback().await;
        return HttpResponse::InternalServerError().body(format!("Failed to delete refresh tokens: {}", e));
    }

    // Delete pets
    if let Err(e) = sqlx::query!(
        "DELETE FROM pets WHERE user_id = $1",
        &signed_data.data.user_id
    )
    .execute(&mut *tx)
    .await {
        let _ = tx.rollback().await;
        return HttpResponse::InternalServerError().body(format!("Failed to delete pets: {}", e));
    }

    // Finally, delete the user
    if let Err(e) = sqlx::query!(
        "DELETE FROM users WHERE id = $1",
        &signed_data.data.user_id
    )
    .execute(&mut *tx)
    .await {
        let _ = tx.rollback().await;
        return HttpResponse::InternalServerError().body(format!("Failed to delete user: {}", e));
    }

    // Commit the transaction
    if let Err(e) = tx.commit().await {
        return HttpResponse::InternalServerError().body(format!("Failed to commit transaction: {}", e));
    }

    HttpResponse::Ok().json(json!({
        "message": "Account and all personal data successfully deleted. Conversation history has been preserved."
    }))
}

#[post("/upload-image")]
async fn upload_image(
    req: HttpRequest,
    mut payload: Multipart,
    query: web::Query<UploadImageQuery>,
    pool: web::Data<sqlx::PgPool>
) -> impl Responder {
    println!("Upload image endpoint hit!");

    // Extract the user_id from the token
    let user_id = match extract_user_id_from_token(&req) {
        Ok(id) => id,
        Err(e) => {
            println!("❌ Failed to extract user_id from token: {}", e);
            return HttpResponse::Unauthorized().body(e.to_string());
        }
    };

    // Validate image type
    let image_type = match &query.image_type {
        Some(image_type) if ["profile", "pet"].contains(&image_type.to_lowercase().as_str()) => image_type.to_lowercase(),
        Some(invalid_type) => {
            println!("❌ Invalid image_type provided: {}", invalid_type);
            return HttpResponse::BadRequest().body("Invalid image_type. Must be 'profile' or 'pet'");
        },
        None => {
            println!("❌ Missing image_type parameter");
            return HttpResponse::BadRequest().body("Missing image_type parameter");
        }
    };

    // Generate a unique image ID
    let image_id = Uuid::new_v4();
    
    // Process the multipart form data
    let mut image_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    
    while let Ok(Some(mut field)) = payload.try_next().await {
        let content_disposition = match field.content_disposition() {
            Some(cd) => cd,
            None => {
                eprintln!("⚠️ Field without content disposition, skipping...");
                continue;
            }
        };
        
        if let Some(name) = content_disposition.get_name() {
            if name == "file" {
                // Get the filename
                if let Some(fname) = content_disposition.get_filename() {
                    filename = Some(fname.to_string());
                    
                    // Get the content type
                    if let Some(ct) = field.content_type() {
                        if ct.type_() == mime::IMAGE {
                            content_type = Some(ct.to_string());
                        } else {
                            eprintln!("❌ Content type is not an image: {}", ct);
                            return HttpResponse::BadRequest().body("File must be an image");
                        }
                    } else {
                        eprintln!("⚠️ No content type found in field, will infer from extension");
                    }
                    
                    // Read the file data
                    let mut data = Vec::new();
                    while let Some(chunk) = field.next().await {
                        match chunk {
                            Ok(bytes) => data.extend_from_slice(&bytes),
                            Err(e) => {
                                eprintln!("❌ Error reading file chunk: {}", e);
                                return HttpResponse::InternalServerError().body(format!("Error reading file: {}", e));
                            }
                        }
                    }
                    
                    image_data = Some(data);
                } else {
                    eprintln!("❌ No filename found in content disposition");
                    return HttpResponse::BadRequest().body("No filename provided");
                }
            } else {
                eprintln!("⚠️ Skipping non-file field: {}", name);
            }
        } else {
            eprintln!("⚠️ Field without name, skipping...");
        }
    }

    // Check if we have the image data
    let image_bytes = match image_data {
        Some(data) => {
            println!("✅ Image data received: {} bytes", data.len());
            
            data
        },
        None => {
            eprintln!("❌ No image file provided in multipart data");
            return HttpResponse::BadRequest().body("No image file provided");
        }
    };
    
    // Get file extension for content type detection
    println!("Determining file extension and content type...");
    let file_ext = match filename.as_ref().and_then(|name| {
        Path::new(name).extension().and_then(|ext| ext.to_str()).map(|s| s.to_lowercase())
    }) {
        Some(ext) => ext,
        None => {
            println!("⚠️ No file extension found, defaulting to jpg");
            "jpg".to_string()
        }
    };

    // Create the GCS client using proper authentication
    let client_config = match ClientConfig::default().with_auth().await {
        Ok(config) => config,
        Err(e) => {
            println!("❌ Error setting up GCS authentication: {}", e);
            return HttpResponse::InternalServerError().body(format!("Failed to initialize GCS client: {}", e));
        }
    };

    let client = GcsClient::new(client_config);
    
    // Get bucket name from env
    let bucket_name = match std::env::var("GCS_BUCKET_NAME") {
        Ok(name) => name,
        Err(_) => {
            println!("❌ GCS_BUCKET_NAME not set in environment");
            return HttpResponse::InternalServerError().body("GCS_BUCKET_NAME not set in environment");
        }
    };
    
    // Generate a unique object name
    let object_name = format!("{}/{}.{}", image_type, Uuid::new_v4(), file_ext);

    // Determine the content type
    let content_type_str = match &content_type {
        Some(ct) => {
            println!("✅ Using content type from field: {}", ct);
            ct.clone()
        },
        None => {
            let inferred_type = match file_ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg".to_string(),
                "png" => "image/png".to_string(),
                "gif" => "image/gif".to_string(),
                _ => "application/octet-stream".to_string(),
            };
            println!("✅ Inferred content type: {}", inferred_type);
            inferred_type
        }
    };

    // Update the upload call to use the correct API
    println!("Preparing GCS upload request...");
    let upload_request = UploadObjectRequest {
        bucket: bucket_name.clone(),
        ..Default::default()
    };
    // Media with object name and content type
    let media = Media {
        name: Cow::Owned(object_name.clone()),
        content_type: Cow::Owned(content_type_str.clone()),
        content_length: Some(image_bytes.len() as u64),
    };
    let upload_type = UploadType::Simple(media);
    let upload_result = client
        .upload_object(&upload_request, image_bytes.clone(), &upload_type)
        .await;
    match &upload_result {
        Ok(_) => (),
        Err(e) => {
            println!("❌ Upload failed: {:?}", e);
            
            let error_string = format!("{:?}", e);
            if error_string.contains("status code: 403") {
                println!("❌ This is a permissions error (403 Forbidden)");
            } else if error_string.contains("status code: 404") {
                println!("❌ This is a not found error (404 Not Found) - check bucket name");
            }
            
            // Check bucket name case sensitivity
            println!("❌ Using bucket name: '{}' (check case sensitivity)", bucket_name);
            println!("❌ Object path: '{}'", object_name);
        }
    }
    let image_url = match upload_result {
        Ok(_) => {
            // Generate a public URL for the uploaded image
            let url = format!(
                "https://storage.googleapis.com/{}/{}",
                bucket_name,
                object_name
            );
            url
        },
        Err(e) => {
            println!("❌ Failed to upload image to GCS: {}", e);
            return HttpResponse::InternalServerError().body(format!("Failed to upload image to GCS: {}", e));
        }
    };
    let result = sqlx::query!(
        "INSERT INTO images (id, user_id, filename, content_type, image_type, image_url) 
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
        image_id,
        user_id,
        filename,
        content_type,
        image_type,
        image_url
    )
    .fetch_one(&**pool)
    .await;
    match result {
        Ok(_) => {
            HttpResponse::Ok().json(json!({
                "message": "Image uploaded successfully",
                "image_id": image_id,
                "image_url": image_url
            }))
        },
        Err(e) => {
            println!("❌ Failed to store image metadata in database: {}", e);
            println!("=== IMAGE UPLOAD FAILED ===");
            HttpResponse::InternalServerError().body(format!("Failed to store image metadata: {}", e))
        }
    }
}

#[get("/images")]
async fn get_images(
    req: HttpRequest,
    query: web::Query<GetImagesQuery>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    // Extract the user_id from the token
    let user_id = match extract_user_id_from_token(&req) {
        Ok(id) => id,
        Err(e) => return HttpResponse::Unauthorized().body(e.to_string()),
    };

    // Build the query based on whether image_type filter is provided
    let images = if let Some(image_type) = &query.image_type {
        sqlx::query_as!(
            models::Image,
            "SELECT id, user_id, filename, content_type, image_type, image_url, created_at, updated_at 
             FROM images 
             WHERE user_id = $1 AND image_type = $2
             ORDER BY created_at DESC",
            user_id,
            image_type
        )
        .fetch_all(&**pool)
        .await
    } else {
        sqlx::query_as!(
            models::Image,
            "SELECT id, user_id, filename, content_type, image_type, image_url, created_at, updated_at 
             FROM images 
             WHERE user_id = $1
             ORDER BY created_at DESC",
            user_id
        )
        .fetch_all(&**pool)
        .await
    };

    match images {
        Ok(images) => HttpResponse::Ok().json(images),
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed to fetch images: {}", e)),
    }
}

#[post("/pet")]
async fn update_pet(
    req: HttpRequest,
    data: web::Json<UpdatePetData>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    // Extract the user_id from the token
    let user_id = match extract_user_id_from_token(&req) {
        Ok(id) => id,
        Err(e) => return HttpResponse::Unauthorized().body(e.to_string()),
    };

    // Check if we're updating or creating a pet
    if let Some(pet_id) = data.id {
        // UPDATING: Verify the pet belongs to the user
        let pet_exists = match sqlx::query!(
            "SELECT COUNT(*) as count FROM pets WHERE id = $1 AND user_id = $2",
            pet_id,
            user_id
        )
        .fetch_one(&**pool)
        .await {
            Ok(result) => result.count.unwrap_or(0) > 0,
            Err(e) => return HttpResponse::InternalServerError().body(format!("Database error: {}", e)),
        };

        if !pet_exists {
            return HttpResponse::NotFound().body("Pet not found or does not belong to you");
        }

        // Update the pet
        match sqlx::query_as!(
            Pet,
            r#"
            UPDATE pets
            SET 
                name = COALESCE($1, name),
                breed = COALESCE($2, breed),
                sex = COALESCE($3, sex),
                birthday = COALESCE($4, birthday),
                pet_image_url = COALESCE($5, pet_image_url),
                color = COALESCE($6, color),
                species = COALESCE($7, species),
                spayed_neutered = COALESCE($8, spayed_neutered),
                weight = COALESCE($9, weight),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $10 AND user_id = $11
            RETURNING id, user_id, name, breed, sex, birthday, pet_image_url, color, species, spayed_neutered, weight
            "#,
            data.name,
            data.breed,
            data.sex,
            data.birthday,
            data.pet_image_url,
            data.color,
            data.species,
            data.spayed_neutered,
            data.weight,
            pet_id,
            user_id
        )
        .fetch_one(&**pool)
        .await {
            Ok(updated_pet) => HttpResponse::Ok().json(json!({
                "message": "Pet updated successfully",
                "pet": updated_pet
            })),
            Err(e) => HttpResponse::InternalServerError().body(format!("Failed to update pet: {}", e)),
        }
    } else {
        // CREATING: Validate required fields for new pet
        if data.name.is_none() || data.breed.is_none() || data.sex.is_none() || data.birthday.is_none() {
            return HttpResponse::BadRequest().body("Name, breed, sex, and birthday are required when creating a new pet");
        }

        // Create a new pet
        match sqlx::query_as!(
            Pet,
            r#"
            INSERT INTO pets (user_id, name, breed, sex, birthday, pet_image_url, color, species, spayed_neutered, weight)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, user_id, name, breed, sex, birthday, pet_image_url, color, species, spayed_neutered, weight
            "#,
            user_id,
            data.name,
            data.breed,
            data.sex,
            data.birthday,
            data.pet_image_url,
            data.color,
            data.species,
            data.spayed_neutered,
            data.weight
        )
        .fetch_one(&**pool)
        .await {
            Ok(new_pet) => HttpResponse::Created().json(json!({
                "message": "Pet created successfully",
                "pet": new_pet
            })),
            Err(e) => HttpResponse::InternalServerError().body(format!("Failed to create pet: {}", e)),
        }
    }
}

#[delete("/pet")]
async fn delete_pet(
    req: HttpRequest,
    data: web::Json<DeletePetData>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    // Extract the user_id from the token
    let user_id = match extract_user_id_from_token(&req) {
        Ok(id) => id,
        Err(e) => return HttpResponse::Unauthorized().body(e.to_string()),
    };

    // First verify the pet belongs to the user
    let _pet = match sqlx::query!(
        "SELECT id, name FROM pets WHERE id = $1 AND user_id = $2",
        data.id,
        user_id
    )
    .fetch_optional(&**pool)
    .await {
        Ok(Some(pet)) => pet,
        Ok(None) => return HttpResponse::NotFound().body("Pet not found or does not belong to you"),
        Err(e) => return HttpResponse::InternalServerError().body(format!("Database error: {}", e)),
    };

    // Delete the pet
    match sqlx::query!(
        "DELETE FROM pets WHERE id = $1 AND user_id = $2",
        data.id,
        user_id
    )
    .execute(&**pool)
    .await {
        Ok(_) => HttpResponse::Ok().json(json!({
            "message": "Pet deleted successfully",
            "pet_id": data.id
        })),
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed to delete pet: {}", e)),
    }
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
            .app_data(web::Data::new(ws_server.clone()))
            .service(register)
            .service(request_verification_code)
            .service(login)
            .service(refresh)
            .service(logout)
            .service(get_profiles)
            .service(update_profile)
            .service(delete_account)
            .service(upload_image)
            .service(get_images)
            .service(update_pet)
            .service(delete_pet)
            .service(websocket_route)
    })
    // .bind(("127.0.0.1", 8080))?
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}

