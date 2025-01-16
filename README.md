# LumiaLive 🎮💡

Transform your Philips Hue lights into a dynamic stream celebration system! LumiaLive bridges Streamlabs alerts with your Hue setup, creating immersive light shows for donations, follows, and subscriptions.

## Features

- **Smart Donation Reactions**: Lights respond differently based on donation amount
  - $100+ triggers an intense red light show
  - $50+ creates a celebratory green effect
  - Any amount shows appreciation with a cool blue pulse
- **Follow Celebrations**: Welcome new followers with a smooth blue transition
- **Sub Celebrations**: Mark new subscriptions with an energetic green display
- **Multi-Light Support**: Works with any number of Hue lights on your network
- **Auto-Reset**: Lights automatically return to their original state after each event
- **Debug Mode**: Test all effects without needing real stream events

## Setup

### Prerequisites
- Rust installed on your system
- A Philips Hue Bridge on your local network
- Streamlabs account with API access

### Installation

1. Clone the repository:
```bash
git clone https://github.com/tristanpoland/LumiaLive.git
cd LumiaLive
```

2. Create a `.env` file in the project root:
```env
HUE_USERNAME=your_hue_api_username
PORT=8080
DEBUG_MODE=false
```

3. Build the project:
```bash
cargo build --release
```

### Getting Your Hue API Username

1. Press the link button on your Hue Bridge
2. Visit `https://discovery.meethue.com/` to find your bridge IP
3. Follow the [Philips Hue API documentation](https://developers.meethue.com/develop/get-started-2/) to create a username

### Connecting to Streamlabs

1. Go to your Streamlabs dashboard
2. Navigate to Settings -> API Settings
3. Add a new webhook pointing to `http://your-server:8080/webhook`

## Usage

Start the server:
```bash
cargo run --release
```

### Testing the Setup

Enable debug mode in your `.env` file to run through all possible light effects:
```env
DEBUG_MODE=true
```

This will cycle through:
- Large donation effect
- Medium donation effect
- Small donation effect
- Follow effect
- Subscription effect

## Configuration

Light effects can be customized by modifying the following functions in `main.rs`:
- `handle_donation`: Controls donation-based effects
- `handle_follow`: Controls follow celebration effects
- `handle_subscription`: Controls subscription celebration effects

### Example: Customizing Colors

Each effect uses Hue's color system (0-65535):
- Red: 0
- Green: 25500
- Blue: 46920

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

## License

[MIT](LICENSE)

## Acknowledgments

- Built with [hueclient](https://crates.io/crates/hueclient)
- Inspired by the streaming community's love for dynamic interactions
