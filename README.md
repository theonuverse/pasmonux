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
  "manufacturer": "Nothing",
  "product_model": "A065",
  "soc_model": "SM8475",
  "uptime_seconds": 8928,
  "battery_level": 100,
  "battery_status": "Full",
  "battery_temp": 31,
  "cpu_temp": 34.4,
  "gpu_temp": 34.098,
  "gpu_load": 5.2692976,
  "memory_used_mb": 5585.789,
  "memory_total_mb": 11260.543,
  "cores": [
    {
      "name": "cpu0",
      "usage": 28.57143,
      "model_name": "Cortex-A510",
      "cur_freq": 1804.8,
      "min_freq": 300,
      "max_freq": 1804.8
    },
    {
      "name": "cpu1",
      "usage": 28.57143,
      "model_name": "Cortex-A510",
      "cur_freq": 1440,
      "min_freq": 300,
      "max_freq": 1804.8
    },
    {
      "name": "cpu2",
      "usage": 26.984129,
      "model_name": "Cortex-A510",
      "cur_freq": 1440,
      "min_freq": 300,
      "max_freq": 1804.8
    },
    {
      "name": "cpu3",
      "usage": 31.746033,
      "model_name": "Cortex-A510",
      "cur_freq": 1440,
      "min_freq": 300,
      "max_freq": 1804.8
    },
    {
      "name": "cpu4",
      "usage": 9.230769,
      "model_name": "Cortex-A710",
      "cur_freq": 1766.4,
      "min_freq": 633.6,
      "max_freq": 2496
    },
    {
      "name": "cpu5",
      "usage": 23.188406,
      "model_name": "Cortex-A710",
      "cur_freq": 1881.6,
      "min_freq": 633.6,
      "max_freq": 2496
    },
    {
      "name": "cpu6",
      "usage": 10.769231,
      "model_name": "Cortex-A710",
      "cur_freq": 1881.6,
      "min_freq": 633.6,
      "max_freq": 2496
    },
    {
      "name": "cpu7",
      "usage": 0,
      "model_name": "Cortex-X2",
      "cur_freq": 2476.8,
      "min_freq": 787.2,
      "max_freq": 2995.2
    }
  ]
}
```

<img src="assets/showcase.png" alt="Asmo showcase" width=250>

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

## Querying with curl & jq

Because Asmo is a plain HTTP JSON endpoint, you can integrate it with **any** tool that speaks HTTP.

**Fetch raw JSON:**

```sh
curl -s localhost:3000/stats
```

**Pretty-print with jq:**

```sh
curl -s localhost:3000/stats | jq .
```

**Filter specific fields — e.g. battery level and status:**

```sh
curl -s localhost:3000/stats | jq '{battery_level, battery_status}'
```

```json
{
  "battery_level": 100,
  "battery_status": "Full"
}
```

**Get only CPU temperatures and GPU load:**

```sh
curl -s localhost:3000/stats | jq '{cpu_temp, gpu_temp, gpu_load}'
```

**List per-core usage as a compact table:**

```sh
curl -s localhost:3000/stats | jq -r '.cores[] | "\(.name)\t\(.usage)%\t\(.cur_freq) MHz"'
```

```
cpu0	28.57%	1804.8 MHz
cpu1	14.29%	1440 MHz
cpu2	26.98%	1440 MHz
...
```

**Monitor continuously (poll every 2 s):**

```sh
watch -n2 'curl -s localhost:3000/stats | jq "{cpu_temp, gpu_temp, battery_level}"'
```

**Log to a file for later analysis:**

```sh
while true; do curl -s localhost:3000/stats | jq -c . >> stats.jsonl; sleep 5; done
```

**Feed into other programs** — pipe to `awk`, `gnuplot`, `grafana-agent`, a Discord webhook, Home Assistant, or anything else that consumes JSON. The endpoint is stateless and side-effect-free, so the possibilities are unlimited.

## Architecture

```
main.rs        → Entrypoint — binds the HTTP server (Axum) on port 3000
discover.rs    → One-shot device probe at startup (thermal zones, core topology, SoC identity)
monitor.rs     → Async polling loop — reads /proc and sysfs via a single rish session every 500 ms
types.rs       → Shared data structures (zero-copy Arc<str> strings, typed BatteryStatus enum)
```

## License

[MIT](LICENSE)
