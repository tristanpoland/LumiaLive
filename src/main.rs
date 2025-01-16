use actix_web::{web, App, HttpResponse, HttpServer};
use hueclient::{Bridge, CommandLight};
use serde::Deserialize;
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::Duration;
use tokio::time::sleep;
use dotenv::dotenv;
use std::env;

// Streamlabs webhook payload structures
#[derive(Debug, Deserialize)]
struct StreamlabsWebhook {
    #[serde(rename = "type")]
    event_type: String,
    message: Vec<StreamlabsEvent>,
}

#[derive(Debug, Deserialize)]
struct StreamlabsEvent {
    #[serde(default)]
    name: String,
    #[serde(default)]
    amount: Option<String>,
    #[serde(default)]
    formatted_amount: Option<String>,
}

// Hue bridge configuration
struct AppState {
    bridge: Arc<Mutex<Bridge>>,
}

// Initialize Hue bridge connection
async fn init_bridge() -> Bridge {
    dotenv().ok();
    let username = env::var("HUE_USERNAME").expect("HUE_USERNAME must be set");
    
    Bridge::discover_required().with_user(&username)
}

// Handle different Streamlabs events
async fn handle_webhook(
    data: web::Json<StreamlabsWebhook>,
    state: web::Data<AppState>,
) -> HttpResponse {
    println!("Received webhook: {:?}", data);

    match data.event_type.as_str() {
        "donation" => handle_donation(&state.bridge, &data.message[0]).await,
        "follow" => handle_follow(&state.bridge).await,
        "subscription" => handle_subscription(&state.bridge).await,
        _ => println!("Unhandled event type: {}", data.event_type),
    }

    HttpResponse::Ok().finish()
}

// Handle donation events
async fn handle_donation(bridge: &Arc<Mutex<Bridge>>, event: &StreamlabsEvent) {
    if let Some(amount) = &event.amount {
        let amount: f64 = amount.parse().unwrap_or(0.0);
        
        // Different effects based on donation amount
        let mut command = CommandLight::default();
        
        match amount {
            a if a >= 100.0 => {
                command.on = Some(true);
                command.bri = Some(254);
                command.hue = Some(0);    // Red
                command.sat = Some(254);
                command.alert = Some("lselect".into());
            },
            a if a >= 50.0 => {
                command.on = Some(true);
                command.bri = Some(254);
                command.hue = Some(25500); // Green
                command.sat = Some(254);
                command.alert = Some("select".into());
            },
            _ => {
                command.on = Some(true);
                command.bri = Some(254);
                command.hue = Some(46920); // Blue
                command.sat = Some(254);
                command.alert = Some("select".into());
            }
        };

        apply_effect(bridge, command).await;
    }
}

// Handle follow events
async fn handle_follow(bridge: &Arc<Mutex<Bridge>>) {
    let mut command = CommandLight::default();
    command.on = Some(true);
    command.bri = Some(254);
    command.hue = Some(46920); // Blue
    command.sat = Some(254);
    command.alert = Some("select".into());

    apply_effect(bridge, command).await;
}

// Handle subscription events
async fn handle_subscription(bridge: &Arc<Mutex<Bridge>>) {
    let mut command = CommandLight::default();
    command.on = Some(true);
    command.bri = Some(254);
    command.hue = Some(25500); // Green
    command.sat = Some(254);
    command.alert = Some("select".into());

    apply_effect(bridge, command).await;
}

// Apply light effect and reset after delay
async fn apply_effect(bridge: &Arc<Mutex<Bridge>>, command: CommandLight) {
    let bridge = bridge.lock();
    
    // Get all lights
    if let Ok(lights) = bridge.get_all_lights() {
        for light in lights {
            let _ = bridge.set_light_state(
                light.id,
                &command
            );
        }
    }

    // Reset lights after 5 seconds
    sleep(Duration::from_secs(5)).await;
    
    let mut reset_command = CommandLight::default();
    reset_command.on = Some(true);
    reset_command.bri = Some(254);
    reset_command.hue = Some(8418);
    reset_command.sat = Some(140);
    reset_command.alert = Some("none".into());

    if let Ok(lights) = bridge.get_all_lights() {
        for light in lights {
            let _ = bridge.set_light_state(
                light.id,
                &reset_command
            );
        }
    }
}

async fn run_debug_cycle(bridge: Arc<Mutex<Bridge>>) {
    println!("Running debug cycle...");
    
    // Simulate donation events
    println!("Testing $150 donation effect");
    handle_donation(
        &bridge,
        &StreamlabsEvent {
            name: "Debug Donor".into(),
            amount: Some("150".into()),
            formatted_amount: Some("$150.00".into()),
        },
    ).await;
    
    sleep(Duration::from_secs(7)).await;
    
    println!("Testing $75 donation effect");
    handle_donation(
        &bridge,
        &StreamlabsEvent {
            name: "Debug Donor".into(),
            amount: Some("75".into()),
            formatted_amount: Some("$75.00".into()),
        },
    ).await;
    
    sleep(Duration::from_secs(7)).await;
    
    println!("Testing $25 donation effect");
    handle_donation(
        &bridge,
        &StreamlabsEvent {
            name: "Debug Donor".into(),
            amount: Some("25".into()),
            formatted_amount: Some("$25.00".into()),
        },
    ).await;
    
    sleep(Duration::from_secs(7)).await;
    
    // Simulate follow
    println!("Testing follow effect");
    handle_follow(&bridge).await;
    
    sleep(Duration::from_secs(7)).await;
    
    // Simulate subscription
    println!("Testing subscription effect");
    handle_subscription(&bridge).await;
    
    println!("Debug cycle complete!");
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let debug_mode = env::var("DEBUG_MODE").unwrap_or_else(|_| "false".to_string()) == "true";
    let bridge = Arc::new(Mutex::new(init_bridge().await));

    if debug_mode {
        println!("Debug mode enabled - running effect cycle");
        run_debug_cycle(bridge.clone()).await;
    }

    println!("Server starting on port {}", port);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState {
                bridge: bridge.clone(),
            }))
            .route("/webhook", web::post().to(handle_webhook))
    })
    .bind(format!("0.0.0.0:{}", port))?
    .run()
    .await
}