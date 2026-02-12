# Asmo

A lightweight REST API server that exposes real-time Android device stats over HTTP — built in Rust for [Termux](https://termux.dev).

Asmo polls hardware telemetry every 500 ms via [Shizuku](https://shizuku.rikka.app/) (`rish`) and serves it as a clean JSON API. Every metric has its own endpoint — query everything at once, or drill into exactly the data you need.

## What it reports

| Category | Fields |
|---|---|
| **Device** | Manufacturer, product model, SoC model |
| **System** | Uptime, memory used / total |
| **Thermal** | CPU temperature, GPU temperature |
| **Battery** | Level, status, temperature |
| **GPU** | Load percentage |
| **Per-core CPU** | Usage %, current / min / max frequency, model name |

## API Reference

All endpoints accept both **GET** and **POST** requests.

### Discovery

| Endpoint | Description |
|---|---|
| `/` | API index — lists every available endpoint |
| `/stats` | Full system stats snapshot |

### Device

| Endpoint | Returns |
|---|---|
| `/manufacturer` | `{"manufacturer": "Nothing"}` |
| `/product_model` | `{"product_model": "A065"}` |
| `/soc_model` | `{"soc_model": "SM8475"}` |

### System

| Endpoint | Returns |
|---|---|
| `/uptime_seconds` | `{"uptime_seconds": 8928}` |
| `/memory_used_mb` | `{"memory_used_mb": 5585.789}` |
| `/memory_total_mb` | `{"memory_total_mb": 11260.543}` |

### Thermal

| Endpoint | Returns |
|---|---|
| `/cpu_temp` | `{"cpu_temp": 34.4}` |
| `/gpu_temp` | `{"gpu_temp": 34.098}` |

### Battery

| Endpoint | Returns |
|---|---|
| `/battery_level` | `{"battery_level": 100}` |
| `/battery_status` | `{"battery_status": "Full"}` |
| `/battery_temp` | `{"battery_temp": 31.0}` |

### GPU

| Endpoint | Returns |
|---|---|
| `/gpu_load` | `{"gpu_load": 5.27}` |

### Per-core CPU

| Endpoint | Description |
|---|---|
| `/cores` | All cores (full array) |
| `/cores/cpu0` | Full snapshot of core 0 |
| `/cores/cpu0/usage` | `{"usage": 28.57}` |
| `/cores/cpu0/model_name` | `{"model_name": "Cortex-A510"}` |
| `/cores/cpu0/cur_freq` | `{"cur_freq": 1804.8}` |
| `/cores/cpu0/min_freq` | `{"min_freq": 300.0}` |
| `/cores/cpu0/max_freq` | `{"max_freq": 1804.8}` |

> Replace `cpu0` with any core name (`cpu1`, `cpu2`, … `cpu7`, etc.).

### Dynamic routing

Endpoints are **generated automatically** from the data structure. If a new field is added to the stats in code, it becomes a reachable endpoint immediately — no routing changes required.

### Error responses

Unknown paths return `404` with a helpful JSON body:

```json
{
  "error": "not found",
  "path": "/nonexistent",
  "hint": "GET / for available endpoints"
}
```

<details>
<summary>Full /stats response example</summary>

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
🚀 Asmo running on: http://192.168.1.42:3000
   GET / for all available endpoints
```

Open the printed URL from any device on the same network.

## Examples

### Discover all endpoints

```sh
curl -s localhost:3000/ | jq .
```

```json
{
  "name": "asmo",
  "version": "0.3.0",
  "endpoints": [
    "/stats",
    "/manufacturer",
    "/battery_level",
    "/cores",
    "/cores/cpu0",
    "/cores/cpu0/usage",
    "..."
  ],
  "usage": "GET or POST any endpoint to retrieve its data."
}
```

### Full stats (GET or POST)

```sh
curl -s localhost:3000/stats | jq .
curl -s -X POST localhost:3000/stats | jq .
```

### Single fields

```sh
# Battery level
curl -s -X POST localhost:3000/battery_level
# → {"battery_level":100}

# GPU load
curl -s -X POST localhost:3000/gpu_load
# → {"gpu_load":5.27}

# CPU temperature
curl -s -X POST localhost:3000/cpu_temp
# → {"cpu_temp":34.4}

# Battery status
curl -s -X POST localhost:3000/battery_status
# → {"battery_status":"Full"}

# Memory usage
curl -s -X POST localhost:3000/memory_used_mb
# → {"memory_used_mb":5585.789}
```

### Per-core CPU data

```sh
# Full snapshot of cpu0
curl -s -X POST localhost:3000/cores/cpu0 | jq .
```

```json
{
  "name": "cpu0",
  "usage": 28.57,
  "model_name": "Cortex-A510",
  "cur_freq": 1804.8,
  "min_freq": 300,
  "max_freq": 1804.8
}
```

```sh
# Just the usage of cpu0
curl -s -X POST localhost:3000/cores/cpu0/usage
# → {"usage":28.57}

# Current frequency of cpu4
curl -s -X POST localhost:3000/cores/cpu4/cur_freq
# → {"cur_freq":1766.4}

# Model name of cpu7
curl -s -X POST localhost:3000/cores/cpu7/model_name
# → {"model_name":"Cortex-X2"}
```

### All cores at once

```sh
curl -s -X POST localhost:3000/cores | jq .
```

### Monitor continuously

```sh
# Poll battery every 2 seconds
watch -n2 'curl -s -X POST localhost:3000/battery_level'

# Poll GPU load + temps
watch -n2 'curl -s localhost:3000/stats | jq "{gpu_load, gpu_temp, cpu_temp}"'
```

### Scripting / piping

```sh
# Log battery level to a file
while true; do curl -s -X POST localhost:3000/battery_level >> battery.jsonl; sleep 5; done

# Get all core usages in one line
for i in $(seq 0 7); do
  echo -n "cpu$i: "
  curl -s -X POST localhost:3000/cores/cpu$i/usage | jq -r '.usage'
done
```

### Integration

The API is stateless and side-effect-free — pipe the JSON into `jq`, `awk`, `gnuplot`, Grafana, Home Assistant, Discord webhooks, or anything else that consumes JSON.

## Architecture

```
main.rs        → Entrypoint — binds the HTTP server (Axum) on port 3000
router.rs      → Dynamic router — resolves any URL path to a stats field at runtime
discover.rs    → One-shot device probe at startup (thermal zones, core topology, SoC identity)
monitor.rs     → Async polling loop — reads /proc and sysfs via a single rish session every 500 ms
types.rs       → Shared data structures (zero-copy Arc<str> strings, typed BatteryStatus enum)
```

### How dynamic routing works

1. `SystemStats` is serialized into a `serde_json::Value` tree on each request.
2. The URL path (`/cores/cpu0/usage`) is split into segments: `["cores", "cpu0", "usage"]`.
3. Each segment navigates one level deeper — object fields by key, array items by `"name"`.
4. The resolved value is returned as JSON.

This means **any new field** added to `SystemStats` (or its nested structs) is instantly available as an endpoint — no manual route registration, no boilerplate.

## License

[MIT](LICENSE)
