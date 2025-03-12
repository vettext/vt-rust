use actix::prelude::*; // Brings in Actor, StreamHandler, Handler, Message, etc.
use actix::WrapFuture; // Needed for `into_actor`
use actix::ActorContext; // Needed for `ctx.stop()`
use actix_web::{HttpRequest, Responder, get};
use actix_web_actors::ws;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use uuid::Uuid;
use crate::models::{WsMessage, WsEvent, ConversationHistoryResponse};
use crate::services::conversations::ConversationService;
use sqlx::PgPool;
use actix_web::web;
use actix::fut::wrap_future;
use chrono::Utc;
use std::collections::HashSet;

// -----------------------
// Define Messages
// -----------------------

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct BroadcastMessage(pub WsMessage);

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct BroadcastToConversation {
    pub message: WsMessage,
    pub conversation_id: Uuid,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct SubscribeToConversation {
    pub user_id: Uuid,
    pub conversation_id: Uuid,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct UnsubscribeFromConversation {
    pub user_id: Uuid,
    pub conversation_id: Uuid,
}

// Messages for connecting and disconnecting
#[derive(Message)]
#[rtype(result = "()")]
pub struct Connect {
    pub addr: Recipient<BroadcastMessage>,
    pub id: Uuid,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub id: Uuid,
}

// -----------------------
// Define WebSocket Server Actor
// -----------------------

pub struct WsServer {
    sessions: HashMap<Uuid, Recipient<BroadcastMessage>>,
    conversation_subscriptions: HashMap<Uuid, HashSet<Uuid>>, // conversation_id -> set of user_ids
}

impl WsServer {
    pub fn new() -> Self {
        WsServer {
            sessions: HashMap::new(),
            conversation_subscriptions: HashMap::new(),
        }
    }

    // Subscribe a user to a conversation
    pub fn subscribe_to_conversation(&mut self, user_id: Uuid, conversation_id: Uuid) {
        println!("User {} subscribed to conversation {}", user_id, conversation_id);
        self.conversation_subscriptions
            .entry(conversation_id)
            .or_insert_with(HashSet::new)
            .insert(user_id);
    }

    // Unsubscribe a user from a conversation
    pub fn unsubscribe_from_conversation(&mut self, user_id: Uuid, conversation_id: Uuid) {
        println!("User {} unsubscribed from conversation {}", user_id, conversation_id);
        if let Some(subscribers) = self.conversation_subscriptions.get_mut(&conversation_id) {
            subscribers.remove(&user_id);
            if subscribers.is_empty() {
                self.conversation_subscriptions.remove(&conversation_id);
            }
        }
    }

    // Broadcast to specific conversation
    pub fn broadcast_to_conversation(&self, message: &WsMessage, conversation_id: Uuid) {
        println!("Broadcasting to conversation {}: {:?}", conversation_id, message.event);
        if let Some(subscribers) = self.conversation_subscriptions.get(&conversation_id) {
            for user_id in subscribers {
                if let Some(recipient) = self.sessions.get(user_id) {
                    let _ = recipient.do_send(BroadcastMessage(message.clone()));
                }
            }
        }
    }

    // Keep the general broadcast for system messages
    pub fn broadcast_message(&self, message: &WsMessage) {
        println!("Broadcasting to all users: {:?}", message.event);
        for recipient in self.sessions.values() {
            let _ = recipient.do_send(BroadcastMessage(message.clone()));
        }
    }
}

impl Actor for WsServer {
    type Context = Context<Self>;
}

impl Handler<Connect> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: Connect, _: &mut Context<Self>) {
        self.sessions.insert(msg.id, msg.addr);
        println!("User {} connected", msg.id);
    }
}

impl Handler<Disconnect> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Context<Self>) {
        self.sessions.remove(&msg.id);
        
        // Remove user from all conversation subscriptions
        for subscribers in self.conversation_subscriptions.values_mut() {
            subscribers.remove(&msg.id);
        }
        
        // Clean up empty conversation subscriptions
        self.conversation_subscriptions.retain(|_, subscribers| !subscribers.is_empty());
        
        println!("User {} disconnected", msg.id);
    }
}

impl Handler<BroadcastMessage> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: BroadcastMessage, _: &mut Context<Self>) {
        self.broadcast_message(&msg.0);
    }
}

impl Handler<BroadcastToConversation> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: BroadcastToConversation, _: &mut Context<Self>) {
        self.broadcast_to_conversation(&msg.message, msg.conversation_id);
    }
}

impl Handler<SubscribeToConversation> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: SubscribeToConversation, _: &mut Context<Self>) {
        self.subscribe_to_conversation(msg.user_id, msg.conversation_id);
    }
}

impl Handler<UnsubscribeFromConversation> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: UnsubscribeFromConversation, _: &mut Context<Self>) {
        self.unsubscribe_from_conversation(msg.user_id, msg.conversation_id);
    }
}

// -----------------------
// Define WebSocket Session Actor
// -----------------------

pub struct WsSession {
    pub id: Uuid,
    pub addr: Addr<WsServer>,
    pub db_pool: web::Data<PgPool>,
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    // Called when the actor starts
    fn started(&mut self, ctx: &mut Self::Context) {
        // Register self in the server
        self.addr
            .send(Connect {
                addr: ctx.address().recipient(),
                id: self.id,
            })
            .into_actor(self)
            .then(|_res, act, _ctx| {
                // Auto-subscribe to all conversations the user is part of
                let db_pool = act.db_pool.clone();
                let user_id = act.id;
                let addr = act.addr.clone();
                
                async move {
                    // First, determine the user's role
                    let user_role = match sqlx::query!(
                        "SELECT scope FROM users WHERE id = $1",
                        user_id
                    )
                    .fetch_optional(&**db_pool)
                    .await {
                        Ok(Some(record)) => record.scope,
                        _ => "unknown".to_string(),
                    };
                    
                    // Subscribe to conversations based on role
                    match user_role.as_str() {
                        "client" => {
                            // Subscribe to client conversations
                            if let Ok(conversations) = ConversationService::get_conversations_by_client_id(&db_pool, user_id).await {
                                for conversation in conversations {
                                    addr.do_send(SubscribeToConversation {
                                        user_id,
                                        conversation_id: conversation.id,
                                    });
                                }
                            }
                        },
                        "provider" => {
                            // Subscribe to provider conversations
                            if let Ok(conversations) = ConversationService::get_conversations_by_provider_id(&db_pool, user_id).await {
                                for conversation in conversations {
                                    addr.do_send(SubscribeToConversation {
                                        user_id,
                                        conversation_id: conversation.id,
                                    });
                                }
                            }
                        },
                        _ => {
                            println!("Unknown user role: {}", user_role);
                        }
                    }
                }
                .into_actor(act)
            })
            .wait(ctx);
    }

    // Called when the actor stops
    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        // Unregister self from the server
        self.addr.do_send(Disconnect { id: self.id });
        Running::Stop
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut ws::WebsocketContext<Self>) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Text(text)) => {
                println!("Received message from user {}: {}", self.id, text);
                
                // Log the raw incoming message for debugging
                println!("Raw WebSocket message: {}", text);
                
                match serde_json::from_str::<WsMessage>(&text) {
                    Ok(ws_message) => {
                        println!("Successfully parsed WebSocket message: {:?}", ws_message);
                        // Process based on event type
                        match ws_message.event.as_str() {
                            "conversations" => {
                                let db_pool = self.db_pool.clone();
                                let user_id = self.id;
                                let addr = ctx.address();
                                let future = async move {
                                    // First, determine the user's role
                                    let user_role = match sqlx::query!(
                                        "SELECT scope FROM users WHERE id = $1",
                                        user_id
                                    )
                                    .fetch_optional(&**db_pool)
                                    .await {
                                        Ok(Some(record)) => record.scope,
                                        _ => "unknown".to_string(),
                                    };

                                    let conversations = match user_role.as_str() {
                                        "client" => {
                                            // Fetch client conversations
                                            match ConversationService::get_conversations_by_client_id(&db_pool, user_id).await {
                                                Ok(convs) => convs,
                                                Err(e) => {
                                                    println!("Error fetching client conversations: {:?}", e);
                                                    Vec::new()
                                                }
                                            }
                                        },
                                        "provider" => {
                                            // Fetch provider conversations
                                            match ConversationService::get_conversations_by_provider_id(&db_pool, user_id).await {
                                                Ok(convs) => convs,
                                                Err(e) => {
                                                    println!("Error fetching provider conversations: {:?}", e);
                                                    Vec::new()
                                                }
                                            }
                                        },
                                        _ => {
                                            println!("Unknown user role: {}", user_role);
                                            Vec::new()
                                        },
                                    };

                                    // Sort by last_updated_timestamp (newest first)
                                    let mut sorted_conversations = conversations;
                                    sorted_conversations.sort_by(|a, b| b.last_updated_timestamp.cmp(&a.last_updated_timestamp));

                                    addr.do_send(BroadcastMessage(WsMessage {
                                        sender_id: Uuid::nil(),
                                        event: "conversations".to_string(),
                                        params: serde_json::json!(sorted_conversations),
                                    }));
                                };
                                ctx.spawn(wrap_future(future));
                            },
                            "message" => {
                                if let Ok(WsEvent::Message { conversation_id, content }) = serde_json::from_value(ws_message.params) {
                                    let db_pool = self.db_pool.clone();
                                    let sender_id = ws_message.sender_id;
                                    let addr = self.addr.clone();
                                    let user_id = self.id;
                                    let timestamp = Utc::now();
                                    let future = async move {
                                        // Check if the user is part of this conversation
                                        let user_role = match sqlx::query!(
                                            "SELECT scope FROM users WHERE id = $1",
                                            user_id
                                        )
                                        .fetch_optional(&**db_pool)
                                        .await {
                                            Ok(Some(record)) => record.scope,
                                            _ => "unknown".to_string(),
                                        };
                                        
                                        let can_send = match user_role.as_str() {
                                            "client" => {
                                                // Check if client is part of this conversation
                                                match sqlx::query!(
                                                    "SELECT id FROM conversations WHERE id = $1 AND client = $2",
                                                    conversation_id,
                                                    user_id
                                                )
                                                .fetch_optional(&**db_pool)
                                                .await {
                                                    Ok(Some(_)) => true,
                                                    _ => false,
                                                }
                                            },
                                            "provider" => {
                                                // Check if provider is part of this conversation
                                                match sqlx::query!(
                                                    "SELECT id FROM conversations WHERE id = $1 AND $2 = ANY(providers)",
                                                    conversation_id,
                                                    user_id
                                                )
                                                .fetch_optional(&**db_pool)
                                                .await {
                                                    Ok(Some(_)) => true,
                                                    _ => false,
                                                }
                                            },
                                            _ => false,
                                        };
                                        
                                        if !can_send {
                                            addr.do_send(BroadcastMessage(WsMessage {
                                                sender_id: Uuid::nil(),
                                                event: "error".to_string(),
                                                params: serde_json::json!({
                                                    "message": "You are not authorized to send messages in this conversation"
                                                }),
                                            }));
                                            return;
                                        }
                                        
                                        // First, ensure the user is subscribed to this conversation
                                        addr.do_send(SubscribeToConversation {
                                            user_id,
                                            conversation_id,
                                        });
                                        
                                        let result = ConversationService::send_message(
                                            &db_pool,
                                            sender_id,
                                            conversation_id,
                                            content,
                                            timestamp,
                                        ).await;

                                        match result {
                                            Ok(message) => {
                                                addr.do_send(BroadcastToConversation {
                                                    message: WsMessage {
                                                        sender_id: Uuid::nil(),
                                                        event: "message_sent".to_string(),
                                                        params: serde_json::json!(message),
                                                    },
                                                    conversation_id,
                                                });
                                            },
                                            Err(e) => {
                                                println!("Error sending message: {:?}", e);
                                                addr.do_send(BroadcastMessage(WsMessage {
                                                    sender_id: Uuid::nil(),
                                                    event: "error".to_string(),
                                                    params: serde_json::json!({
                                                        "message": format!("Error sending message: {:?}", e)
                                                    }),
                                                }));
                                            }
                                        }
                                    };
                                    ctx.spawn(wrap_future(future));
                                } else {
                                    ctx.text("Invalid message data format");
                                }
                            },
                            "new_conversation" => {
                                if let Ok(WsEvent::NewConversation { pet_id, providers }) = serde_json::from_value(ws_message.params) {
                                    let db_pool = self.db_pool.clone();
                                    let user_id = self.id;
                                    let addr = self.addr.clone();
                                    let future = async move {
                                        // Check if the user is a client (only clients can create conversations)
                                        let user_role = match sqlx::query!(
                                            "SELECT scope FROM users WHERE id = $1",
                                            user_id
                                        )
                                        .fetch_optional(&**db_pool)
                                        .await {
                                            Ok(Some(record)) => record.scope,
                                            _ => "unknown".to_string(),
                                        };
                                        
                                        if user_role != "client" {
                                            addr.do_send(BroadcastMessage(WsMessage {
                                                sender_id: Uuid::nil(),
                                                event: "error".to_string(),
                                                params: serde_json::json!({
                                                    "message": "Only clients can create conversations"
                                                }),
                                            }));
                                            return;
                                        }
                                        
                                        let result = ConversationService::create_conversation(
                                            &db_pool,
                                            providers.clone().unwrap_or_default(),
                                            user_id,
                                            pet_id
                                        ).await;

                                        match result {
                                            Ok(conversation) => {
                                                // Subscribe the client to the new conversation
                                                addr.do_send(SubscribeToConversation {
                                                    user_id,
                                                    conversation_id: conversation.id,
                                                });
                                                
                                                // Subscribe all providers to the conversation
                                                if let Some(ref provider_ids) = providers {
                                                    for _provider_id in provider_ids {
                                                        addr.do_send(SubscribeToConversation {
                                                            user_id: *_provider_id,
                                                            conversation_id: conversation.id,
                                                        });
                                                    }
                                                }
                                                
                                                // Notify the client about the new conversation
                                                addr.do_send(BroadcastMessage(WsMessage {
                                                    sender_id: Uuid::nil(),
                                                    event: "conversation_created".to_string(),
                                                    params: serde_json::json!(conversation),
                                                }));
                                                
                                                // Notify all providers about the new conversation
                                                if let Some(ref provider_ids) = providers {
                                                    for _provider_id in provider_ids {
                                                        addr.do_send(BroadcastToConversation {
                                                            message: WsMessage {
                                                                sender_id: Uuid::nil(),
                                                                event: "new_conversation_invitation".to_string(),
                                                                params: serde_json::json!(conversation.clone()),
                                                            },
                                                            conversation_id: conversation.id,
                                                        });
                                                    }
                                                }
                                            },
                                            Err(e) => {
                                                println!("Error creating conversation: {:?}", e);
                                                addr.do_send(BroadcastMessage(WsMessage {
                                                    sender_id: Uuid::nil(),
                                                    event: "error".to_string(),
                                                    params: serde_json::json!({
                                                        "message": format!("Error creating conversation: {:?}", e)
                                                    }),
                                                }));
                                            }
                                        }
                                    };
                                    ctx.spawn(wrap_future(future));
                                } else {
                                    ctx.text("Invalid new conversation data format");
                                }
                            },
                            "conversation_history" => {
                                if let Ok(WsEvent::ConversationHistory { conversation_id, page, limit }) = serde_json::from_value(ws_message.params) {
                                    let addr = ctx.address();
                                    let user_id = self.id;
                                    let server_addr = self.addr.clone();
                                    let db_pool = self.db_pool.clone();
                                    
                                    let future = async move {
                                        // Check if the user is part of this conversation
                                        let user_role = match sqlx::query!(
                                            "SELECT scope FROM users WHERE id = $1",
                                            user_id
                                        )
                                        .fetch_optional(&**db_pool)
                                        .await {
                                            Ok(Some(record)) => record.scope,
                                            _ => "unknown".to_string(),
                                        };
                                        
                                        let can_access = match user_role.as_str() {
                                            "client" => {
                                                // Check if client is part of this conversation
                                                match sqlx::query!(
                                                    "SELECT id FROM conversations WHERE id = $1 AND client = $2",
                                                    conversation_id,
                                                    user_id
                                                )
                                                .fetch_optional(&**db_pool)
                                                .await {
                                                    Ok(Some(_)) => true,
                                                    _ => false,
                                                }
                                            },
                                            "provider" => {
                                                // Check if provider is part of this conversation
                                                match sqlx::query!(
                                                    "SELECT id FROM conversations WHERE id = $1 AND $2 = ANY(providers)",
                                                    conversation_id,
                                                    user_id
                                                )
                                                .fetch_optional(&**db_pool)
                                                .await {
                                                    Ok(Some(_)) => true,
                                                    _ => false,
                                                }
                                            },
                                            _ => false,
                                        };
                                        
                                        if !can_access {
                                            addr.do_send(BroadcastMessage(WsMessage {
                                                sender_id: Uuid::nil(),
                                                event: "error".to_string(),
                                                params: serde_json::json!({
                                                    "message": "You are not authorized to access this conversation history"
                                                }),
                                            }));
                                            return;
                                        }
                                        
                                        // Subscribe to the conversation when requesting history
                                        server_addr.do_send(SubscribeToConversation {
                                            user_id,
                                            conversation_id,
                                        });
                                        
                                        // Fetch real messages from database
                                        match ConversationService::get_conversation_messages(
                                            &db_pool, conversation_id, page, limit
                                        ).await {
                                            Ok((messages, total_count, has_more)) => {
                                                let response = ConversationHistoryResponse {
                                                    messages,
                                                    total_count,
                                                    has_more,
                                                };
                                                
                                                addr.do_send(BroadcastMessage(WsMessage {
                                                    sender_id: Uuid::nil(),
                                                    event: "conversation_history_response".to_string(),
                                                    params: serde_json::to_value(response).unwrap_or(serde_json::Value::Null),
                                                }));
                                            },
                                            Err(e) => {
                                                println!("Error fetching conversation history: {:?}", e);
                                                addr.do_send(BroadcastMessage(WsMessage {
                                                    sender_id: Uuid::nil(),
                                                    event: "error".to_string(),
                                                    params: serde_json::json!({
                                                        "message": format!("Error fetching conversation history: {:?}", e)
                                                    }),
                                                }));
                                            }
                                        }
                                    };
                                    ctx.spawn(wrap_future(future));
                                } else {
                                    ctx.text("Invalid conversation history data format");
                                }
                            },
                            "subscribe_conversation" => {
                                if let Some(conversation_id) = ws_message.params.get("conversation_id") {
                                    if let Ok(conversation_id) = serde_json::from_value::<Uuid>(conversation_id.clone()) {
                                        self.addr.do_send(SubscribeToConversation {
                                            user_id: self.id,
                                            conversation_id,
                                        });
                                        
                                        // Fetch user profile data and notify others
                                        let user_id = self.id;
                                        let db_pool = self.db_pool.clone();
                                        let addr = self.addr.clone();
                                        
                                        let future = async move {
                                            // Get user profile data
                                            let user_profile = match sqlx::query!(
                                                "SELECT first_name, last_name, profile_image_url FROM users WHERE id = $1",
                                                user_id
                                            )
                                            .fetch_one(&**db_pool)
                                            .await {
                                                Ok(profile) => profile,
                                                Err(e) => {
                                                    println!("Error fetching user profile: {:?}", e);
                                                    return;
                                                }
                                            };
                                            
                                            // Create a display name from first and last name
                                            let display_name = match (user_profile.first_name.as_ref(), user_profile.last_name.as_ref()) {
                                                (Some(first), Some(last)) => format!("{} {}", first, last),
                                                (Some(first), None) => first.clone(),
                                                (None, Some(last)) => last.clone(),
                                                (None, None) => "Unknown User".to_string(),
                                            };
                                            
                                            // Send a system message to the conversation about the user joining
                                            addr.do_send(BroadcastToConversation {
                                                conversation_id,
                                                message: WsMessage {
                                                    sender_id: Uuid::nil(), // System message
                                                    event: "user_joined".to_string(),
                                                    params: serde_json::json!({
                                                        "user_id": user_id,
                                                        "display_name": display_name,
                                                        "profile_image_url": user_profile.profile_image_url,
                                                        "conversation_id": conversation_id,
                                                        "timestamp": Utc::now().timestamp_millis()
                                                    }),
                                                },
                                            });
                                        };
                                        
                                        ctx.spawn(wrap_future(future));
                                        
                                        ctx.text(serde_json::to_string(&WsMessage {
                                            sender_id: Uuid::nil(),
                                            event: "subscribed".to_string(),
                                            params: serde_json::json!({
                                                "conversation_id": conversation_id,
                                                "status": "success"
                                            }),
                                        }).unwrap());
                                    } else {
                                        ctx.text("Invalid conversation ID format");
                                    }
                                } else {
                                    ctx.text("Missing conversation_id parameter");
                                }
                            },
                            "unsubscribe_conversation" => {
                                if let Some(conversation_id) = ws_message.params.get("conversation_id") {
                                    if let Ok(conversation_id) = serde_json::from_value::<Uuid>(conversation_id.clone()) {
                                        self.addr.do_send(UnsubscribeFromConversation {
                                            user_id: self.id,
                                            conversation_id,
                                        });
                                        
                                        // Fetch user profile data and notify others about the user leaving
                                        let user_id = self.id;
                                        let db_pool = self.db_pool.clone();
                                        let addr = self.addr.clone();
                                        
                                        let future = async move {
                                            // Get user profile data
                                            let user_profile = match sqlx::query!(
                                                "SELECT first_name, last_name, profile_image_url FROM users WHERE id = $1",
                                                user_id
                                            )
                                            .fetch_one(&**db_pool)
                                            .await {
                                                Ok(profile) => profile,
                                                Err(e) => {
                                                    println!("Error fetching user profile: {:?}", e);
                                                    return;
                                                }
                                            };
                                            
                                            // Create a display name from first and last name
                                            let display_name = match (user_profile.first_name.as_ref(), user_profile.last_name.as_ref()) {
                                                (Some(first), Some(last)) => format!("{} {}", first, last),
                                                (Some(first), None) => first.clone(),
                                                (None, Some(last)) => last.clone(),
                                                (None, None) => "Unknown User".to_string(),
                                            };
                                            
                                            // Send a system message to the conversation about the user leaving
                                            addr.do_send(BroadcastToConversation {
                                                conversation_id,
                                                message: WsMessage {
                                                    sender_id: Uuid::nil(), // System message
                                                    event: "user_left".to_string(),
                                                    params: serde_json::json!({
                                                        "user_id": user_id,
                                                        "display_name": display_name,
                                                        "profile_image_url": user_profile.profile_image_url,
                                                        "conversation_id": conversation_id,
                                                        "timestamp": Utc::now().timestamp_millis(),
                                                        "reason": "unsubscribed"
                                                    }),
                                                },
                                            });
                                        };
                                        
                                        ctx.spawn(wrap_future(future));
                                        
                                        ctx.text(serde_json::to_string(&WsMessage {
                                            sender_id: Uuid::nil(),
                                            event: "unsubscribed".to_string(),
                                            params: serde_json::json!({
                                                "conversation_id": conversation_id,
                                                "status": "success"
                                            }),
                                        }).unwrap());
                                    } else {
                                        ctx.text("Invalid conversation ID format");
                                    }
                                } else {
                                    ctx.text("Missing conversation_id parameter");
                                }
                            },
                            _ => {
                                ctx.text("Unknown event type");
                            }
                        }
                    },
                    Err(e) => {
                        // Detailed error logging
                        println!("Failed to parse WebSocket message: {}", e);
                        println!("Message causing error: {}", text);
                        
                        // Try to determine what part is failing
                        if let Ok(raw_json) = serde_json::from_str::<serde_json::Value>(&text) {
                            println!("JSON is valid, but doesn't match WsMessage structure");
                            println!("Expected structure: sender_id (UUID), event (String), params (Object)");
                            println!("Received structure: {:?}", raw_json);
                            
                            // Check for specific missing fields
                            if !raw_json.get("sender_id").is_some() {
                                println!("Missing 'sender_id' field");
                            }
                            if !raw_json.get("event").is_some() {
                                println!("Missing 'event' field");
                            }
                            if !raw_json.get("params").is_some() {
                                println!("Missing 'params' field");
                            }
                        } else {
                            println!("JSON is invalid or malformed");
                        }
                        
                        ctx.text(format!("Invalid message format: {}", e));
                    }
                }
            }
            Ok(ws::Message::Binary(_)) => {
                ctx.text("Binary messages are not supported");
            }
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => (),
        }
    }
}

impl Handler<BroadcastMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: BroadcastMessage, ctx: &mut Self::Context) {
        ctx.text(serde_json::to_string(&msg.0).unwrap());
    }
}

// -----------------------
// Define WebSocket Route Handler
// -----------------------

#[get("/ws/")]
pub async fn websocket_route(
    req: HttpRequest,
    stream: actix_web::web::Payload,
    srv: actix_web::web::Data<Addr<WsServer>>,
    pool: web::Data<PgPool>,
) -> impl Responder {
    let user_id = Uuid::new_v4();

    ws::start(
        WsSession {
            id: user_id,
            addr: srv.get_ref().clone(),
            db_pool: pool,
        },
        &req,
        stream,
    )
}
