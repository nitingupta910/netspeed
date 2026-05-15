# netspeed

A real-time network speed monitor with a terminal UI, written in Rust.

![TUI screenshot placeholder](https://raw.githubusercontent.com/nitingupta910/netspeed/main/assets/screenshot.png)

## Features

- **Live throughput** — reads `/proc/net/dev` every second; shows download and upload speeds, totals, and per-session peaks
- **Sparklines & chart** — 60-second scrolling history rendered with Braille-resolution line charts
- **Speed test** — one-key test against Cloudflare's speed endpoint; runs for 10 seconds in both directions for a stable reading
- **Interface picker** — pop-up selector to switch between network interfaces without restarting
- **Auto-detect** — with no arguments, finds the default-route interface from `/proc/net/route`

## Installation

### From crates.io

```bash
cargo install netspeed
```

### From source

```bash
git clone https://github.com/nitingupta910/netspeed
cd netspeed
cargo install --path .
```

## Usage

```
# Auto-detect the default interface
netspeed

# Monitor a specific interface
netspeed -i eth0
netspeed --interface wlan0
```

### Key bindings

| Key | Action |
|-----|--------|
| `q` | Quit |
| `s` | Run speed test |
| `i` | Open interface selector |
| `↑` / `↓` | Navigate interface list |
| `Enter` | Confirm selection |
| `Esc` | Close popup |

## Requirements

- Linux (reads `/proc/net/dev` and `/proc/net/route`)
- Rust 1.85+ (edition 2024)

## License

Apache-2.0 — see [LICENSE](LICENSE).
