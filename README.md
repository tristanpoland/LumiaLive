# LumiaLive

## Overview
LumiaLive is a Rust application that integrates Philips Hue lighting systems with Streamlabs alerts, creating interactive lighting effects for streaming events. The application responds to various streaming events (donations, follows, subscriptions, and bits) by triggering customizable lighting effects on Philips Hue lights.

## Features
- Real-time response to Streamlabs events
- Support for multiple event types:
  - Twitch follows
  - Twitch subscriptions
  - Twitch bits
  - Streamlabs donations
- Tiered effects based on donation/bits amounts
- Automatic light state restoration after effects
- Configurable default light states
- Graceful shutdown handling
- Comprehensive error handling and logging

## Architecture

### Core Components

#### Error Handling (`AppError`)
The application uses a custom error type `AppError` that handles various error scenarios:
- IO operations
- JSON parsing
- Socket.IO communication
- Philips Hue bridge communication
- Amount parsing for donations/bits

#### Configuration Structure
The application uses a hierarchical configuration system:

```json
{
  "credentials": {
    "streamlabs": {
      "socket_token": "your_token"
    },
    "hue": {
      "username": "your_username",
      "bridge_ip": "optional_ip"
    }
  },
  "default_state": {
    "on": true,
    "brightness": 254,
    "hue": 8418,
    "saturation": 140,
    "alert": "none"
  },
  "events": {
    "donation": {
      "enabled": true,
      "tiers": [
        {
          "amount": 5.0,
          "effect": {
            "color": "#FF0000",
            "brightness": 254,
            "alert": "lselect",
            "duration": 5000
          }
        }
      ]
    }
    // Similar structure for other events
  }
}
```

### Event Processing Pipeline

1. **Event Reception**
   - Events are received through the Streamlabs Socket.IO connection
   - Each event is parsed into a `StreamlabsEvent` structure
   - Events are validated and routed to appropriate handlers

2. **Event Handling**
   - Each event type has a dedicated handler:
     - `handle_donation`: Processes donation events with tiered effects
     - `handle_twitch_follow`: Handles Twitch follow events
     - `handle_twitch_subscription`: Manages subscription events
     - `handle_bits`: Processes Twitch bits with tiered effects

3. **Light Effect Application**
   - Effects are applied through the `apply_effect` method
   - Colors are converted from hex to Hue HSV format
   - Effects are applied to all connected lights
   - Default state is restored after the effect duration

## Technical Details

### Color Conversion
The application includes a custom `hex_to_hue` function that converts hex color codes to Philips Hue's HSV format:
- Converts hex to RGB values
- Transforms RGB to HSV
- Scales values to Hue's range (0-65535 for hue, 0-254 for saturation)

### State Management
The `AppState` struct maintains:
- A thread-safe reference to the Hue bridge
- Application configuration
- Event handling logic

### Asynchronous Processing
- Uses Tokio for async runtime
- Implements non-blocking event handling
- Maintains responsiveness during effect application

## Setup and Configuration

### Prerequisites
- Rust toolchain
- Access to a Philips Hue bridge
- Streamlabs account with Socket API token

### Configuration Steps

1. Create a `config.json` file with your credentials and preferences
2. Set up Philips Hue bridge:
   - Obtain bridge IP (optional - will auto-discover if not provided)
   - Configure username/token for bridge access
3. Configure Streamlabs:
   - Obtain Socket API token
   - Add token to configuration

### Environment Variables
The application uses `env_logger` for logging configuration:
- Set `RUST_LOG` environment variable to control log levels
- Default level is set to `Info`

## Error Handling and Logging

### Log Levels
- ERROR: Critical issues preventing normal operation
- WARN: Non-critical issues that might need attention
- INFO: General operation information
- DEBUG: Detailed information for troubleshooting

### Error Categories
1. Configuration Errors
   - Invalid JSON format
   - Missing required fields
   - Invalid color codes

2. Connection Errors
   - Streamlabs Socket.IO connection failures
   - Hue bridge connection issues
   - Network timeouts

3. Runtime Errors
   - Invalid event data
   - Failed light state changes
   - Amount parsing errors

## Best Practices

### Configuration
- Keep sensitive credentials secure
- Use tiered effects for scalable responses
- Set reasonable effect durations
- Configure appropriate brightness levels

### Network
- Ensure stable connection to both services
- Handle connection drops gracefully
- Implement appropriate timeouts

### Event Handling
- Validate all incoming data
- Handle missing or malformed fields
- Implement appropriate error recovery

## Troubleshooting

### Common Issues

1. Bridge Connection Failures
   - Verify bridge IP if manually configured
   - Check network connectivity
   - Validate bridge username/token

2. Streamlabs Connection Issues
   - Verify socket token
   - Check network connectivity
   - Confirm Streamlabs service status

3. Effect Issues
   - Verify color code format
   - Check brightness ranges
   - Confirm effect durations

### Debugging Steps
1. Enable debug logging with `RUST_LOG=debug`
2. Check application logs for specific error messages
3. Verify configuration file format and values
4. Test network connectivity to both services
5. Confirm light accessibility through bridge

## Security Considerations

1. Credential Protection
   - Store credentials securely
   - Use environment variables when possible
   - Implement proper file permissions

2. Network Security
   - Use secure connections
   - Implement timeout handling
   - Validate all incoming data

3. Resource Management
   - Implement rate limiting
   - Handle connection pooling
   - Manage memory usage

## Performance Optimization

1. Event Processing
   - Batch similar events when possible
   - Implement debouncing for rapid events
   - Use appropriate async patterns

2. Resource Usage
   - Monitor memory consumption
   - Implement connection pooling
   - Use efficient data structures

3. Network Efficiency
   - Minimize bridge commands
   - Batch light updates when possible
   - Implement appropriate caching

## Future Enhancements

Potential areas for improvement:
1. Support for additional streaming platforms
2. More complex light effects and patterns
3. Web-based configuration interface
4. Effect preview functionality
5. Additional event types support
6. Enhanced error recovery mechanisms
7. Performance monitoring and metrics
8. Configuration hot-reloading