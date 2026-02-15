# Desktop Dictate

A lightweight desktop dictation app built with Tauri 2 + React. Uses Soniox real-time speech-to-text to transcribe your microphone and type the result directly into any focused application.

## Features

- **Global hotkey** -- toggle recording from anywhere (preset keys or custom key combos)
- **Real-time preview** -- see partial transcription as you speak
- **Direct typing** -- recognized text is typed into the previously focused window via clipboard paste
- **60+ languages** -- with configurable language hints and language restrictions
- **Transcription history** -- past sessions are saved and copyable
- **System tray** -- runs in background, click tray icon to show/hide

## Requirements

- **Linux (X11)** with `xdotool` and `xclip`:
  ```bash
  sudo apt install xdotool xclip
  ```
- **Soniox API key** -- get one free at https://soniox.com

## Build from Source

```bash
# System dependencies (Debian/Ubuntu)
sudo apt install libssl-dev libwebkit2gtk-4.1-dev build-essential curl wget

# Install Rust (if not already)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Node.js dependencies
npm install

# Dev mode
npm run tauri dev

# Production build
npm run tauri build
```

The built binary and `.deb`/`.AppImage` packages will be in `src-tauri/target/release/bundle/`.

## Usage

1. Launch the app
2. Open **Settings** (gear icon) and enter your Soniox API key
3. Choose a hotkey (preset or record a custom combo)
4. Optionally configure language hints / restrictions
5. Focus any text field in any application
6. Press the hotkey to start dictating, press again to stop
7. The transcribed text is typed at your cursor position

## Settings

| Setting | Description |
|---|---|
| **API Key** | Your Soniox API key (required) |
| **Hotkey** | Preset (Insert, F1-F12, etc.) or any custom key combination |
| **Language Hints** | Optional list of expected languages to improve accuracy |
| **Language Restrictions** | Optional -- restrict recognition to only selected languages |

## Architecture

```
src/              # React/TypeScript frontend
  App.tsx         # Main view, event listeners, preview
  components/
    Settings.tsx  # Settings page, MultiSelect, hotkey recorder

src-tauri/src/    # Rust backend
  lib.rs          # App state, Tauri commands, hotkey registration
  audio/mod.rs    # Microphone capture via cpal
  soniox/mod.rs   # WebSocket streaming to Soniox API
  typer/mod.rs    # Text insertion via xdotool/xclip
```

## License

MIT
