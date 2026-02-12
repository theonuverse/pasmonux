# Asmo

A lightweight REST API server that exposes real-time Android device stats over HTTP — built in Rust for [Termux](https://termux.dev).

Asmo polls hardware telemetry every 500 ms via [Shizuku](https://shizuku.rikka.app/) (`rish`) and serves it as a single JSON endpoint. Point any browser, script, or dashboard at `http://<device-ip>:3000/stats` and get live data instantly.

## What it reports

| Category | Fields |
|---|---|
| **Device** | Manufacturer, product model, SoC model |
| **System** | Uptime, memory used / total |
| **Thermal** | CPU temperature, GPU temperature |
| **Battery** | Level, status, temperature |
| **GPU** | Load percentage |
| **Per-core CPU** | Usage %, current / min / max frequency, model name |

<details>
<summary>Example response</summary>

```json
{
  "manufacturer": "Samsung",
  "product_model": "SM-S926B",
  "soc_model": "sun",
  "uptime_seconds": 34812,
  "battery_level": 72,
  "battery_status": "Discharging",
  "battery_temp": 28.5,
  "cpu_temp": 38.0,
  "gpu_temp": 36.2,
  "gpu_load": 12.5,
  "memory_used_mb": 4321.0,
  "memory_total_mb": 7640.0,
  "cores": [
    {
      "name": "cpu0",
      "usage": 14.2,
      "model_name": "Cortex-A520",
      "cur_freq": 1100.0,
      "min_freq": 400.0,
      "max_freq": 2000.0
    }
  ]
}
```

</details>

## Prerequisites

- [Termux](https://termux.dev) installed
- [Shizuku](https://shizuku.rikka.app/) running (provides `rish` for privileged sysfs access)

## Build from source

```sh
# Update packages
yes | pkg up

# Install dependencies
pkg install git rust -y

# Clone the repo
git clone https://github.com/theonuverse/asmo.git
cd asmo

# Build
cargo build --release

# Install into Termux PATH
cp target/release/asmo $PREFIX/bin/
```

> **Tip:** If you just want to test without installing, run `cargo run --release` from the project directory.

## Usage

```sh
asmo
```

```
🚀 Asmo running on: http://192.168.1.42:3000/stats
```

Open the printed URL from any device on the same network.

## Architecture

```
main.rs        → Entrypoint — binds the HTTP server (Axum) on port 3000
discover.rs    → One-shot device probe at startup (thermal zones, core topology, SoC identity)
monitor.rs     → Async polling loop — reads /proc and sysfs via a single rish session every 500 ms
types.rs       → Shared data structures (zero-copy Arc<str> strings, typed BatteryStatus enum)
```

## License

[MIT](LICENSE)
