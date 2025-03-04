use actix::prelude::*; // Import Actix prelude for common traits and functionalities
use actix_web::{post, web, App, HttpRequest, HttpResponse, HttpServer, Responder, get};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use chrono::{Utc};
use uuid::Uuid;
use actix_multipart::Multipart;
use futures::{StreamExt, TryStreamExt};
use std::io::Write;
use std::path::Path;
use std::fs;
use std::process::Command;

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
    RefreshData, LogoutData, RefreshToken, UpdateProfileData, ProfilesQuery, User, DeleteUserData,
    Pet, GetImagesQuery, UploadImageQuery
};
use crate::websockets::websocket_route; // Import the WebSocket route handler

// Google Cloud Storage upload
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::upload::{UploadObjectRequest, UploadType, Media};
use google_cloud_default::WithAuthExt;

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
                pet_data.birthday.unwrap_or(Utc::now()),
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
        Err(e) => return HttpResponse::Unauthorized().body(e.to_string()),
    };

    // Validate image type
    let image_type = match &query.image_type {
        Some(image_type) if ["profile", "pet"].contains(&image_type.as_str()) => image_type.clone(),
        Some(_) => return HttpResponse::BadRequest().body("Invalid image_type. Must be 'profile' or 'pet'"),
        None => return HttpResponse::BadRequest().body("Missing image_type parameter"),
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
            None => continue,
        };
        
        if let Some(name) = content_disposition.get_name() {
            if name == "file" {
                // Get the filename
                if let Some(fname) = content_disposition.get_filename() {
                    filename = Some(fname.to_string());
                    
                    // Get the content type
                    if let Some(ct) = field.content_type() {
                        content_type = Some(ct.to_string());
                    }
                    
                    // Read the file data
                    let mut data = Vec::new();
                    while let Some(chunk) = field.next().await {
                        match chunk {
                            Ok(bytes) => {
                                data.extend_from_slice(&bytes);
                            },
                            Err(e) => {
                                return HttpResponse::InternalServerError()
                                    .body(format!("Error reading file: {}", e));
                            }
                        }
                    }
                    
                    image_data = Some(data);
                }
            }
        }
    }
    
    // Check if we have the image data
    let image_bytes = match image_data {
        Some(data) => data,
        None => return HttpResponse::BadRequest().body("No image file provided"),
    };
    
    // Check file type
    let file_ext = match filename {
        Some(ref name) => Path::new(name)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("unknown"),
        None => "unknown",
    };
    
    // Validate file type
    if !["jpg", "jpeg", "png", "gif"].contains(&file_ext) {
        return HttpResponse::BadRequest().body("Invalid file type. Only jpg, jpeg, png, and gif are allowed.");
    }
    
    // Create a temporary directory if it doesn't exist
    let temp_dir = "temp_uploads";
    if !Path::new(temp_dir).exists() {
        match fs::create_dir(temp_dir) {
            Ok(_) => {},
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .body(format!("Failed to create temp directory: {}", e));
            }
        }
    }
    
    // Save the file temporarily
    let temp_path = format!("{}/{}.{}", temp_dir, image_id, file_ext);
    let mut file = match fs::File::create(&temp_path) {
        Ok(f) => f,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .body(format!("Failed to create temp file: {}", e));
        }
    };
    
    match file.write_all(&image_bytes) {
        Ok(_) => {},
        Err(e) => {
            return HttpResponse::InternalServerError()
                .body(format!("Failed to write to temp file: {}", e));
        }
    }
    
    // Initialize GCS client with credentials from environment variable or default
    let client_config = match std::env::var("GCS_CREDENTIALS") {
        Ok(credentials_path) => {
            // Use explicit credentials file if provided
            // Set the credentials file path as an environment variable
            std::env::set_var("GOOGLE_APPLICATION_CREDENTIALS", &credentials_path);
            println!("Using credentials from: {}", credentials_path);
            ClientConfig::default()
        },
        Err(_) => {
            // Fall back to default credentials (from environment)
            ClientConfig::default()
        }
    };

    // Apply authentication to the client config
    let client_config = match client_config.with_auth().await {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error setting up GCS authentication: {}", e);
            return HttpResponse::InternalServerError().body(format!("Failed to authenticate with Google Cloud Storage: {}", e));
        }
    };

    // Now create the client with the authenticated config
    let gcs_client = Client::new(client_config);

    // Get bucket name from environment variable
    let bucket_name = std::env::var("GCS_BUCKET_NAME").unwrap_or_else(|_| "vet-text-1".to_string());

    // Create upload request
    let mut upload_request = UploadObjectRequest::default();
    upload_request.bucket = bucket_name.clone();

    // Define object path based on image type and user ID
    let object_name = format!(
        "{}/{}/{}.{}",
        image_type,
        user_id,
        image_id,
        file_ext
    );

    // Set content type if available
    let content_type_str = content_type.clone().unwrap_or_else(|| {
        match file_ext {
            "jpg" | "jpeg" => "image/jpeg".to_string(),
            "png" => "image/png".to_string(),
            "gif" => "image/gif".to_string(),
            _ => "application/octet-stream".to_string(),
        }
    });

    // Create a Media object for the content type
    let media = Media::new(content_type_str);

    // Upload the file to GCS
    println!("Attempting to upload file to GCS bucket: {}, path: {}", bucket_name, object_name);

    // Debug: print service account info before upload attempt
    let debug_cmd = Command::new("curl")
        .arg("-s")
        .arg("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/email")
        .arg("-H")
        .arg("Metadata-Flavor: Google")
        .output();

    match debug_cmd {
        Ok(output) => {
            let email = String::from_utf8_lossy(&output.stdout);
            println!("Using service account: {}", email);
            
            // Also check if we can list the bucket to verify permissions
            println!("Checking if we can list the bucket...");
            let list_cmd = Command::new("gsutil")
                .arg("ls")
                .arg(format!("gs://{}", bucket_name))
                .output();
                
            match list_cmd {
                Ok(list_output) => {
                    if list_output.status.success() {
                        println!("Successfully listed bucket contents");
                    } else {
                        println!("Failed to list bucket: {}", 
                            String::from_utf8_lossy(&list_output.stderr));
                    }
                },
                Err(e) => println!("Error running gsutil: {}", e)
            }
        },
        Err(e) => println!("Failed to get service account: {}", e)
    };

    let upload_result = gcs_client.upload_object(
        &upload_request, 
        object_name.clone(),  // Object name as a separate parameter
        &UploadType::Simple(media)
    ).await;

    // Debug result
    match &upload_result {
        Ok(_) => println!("GCS upload successful for {}", object_name),
        Err(e) => {
            eprintln!("GCS upload error details:");
            eprintln!("- Error type: {:?}", e);
            eprintln!("- Error display: {}", e);
            
            // Try to extract more info from the error
            let error_string = format!("{:?}", e);
            if error_string.contains("status code: 403") {
                eprintln!("- This is a permissions error (403 Forbidden)");
            } else if error_string.contains("status code: 404") {
                eprintln!("- This is a not found error (404 Not Found) - check bucket name");
            }
            
            // Check bucket name case sensitivity
            eprintln!("- Using bucket name: '{}' (check case sensitivity)", bucket_name);
            eprintln!("- Object path: '{}'", object_name);
        }
    }
    
    // Clean up the temporary file
    fs::remove_file(&temp_path).ok();
    
    let image_url = match upload_result {
        Ok(_) => {
            // Generate a public URL for the uploaded image
            format!(
                "https://storage.googleapis.com/{}/{}",
                bucket_name,
                object_name
            )
        },
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Failed to upload image to GCS: {}", e));
        }
    };
    
    // Store the image metadata in the database
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
        Ok(_) => HttpResponse::Ok().json(json!({
            "message": "Image uploaded successfully",
            "image_id": image_id,
            "image_url": image_url
        })),
        Err(e) => {
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
            .service(websocket_route)
    })
    // .bind(("127.0.0.1", 8080))?
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}

