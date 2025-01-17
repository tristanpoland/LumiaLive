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
use thiserror::Error;

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

// Configuration structures
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

// Streamlabs event structures
#[derive(Debug, Deserialize)]
struct StreamlabsEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(rename = "for")]
    event_for: Option<String>,
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
}

struct AppState {
    bridge: Arc<Mutex<Bridge>>,
    config: Config,
}

// Convert hex color to Hue values
fn hex_to_hue(hex: &str) -> Result<(u16, u8), AppError> {
    let hex = hex.trim_start_matches('#');
    let rgb = Vec::from_hex(hex)
        .map_err(|e| AppError::Bridge(format!("Invalid hex color: {}", e)))?;
    
    if rgb.len() != 3 {
        return Err(AppError::Bridge("Invalid RGB values".to_string()));
    }
    
    let (r, g, b) = (rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0);
    
    // Convert RGB to HSV
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
    
    // Convert to Hue bridge values
    let hue = ((hue / 360.0) * 65535.0) as u16;
    let saturation = (saturation * 254.0) as u8;
    
    Ok((hue, saturation))
}

impl AppState {
    async fn handle_event(&self, event: StreamlabsEvent) -> Result<(), AppError> {
        debug!("Processing event: {:?}", event);
        
        match (event.event_type.as_str(), event.event_for.as_deref()) {
            ("donation", None) if self.config.events.donation.enabled => {
                if let Some(message) = event.message.first() {
                    self.handle_donation(message).await?;
                }
            },
            ("follow", Some("twitch_account")) if self.config.events.twitch_follow.enabled => {
                self.handle_twitch_follow().await?;
            },
            ("subscription", Some("twitch_account")) if self.config.events.twitch_subscription.enabled => {
                self.handle_twitch_subscription().await?;
            },
            ("bits", Some("twitch_account")) if self.config.events.twitch_bits.enabled => {
                if let Some(message) = event.message.first() {
                    self.handle_bits(message).await?;
                }
            },
            _ => debug!("Unhandled or disabled event: {:?}", event),
        }
        
        Ok(())
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
        let bridge = self.bridge.lock();
        let (hue, sat) = hex_to_hue(&effect.color)?;
        
        let mut command = CommandLight::default();
        command.on = Some(true);
        command.bri = Some(effect.brightness);
        command.hue = Some(hue);
        command.sat = Some(sat);
        command.alert = Some(effect.alert.clone());

        debug!("Applying light effect: {:?}", effect);
        
        let lights = bridge.get_all_lights()
            .map_err(|e| AppError::Bridge(e.to_string()))?;
            
        for light in lights {
            bridge.set_light_state(light.id, &command)
                .map_err(|e| AppError::Bridge(e.to_string()))?;
        }

        // Reset after duration
        sleep(Duration::from_millis(effect.duration)).await;
        
        debug!("Resetting lights to default state");
        let mut reset_command = CommandLight::default();
        reset_command.on = Some(self.config.default_state.on);
        reset_command.bri = Some(self.config.default_state.brightness);
        reset_command.hue = Some(self.config.default_state.hue);
        reset_command.sat = Some(self.config.default_state.saturation);
        reset_command.alert = Some(self.config.default_state.alert.clone());

        let lights = bridge.get_all_lights()
            .map_err(|e| AppError::Bridge(e.to_string()))?;
            
        for light in lights {
            bridge.set_light_state(light.id, &reset_command)
                .map_err(|e| AppError::Bridge(e.to_string()))?;
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    // Initialize logging with env_logger
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    
    info!("Starting LumiaLive...");
    
    info!("Loading configuration...");
    let config: Config = serde_json::from_str(&fs::read_to_string("config.json")?)?;
    
    info!("Connecting to Hue bridge...");
    // Fix for Bridge creation
    let bridge = if let Some(ip) = &config.credentials.hue.bridge_ip {
        Bridge::discover()
            .ok_or_else(|| AppError::Bridge("Failed to discover bridge".to_string()))?
            .with_user(&config.credentials.hue.username)
    } else {
        info!("No bridge IP configured, discovering bridge...");
        Bridge::discover_required()
            .with_user(&config.credentials.hue.username)
    };

    
    let bridge = Arc::new(Mutex::new(bridge));
    
    let state = Arc::new(AppState {
        bridge: bridge.clone(),
        config: config.clone(),
    });

    info!("Connecting to Streamlabs socket API...");
    let socket_url = "https://sockets.streamlabs.com";
    
    let client = ClientBuilder::new(socket_url)
        .on_any({
            let state = state.clone();
            move |event: Event, payload: Payload, _| {
                if let Payload::String(message) = payload {
                    if let Ok(event) = serde_json::from_str::<StreamlabsEvent>(&message) {
                        let state = state.clone();
                        // Process events synchronously since we need to hold the bridge lock
                        if let Err(e) = tokio::runtime::Handle::current()
                            .block_on(state.handle_event(event)) {
                            error!("Error handling event: {}", e);
                        }
                    } else {
                        warn!("Failed to parse Streamlabs event");
                    }
                }
            }
        })
        .connect()
        .map_err(|e| {
            error!("Failed to connect to Streamlabs: {}", e);
            AppError::SocketIo(e)
        })?;

    // Emit the token as a string
    let token_payload = Payload::String(config.credentials.streamlabs.socket_token.clone());
    client.emit("token", token_payload)
        .map_err(|e| AppError::SocketIo(e))?;

    info!("Connected and ready!");
    
    // Handle shutdown gracefully
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Shutdown signal received, cleaning up...");
            if let Err(e) = client.disconnect() {
                error!("Error during disconnect: {}", e);
            }
            info!("Shutdown complete");
        }
        Err(err) => {
            error!("Unable to listen for shutdown signal: {}", err);
        }
    }
    
    Ok(())
}