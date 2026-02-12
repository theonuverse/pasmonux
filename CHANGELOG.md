# Changelog

## [0.3.0] — 2026-02-12

### Added
- **Dynamic per-field endpoints** — every stat is individually addressable (`/battery_level`, `/gpu_load`, `/cpu_temp`, etc.)
- **Multi-field queries** — comma-separated fields in a single request (`/battery_level,battery_status,battery_temp`), with results returned in the order you specify
- **Wildcard queries** — `*` and `all` expand over arrays (`/cores/*/usage`, `/cores/all/usage,cur_freq,model_name`)
- **Per-core deep access** — `/cores/cpu0`, `/cores/cpu0/usage`, `/cores/cpu0/usage,cur_freq`
- **API index** — `GET /` lists every available endpoint with usage hints
- **Dynamic routing** — new fields added to `SystemStats` are automatically exposed as endpoints with zero code changes
- **Preserved field order** — JSON responses maintain struct declaration order; comma queries maintain your URL order (`serde_json` `preserve_order` feature)
- **`router.rs`** — new module handling all dynamic path resolution, wildcards, and multi-field logic

### Changed
- **`discover.rs`** — thermal zone scanning and CPU core enumeration now use direct `std::fs` reads instead of `rish`, reducing startup privilege requirements
- **`monitor.rs`** — split into direct sysfs reads (thermals, GPU load, memory, CPU frequencies) and `rish`-only reads (uptime, `/proc/stat`, battery via `dumpsys`), reducing privileged shell usage
- **`main.rs`** — simplified to pure bootstrap; all routing moved to `router.rs`
- Startup message now shows `http://<ip>:3000` (was `/stats`)

### Removed
- `rish` dependency for device discovery (thermal zones, core count) — now reads sysfs directly
- `rish` dependency for reading CPU temperatures, GPU temperature, GPU load, memory info, and CPU frequencies in the monitor loop
- POST method support — API is GET-only, which is semantically correct for read-only data

## [0.2.0]

- Initial release with single `/stats` endpoint
