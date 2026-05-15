# netspeed

A network speed test and real-time terminal monitor, written in Rust.

![TUI screenshot placeholder](https://raw.githubusercontent.com/nitingupta910/netspeed/main/assets/screenshot.png)

## Features

- **Script-friendly CLI mode** — by default runs one speed test and prints parseable output
- **Visible CLI progress** — text mode reports the active test phase on stderr so a run does not look stuck
- **Live throughput TUI** — reads `/proc/net/dev` every second; shows download and upload speeds, totals, and per-session peaks
- **Sparklines & chart** — 60-second scrolling history rendered with Braille-resolution line charts
- **Speed test** — test against Cloudflare's speed endpoint; runs for 10 seconds in both directions for a stable reading
- **Interface picker** — pop-up selector to switch between network interfaces without restarting
- **Auto-detect** — with no arguments, finds the default-route interface from `/proc/net/route`

## Installation

### From crates.io

```bash
cargo install netspeed-ng
```

### From source

```bash
git clone https://github.com/nitingupta910/netspeed
cd netspeed
cargo install --path .
```

## Usage

```
# Run one speed test on the default interface
netspeed

# Run one speed test on a specific interface
netspeed -i eth0
netspeed --interface wlan0

# JSON output for scripts
netspeed --output json

# Show detailed progress on stderr while keeping the final result on stdout
netspeed --progress

# Launch the interactive TUI
netspeed --tui
netspeed --tui --interface wlan0
```

Text mode shows basic progress on stderr, then prints the final result as a single line on stdout:

```text
Testing network speed on eth0...
Testing download...
Testing upload...
```

```text
interface=eth0 download_mbps=713.88 upload_mbps=14.06
```

JSON output stays quiet by default for scripts:

```json
{"interface":"eth0","download_mbps":713.88,"upload_mbps":14.06}
```

### TUI key bindings

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
