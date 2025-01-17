use hueclient::{Bridge, CommandLight};
use serde::{Deserialize, Serialize};
use rust_socketio::{
    client::ClientBuilder,
    payload::Payload,
    Event,
};
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::Duration;
use tokio::time::sleep;
use std::fs;
use hex::FromHex;
use log::{info, warn, error, debug};
use tokio::signal;
use tokio::sync::mpsc;
use thiserror::Error;
use serde_json::Value;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Socket.io error: {0}")]
    SocketIo(#[from] rust_socketio::Error),
    
    #[error("Bridge error: {0}")]
    Bridge(String),
    
    #[error("Invalid amount format: {0}")]
    InvalidAmount(String),
}

// Keep existing config structures
#[derive(Debug, Deserialize, Serialize, Clone)]
struct Config {
    credentials: Credentials,
    default_state: LightState,
    events: EventConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Credentials {
    streamlabs: StreamlabsCredentials,
    hue: HueCredentials,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct StreamlabsCredentials {
    socket_token: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct HueCredentials {
    username: String,
    bridge_ip: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct LightState {
    on: bool,
    brightness: u8,
    hue: u16,
    saturation: u8,
    alert: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct EventConfig {
    donation: EventTieredEffect,
    twitch_follow: SimpleEventEffect,
    twitch_subscription: SimpleEventEffect,
    twitch_bits: EventTieredEffect,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct SimpleEventEffect {
    enabled: bool,
    effect: LightEffect,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct EventTieredEffect {
    enabled: bool,
    tiers: Vec<TierEffect>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TierEffect {
    amount: f64,
    effect: LightEffect,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct LightEffect {
    color: String,
    brightness: u8,
    alert: String,
    duration: u64,
}

// Updated Streamlabs event structures
#[derive(Debug, Deserialize)]
struct StreamlabsEvent {
    event_id: String,
    #[serde(rename = "for")]
    event_for: Option<String>,
    #[serde(rename = "type")]
    event_type: String,
    message: Vec<EventMessage>,
}

#[derive(Debug, Deserialize)]
struct EventMessage {
    #[serde(default)]
    name: String,
    #[serde(default)]
    amount: Option<String>,
    #[serde(default)]
    formatted_amount: Option<String>,
    #[serde(default)]
    _id: String,
    #[serde(default)]
    id: Option<String>,
    payload: Option<EventPayload>,
}

#[derive(Debug, Deserialize)]
struct EventPayload {
    #[serde(default)]
    name: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    priority: Option<i32>,
}

struct AppState {
    bridge: Arc<Mutex<Bridge>>,
    config: Config,
}

// Existing hex_to_hue function remains the same
fn hex_to_hue(hex: &str) -> Result<(u16, u8), AppError> {
    let hex = hex.trim_start_matches('#');
    let rgb = Vec::from_hex(hex)
        .map_err(|e| AppError::Bridge(format!("Invalid hex color: {}", e)))?;
    
    if rgb.len() != 3 {
        return Err(AppError::Bridge("Invalid RGB values".to_string()));
    }
    
    let (r, g, b) = (rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0);
    
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    
    let hue = ((hue / 360.0) * 65535.0) as u16;
    let saturation = (saturation * 254.0) as u8;
    
    Ok((hue, saturation))
}

impl AppState {
    async fn handle_event(&self, event: StreamlabsEvent) -> Result<(), AppError> {
        info!("Processing event: {:?}", event);
        
        let result = match (event.event_type.as_str(), event.event_for.as_deref()) {
            ("donation", None) if self.config.events.donation.enabled => {
                info!("Handling donation event");
                if let Some(message) = event.message.first() {
                    self.handle_donation(message).await?;
                }
                Ok(())
            },
            ("follow", Some("twitch_account")) if self.config.events.twitch_follow.enabled => {
                info!("Handling Twitch follow event");
                self.handle_twitch_follow().await
            },
            ("subscription", Some("twitch_account")) if self.config.events.twitch_subscription.enabled => {
                info!("Handling Twitch subscription event");
                self.handle_twitch_subscription().await
            },
            ("bits", Some("twitch_account")) if self.config.events.twitch_bits.enabled => {
                info!("Handling Twitch bits event");
                if let Some(message) = event.message.first() {
                    self.handle_bits(message).await?;
                }
                Ok(())
            },
            _ => {
                info!("Unhandled or disabled event: type={}, for={:?}", 
                      event.event_type, event.event_for);
                Ok(())
            }
        };

        if let Err(e) = &result {
            error!("Error processing event: {}", e);
        }
        
        result
    }

    async fn handle_donation(&self, message: &EventMessage) -> Result<(), AppError> {
        if let Some(amount_str) = &message.amount {
            let amount: f64 = amount_str.parse()
                .map_err(|_| AppError::InvalidAmount(amount_str.clone()))?;
            
            let effect = self.config.events.donation.tiers
                .iter()
                .find(|tier| amount >= tier.amount)
                .map(|tier| &tier.effect)
                .unwrap_or_else(|| &self.config.events.donation.tiers.last().unwrap().effect);
            
            info!("Processing donation of {} from {}", amount_str, message.name);
            self.apply_effect(effect).await?;
        }
        
        Ok(())
    }

    async fn handle_twitch_follow(&self) -> Result<(), AppError> {
        info!("Processing Twitch follow");
        self.apply_effect(&self.config.events.twitch_follow.effect).await
    }

    async fn handle_twitch_subscription(&self) -> Result<(), AppError> {
        info!("Processing Twitch subscription");
        self.apply_effect(&self.config.events.twitch_subscription.effect).await
    }

    async fn handle_bits(&self, message: &EventMessage) -> Result<(), AppError> {
        if let Some(amount_str) = &message.amount {
            let amount: f64 = amount_str.parse()
                .map_err(|_| AppError::InvalidAmount(amount_str.clone()))?;
            
            let effect = self.config.events.twitch_bits.tiers
                .iter()
                .find(|tier| amount >= tier.amount)
                .map(|tier| &tier.effect)
                .unwrap_or_else(|| &self.config.events.twitch_bits.tiers.last().unwrap().effect);
            
            info!("Processing {} bits from {}", amount_str, message.name);
            self.apply_effect(effect).await?;
        }
        
        Ok(())
    }

    async fn apply_effect(&self, effect: &LightEffect) -> Result<(), AppError> {
        info!("Applying light effect: {:?}", effect);
        let bridge = self.bridge.lock();
        let (hue, sat) = hex_to_hue(&effect.color)?;
        
        let mut command = CommandLight::default();
        command.on = Some(true);
        command.bri = Some(effect.brightness);
        command.hue = Some(hue);
        command.sat = Some(sat);
        command.alert = Some(effect.alert.clone());

        info!("Created light command with hue={}, sat={}", hue, sat);
        
        let lights = bridge.get_all_lights()
            .map_err(|e| AppError::Bridge(e.to_string()))?;
            
        info!("Applying effect to {} lights", lights.len());
        for light in lights {
            info!("Setting state for light {}", light.id);
            bridge.set_light_state(light.id, &command)
                .map_err(|e| AppError::Bridge(e.to_string()))?;
        }

        info!("Waiting {} ms before resetting", effect.duration);
        sleep(Duration::from_millis(effect.duration)).await;
        
        info!("Resetting lights to default state");
        let mut reset_command = CommandLight::default();
        reset_command.on = Some(self.config.default_state.on);
        reset_command.bri = Some(self.config.default_state.brightness);
        reset_command.hue = Some(self.config.default_state.hue);
        reset_command.sat = Some(self.config.default_state.saturation);
        reset_command.alert = Some(self.config.default_state.alert.clone());

        let lights = bridge.get_all_lights()
            .map_err(|e| AppError::Bridge(e.to_string()))?;
            
        for light in lights {
            info!("Resetting light {}", light.id);
            bridge.set_light_state(light.id, &reset_command)
                .map_err(|e| AppError::Bridge(e.to_string()))?;
        }
        
        Ok(())
    }
}

fn process_event(message: &str, tx: &mpsc::Sender<StreamlabsEvent>) {
    info!("Processing message: {}", message);
    match serde_json::from_str::<StreamlabsEvent>(message) {
        Ok(event) => {
            info!("Successfully parsed event: {:?}", event);
            match (event.event_type.as_str(), event.event_for.as_deref()) {
                ("donation", None) |
                ("follow", Some("twitch_account")) |
                ("subscription", Some("twitch_account")) |
                ("bits", Some("twitch_account")) => {
                    info!("Sending valid event to handler: {:?}", event);
                    if let Err(e) = tx.blocking_send(event) {
                        error!("Failed to send event to handler: {}", e);
                    }
                }
                _ => {
                    debug!("Ignoring unhandled event type: {:?}", event);
                }
            }
        }
        Err(e) => {
            error!("Failed to parse Streamlabs event: {}", e);
            error!("Raw message that failed to parse: {}", message);
        }
    }
}

fn main() -> Result<(), AppError> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();
    
    info!("Starting LumiaLive...");
    
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        info!("Loading configuration...");
        let config: Config = match fs::read_to_string("config.json") {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => config,
                Err(e) => {
                    error!("Failed to parse config.json: {}", e);
                    return;
                }
            },
            Err(e) => {
                error!("Failed to read config.json: {}", e);
                return;
            }
        };

        let (event_tx, event_rx) = mpsc::channel::<StreamlabsEvent>(32);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create runtime");

        info!("Connecting to Streamlabs socket API...");
        let socket_url = format!(
            "https://sockets.streamlabs.com?token={}", 
            config.credentials.streamlabs.socket_token
        );
        
        info!("Using socket URL: {}", socket_url);

        let client = match ClientBuilder::new(&socket_url)
            .transport_type(rust_socketio::TransportType::Websocket)
            .on("connect", |_, _| {
                info!("Socket.IO connected successfully!");
            })
            .on("disconnect", |_, _| {
                warn!("Socket.IO disconnected!");
            })
            .on("connect_error", |err, _| {
                error!("Socket.IO connection error: {:?}", err);
            })
            .on("event", {
                let tx = event_tx.clone();
                move |payload: Payload, _| {
                    info!("Received raw socket event");
                    match payload {
                        Payload::String(message) => {
                            info!("Processing String payload: {}", message);
                            process_event(&message, &tx);
                        }
                        Payload::Text(json_value) => {
                            info!("Processing Text payload: {:?}", json_value);
                            if let Some(first_event) = json_value.first() {
                                if let Ok(message) = serde_json::to_string(first_event) {
                                    process_event(&message, &tx);
                                } else {
                                    error!("Failed to serialize JSON value to string");
                                }
                            } else {
                                error!("Text payload is not an array");
                            }
                        }
                        other => {
                            warn!("Received unexpected payload type: {:?}", other);
                        }
                    }
                }
            })
            .connect() {
                Ok(client) => client,
                Err(e) => {
                    error!("Failed to connect to Streamlabs: {}", e);
                    return;
                }
            };

        info!("Connected to Streamlabs!");

        // Initialize Hue bridge and state
        let state = rt.block_on(async {
            info!("Connecting to Hue bridge...");
            let bridge = if let Some(ip) = &config.credentials.hue.bridge_ip {
                info!("Using configured bridge IP: {}", ip);
                match Bridge::discover() {
                    Some(bridge) => bridge.with_user(&config.credentials.hue.username),
                    None => {
                        error!("Failed to discover bridge at configured IP");
                        return Err(AppError::Bridge("Failed to discover bridge".to_string()));
                    }
                }
            } else {
                info!("No bridge IP configured, discovering bridge...");
                Bridge::discover_required()
                    .with_user(&config.credentials.hue.username)
            };

            let bridge = Arc::new(Mutex::new(bridge));
            
            // Test bridge connection
            {
                let bridge_lock = bridge.lock();
                match bridge_lock.get_all_lights() {
                    Ok(lights) => info!("Successfully connected to bridge. Found {} lights", lights.len()),
                    Err(e) => {
                        error!("Failed to get lights from bridge: {}", e);
                        return Err(AppError::Bridge(format!("Failed to get lights: {}", e)));
                    }
                }
            }
            
            Ok::<_, AppError>(Arc::new(AppState {
                bridge: bridge.clone(),
                config: config.clone(),
            }))
        }).expect("Failed to initialize bridge");

        // Spawn event handler
        let _event_handler = {
            let state = state.clone();
            let mut event_rx = event_rx;
            let rt_handle = rt.handle().clone();
            rt.spawn_blocking(move || {
                info!("Event handler thread started");
                while let Some(event) = event_rx.blocking_recv() {
                    info!("Processing event in handler: {:?}", event);
                    if let Err(e) = rt_handle.block_on(state.handle_event(event)) {
                        error!("Error handling event: {}", e);
                    }
                }
                info!("Event handler thread shutting down");
            })
        };

        info!("System ready! Waiting for events...");

        // Wait for shutdown signal
        match shutdown_rx.recv() {
            Ok(_) => info!("Received shutdown signal"),
            Err(e) => error!("Shutdown receiver error: {}", e),
        }

        // Clean shutdown
        info!("Shutdown signal received, cleaning up...");
        drop(event_tx);
        if let Err(e) = client.disconnect() {
            error!("Error during disconnect: {}", e);
        }
        info!("Shutdown complete");
    });

    // Main thread just waits for ctrl-c
    info!("Press Enter to exit...");
    if let Err(e) = std::io::stdin().read_line(&mut String::new()) {
        error!("Error waiting for input: {}", e);
    }
    
    // Signal shutdown
    info!("Sending shutdown signal...");
    if let Err(e) = shutdown_tx.send(()) {
        error!("Error sending shutdown signal: {}", e);
    }
    
    Ok(())
}