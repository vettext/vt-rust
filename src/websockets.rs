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
use std::error::Error;

// -----------------------
// Define Messages
// -----------------------

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct BroadcastMessage(pub WsMessage);

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
}

impl WsServer {
    pub fn new() -> Self {
        WsServer {
            sessions: HashMap::new(),
        }
    }

    pub fn broadcast_message(&self, message: &WsMessage) {
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
        println!("User {} disconnected", msg.id);
    }
}

impl Handler<BroadcastMessage> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: BroadcastMessage, _: &mut Context<Self>) {
        self.broadcast_message(&msg.0);
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
            .then(|_res, _act, _ctx| fut::ready(()))
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
                
                match serde_json::from_str::<WsMessage>(&text) {
                    Ok(ws_message) => {
                        match ws_message.event.as_str() {
                            "conversations" => {
                                let db_pool = self.db_pool.clone();
                                let user_id = self.id;
                                let addr = ctx.address();
                                let future = async move {
                                    let conversations_future = ConversationService::get_conversations_by_client_id(&db_pool, user_id);
                                    match conversations_future.await {
                                        Ok(conversations) => {
                                            addr.do_send(BroadcastMessage(WsMessage {
                                                sender_id: Uuid::nil(),
                                                event: "conversations".to_string(),
                                                data: serde_json::json!(conversations),
                                            }));
                                        },
                                        Err(e) => {
                                            println!("Error fetching conversations: {:?}", e);
                                            addr.do_send(BroadcastMessage(WsMessage {
                                                sender_id: Uuid::nil(),
                                                event: "error".to_string(),
                                                data: serde_json::json!({
                                                    "message": format!("Error fetching conversations: {:?}", e)
                                                }),
                                            }));
                                        }
                                    }
                                };
                                ctx.spawn(wrap_future(future));
                            },
                            "message" => {
                                if let Ok(WsEvent::Message { conversation_id, content }) = serde_json::from_value(ws_message.data) {
                                    let db_pool = self.db_pool.clone();
                                    let sender_id = ws_message.sender_id;
                                    let addr = ctx.address();
                                    let timestamp = Utc::now();
                                    let future = async move {
                                        let result = ConversationService::send_message(
                                            &db_pool,
                                            sender_id,
                                            conversation_id,
                                            content,
                                            timestamp,
                                        ).await;

                                        match result {
                                            Ok(message) => {
                                                addr.do_send(BroadcastMessage(WsMessage {
                                                    sender_id: Uuid::nil(),
                                                    event: "message_sent".to_string(),
                                                    data: serde_json::json!(message),
                                                }));
                                            },
                                            Err(e) => {
                                                println!("Error sending message: {:?}", e);
                                                addr.do_send(BroadcastMessage(WsMessage {
                                                    sender_id: Uuid::nil(),
                                                    event: "error".to_string(),
                                                    data: serde_json::json!({
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
                                if let Ok(WsEvent::NewConversation { pet_id, providers }) = serde_json::from_value(ws_message.data) {
                                    let db_pool = self.db_pool.clone();
                                    let client_id = self.id;
                                    let addr = ctx.address();
                                    let future = async move {
                                        let result = ConversationService::create_conversation(
                                            &db_pool,
                                            providers.unwrap_or_default(),
                                            client_id,
                                            pet_id
                                        ).await;

                                        match result {
                                            Ok(conversation) => {
                                                addr.do_send(BroadcastMessage(WsMessage {
                                                    sender_id: Uuid::nil(),
                                                    event: "conversation_created".to_string(),
                                                    data: serde_json::json!(conversation),
                                                }));
                                            },
                                            Err(e) => {
                                                println!("Error creating conversation: {:?}", e);
                                                addr.do_send(BroadcastMessage(WsMessage {
                                                    sender_id: Uuid::nil(),
                                                    event: "error".to_string(),
                                                    data: serde_json::json!({
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
                                if let Ok(WsEvent::ConversationHistory { conversation_id, page, limit }) = serde_json::from_value(ws_message.data) {
                                    let response = handle_ws_event(WsEvent::ConversationHistory { conversation_id, page, limit }, conversation_id).await;
                                    match response {
                                        Ok(value) => {
                                            addr.do_send(BroadcastMessage(WsMessage {
                                                sender_id: Uuid::nil(),
                                                event: "conversation_history_response".to_string(),
                                                data: value,
                                            }));
                                        },
                                        Err(e) => {
                                            println!("Error handling conversation history: {:?}", e);
                                            addr.do_send(BroadcastMessage(WsMessage {
                                                sender_id: Uuid::nil(),
                                                event: "error".to_string(),
                                                data: serde_json::json!({
                                                    "message": format!("Error handling conversation history: {:?}", e)
                                                }),
                                            }));
                                        }
                                    }
                                } else {
                                    ctx.text("Invalid conversation history data format");
                                }
                            },
                            _ => {
                                ctx.text("Unknown event type");
                            }
                        }
                    },
                    Err(e) => {
                        println!("Failed to parse message: {:?}", e);
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

async fn handle_ws_event(event: WsEvent, user_id: Uuid) -> Result<serde_json::Value, Error> {
    match event {
        WsEvent::ConversationHistory { conversation_id, page, limit } => {
            // For now, return dummy data
            let dummy_messages = (0..limit).map(|i| Message {
                id: Uuid::new_v4(),
                conversation_id,
                content: format!("Test message {}", i),
                timestamp: Utc::now() - chrono::Duration::hours(i as i64),
            }).collect();

            let response = ConversationHistoryResponse {
                messages: dummy_messages,
                total_count: 100, // Dummy total count
                has_more: page * limit < 100, // Dummy logic for has_more
            };

            Ok(serde_json::to_value(response)?)
        },
        _ => {
            // Handle other event types
            Ok(serde_json::Value::Null)
        }
    }
}
