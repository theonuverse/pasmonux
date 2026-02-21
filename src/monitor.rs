use std::ffi::CString;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use crate::types::{
    BatteryStatus, CoreData, CpuSnap, DevicePaths, StaticDeviceInfo, SystemStats,
};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const STORAGE_TICK_INTERVAL: u64 = 60; // 60 ticks = 30 s

// ---------------------------------------------------------------------------
// Hot monitoring loop — spawned once, runs forever.
// ---------------------------------------------------------------------------

pub async fn run_monitor(
    tx: watch::Sender<SystemStats>,
    paths: DevicePaths,
    static_info: Arc<StaticDeviceInfo>,
) {
    let core_len = static_info.cores.len();

    // Fast rish batch — runs every tick (network + cpu + battery + display brightness).
    let fast_cmd = b"echo UPTIME $(cat /proc/uptime); \
                     cat /proc/stat; \
                     dumpsys battery | grep -E 'level|status|temp'; \
                     echo NET_DATA; cat /proc/net/dev; echo NET_END; \
                     echo DISPLAY_DATA; \
                     dumpsys display | grep -oE 'mBrightness=[0-9.]+|mActiveRenderFrameRate=[0-9.]+'; \
                     echo DISPLAY_END; \
                     echo 'END_OF_BATCH'\n";

    // Slow rish batch — same as fast (refresh rate changes rarely but comes for free).
    let slow_cmd = &fast_cmd[..];

    // Spawn a single long-lived `rish` shell.
    let mut child = Command::new("rish")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn rish");

    let mut stdin = BufWriter::new(child.stdin.take().expect("rish stdin"));
    let stdout = BufReader::new(child.stdout.take().expect("rish stdout"));
    let mut lines = stdout.lines();

    // Ensure a scope-guard reaps the child so we never leave zombies.
    scopeguard::defer! {
        let _ = child.wait();
    }

    // Pre-allocated scratch space — reused every tick.
    let mut core_snaps: Vec<CpuSnap> = (0..core_len).map(|_| CpuSnap::default()).collect();
    let mut core_usages = vec![0.0_f32; core_len];

    // Slow-tick cached state — retained between iterations.
    let mut tick: u64 = 0;
    let mut cached_storage_free_gb = 0.0_f32;
    let mut cached_storage_total_gb = 0.0_f32;

    loop {
        let is_storage_tick = tick % STORAGE_TICK_INTERVAL == 0;

        // ── Direct sysfs/procfs reads (no privilege needed) ──────────
        let cpu_temp = read_sysfs_thermal(&paths.cpu_temp);
        let gpu_temp = read_sysfs_thermal(&paths.gpu_temp);
        let gpu_load = read_gpu_load();
        let (memory_total_mb, memory_avail_mb, swap_total_mb, swap_free_mb) = read_memory();
        let cur_freqs = read_cpu_freqs(core_len);

        // Slow direct reads.
        if is_storage_tick {
            let (free, total) = read_storage();
            cached_storage_free_gb = free;
            cached_storage_total_gb = total;
        }

        // ── Privileged reads via rish ────────────────────────────────
        let cmd = &fast_cmd[..];
        let _ = slow_cmd; // suppress unused warning
        if stdin.write_all(cmd).is_err() || stdin.flush().is_err() {
            break;
        }

        core_usages.iter_mut().for_each(|u| *u = 0.0);

        let mut battery_temp = 0.0_f32;
        let mut battery_level = 0_i32;
        let mut battery_status = BatteryStatus::Unknown;
        let mut uptime_seconds = 0_u64;
        let mut tx_bytes = 0_u64;
        let mut rx_bytes = 0_u64;
        let mut in_net_section = false;
        let mut in_display_section = false;
        let mut brightness = 0.0_f32;
        let mut refresh_rate = 0.0_f32;
        let mut brightness_found = false;
        let mut refresh_rate_found = false;

        while let Some(Ok(raw_line)) = lines.next() {
            let line = raw_line.trim();

            if line == "END_OF_BATCH" {
                break;
            }

            // ── Section markers ──────────────────────────────────────
            if line == "NET_DATA" {
                in_net_section = true;
                continue;
            }
            if line == "NET_END" {
                in_net_section = false;
                continue;
            }
            if line == "DISPLAY_DATA" {
                in_display_section = true;
                continue;
            }
            if line == "DISPLAY_END" {
                in_display_section = false;
                continue;
            }

            // ── Network section ──────────────────────────────────────
            if in_net_section {
                // /proc/net/dev: iface: rx_bytes rx_packets … tx_bytes …
                if let Some((iface, rest)) = line.split_once(':') {
                    let iface = iface.trim();
                    if iface != "lo" {
                        let fields: Vec<&str> = rest.split_whitespace().collect();
                        if fields.len() >= 10 {
                            rx_bytes += fields[0].parse::<u64>().unwrap_or(0);
                            tx_bytes += fields[8].parse::<u64>().unwrap_or(0);
                        }
                    }
                }
                continue;
            }

            // ── Display section ──────────────────────────────────────
            if in_display_section {
                if let Some(val) = line.strip_prefix("mBrightness=") {
                    if !brightness_found {
                        brightness = val.parse().unwrap_or(0.0);
                        brightness_found = true;
                    }
                } else if let Some(val) = line.strip_prefix("mActiveRenderFrameRate=") {
                    if !refresh_rate_found {
                        refresh_rate = val.parse().unwrap_or(0.0);
                        refresh_rate_found = true;
                    }
                }
                continue;
            }

            // ── Normal section (uptime / cpu / battery) ──────────────
            let (tag, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));

            match tag {
                "UPTIME" => {
                    uptime_seconds = rest
                        .split_whitespace()
                        .next()
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(0.0) as u64;
                }
                "cpu" => { /* aggregate line — skip */ }
                "level:" => battery_level = rest.trim().parse().unwrap_or(0),
                "status:" => {
                    battery_status =
                        BatteryStatus::from_code(rest.trim().parse().unwrap_or(0));
                }
                "temperature:" => {
                    battery_temp = parse_or_zero(rest.trim()) / 10.0;
                }
                tag if tag.starts_with("cpu") => {
                    if let Ok(idx) = tag[3..].parse::<usize>()
                        && idx < core_len
                    {
                        let (t, i) = parse_cpu_stat(rest);
                        let dt = t.saturating_sub(core_snaps[idx].total);
                        let di = i.saturating_sub(core_snaps[idx].idle);
                        if dt > 0 {
                            core_usages[idx] =
                                (dt - di) as f32 / dt as f32 * 100.0;
                        }
                        core_snaps[idx] = CpuSnap { total: t, idle: i };
                    }
                }
                _ => {}
            }
        }

        // Build the payload — Arc clones are just atomic increments.
        let cores: Vec<CoreData> = static_info
            .cores
            .iter()
            .enumerate()
            .map(|(i, info)| CoreData {
                name: Arc::clone(&info.name),
                usage: core_usages.get(i).copied().unwrap_or(0.0),
                model_name: Arc::clone(&info.model_name),
                cur_freq: cur_freqs.get(i).copied().unwrap_or(0.0),
                min_freq: info.min_freq,
                max_freq: info.max_freq,
            })
            .collect();

        let stats = SystemStats {
            manufacturer: Arc::clone(&static_info.manufacturer),
            product_model: Arc::clone(&static_info.product_model),
            soc_model: Arc::clone(&static_info.soc_model),
            kernel_version: Arc::clone(&static_info.kernel_version),
            android_version: Arc::clone(&static_info.android_version),
            uptime_seconds,
            battery_level,
            battery_status,
            battery_temp,
            cpu_temp,
            gpu_temp,
            gpu_load,
            memory_used_mb: (memory_total_mb - memory_avail_mb).max(0.0),
            memory_total_mb,
            swap_used_mb: (swap_total_mb - swap_free_mb).max(0.0),
            swap_total_mb,
            tx_bytes_mb: tx_bytes as f32 / (1024.0 * 1024.0),
            rx_bytes_mb: rx_bytes as f32 / (1024.0 * 1024.0),
            storage_free_gb: cached_storage_free_gb,
            storage_total_gb: cached_storage_total_gb,
            refresh_rate,
            brightness,
            cores,
        };

        let _ = tx.send(stats);
        tick += 1;
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ---------------------------------------------------------------------------
// Direct sysfs/procfs readers — no privilege needed.
// ---------------------------------------------------------------------------

/// Read a thermal zone temperature, returns degrees Celsius.
#[inline]
fn read_sysfs_thermal(path: &str) -> f32 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<f32>().ok())
        .unwrap_or(0.0)
        / 1000.0
}

/// Read GPU load from kgsl sysfs.
#[inline]
fn read_gpu_load() -> f32 {
    let content =
        std::fs::read_to_string("/sys/class/kgsl/kgsl-3d0/gpubusy").unwrap_or_default();
    let mut it = content.split_whitespace();
    let busy: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let total: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    if total > 0 {
        busy as f32 / total as f32 * 100.0
    } else {
        0.0
    }
}

/// Read MemTotal, MemAvailable, SwapTotal, and SwapFree from `/proc/meminfo`.
/// Returns (total_mb, available_mb, swap_total_mb, swap_free_mb).
#[inline]
fn read_memory() -> (f32, f32, f32, f32) {
    let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total = 0.0_f32;
    let mut avail = 0.0_f32;
    let mut swap_total = 0.0_f32;
    let mut swap_free = 0.0_f32;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total = parse_or_zero(rest.trim()) / 1024.0;
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail = parse_or_zero(rest.trim()) / 1024.0;
        } else if let Some(rest) = line.strip_prefix("SwapTotal:") {
            swap_total = parse_or_zero(rest.trim()) / 1024.0;
        } else if let Some(rest) = line.strip_prefix("SwapFree:") {
            swap_free = parse_or_zero(rest.trim()) / 1024.0;
        }
    }
    (total, avail, swap_total, swap_free)
}

/// Read current frequency for each core from sysfs, returns MHz.
fn read_cpu_freqs(count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| {
            std::fs::read_to_string(format!(
                "/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_cur_freq"
            ))
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(0.0)
                / 1000.0
        })
        .collect()
}

/// Read storage free/total for `/data` via `statvfs`.
/// Returns (free_gb, total_gb).
#[inline]
fn read_storage() -> (f32, f32) {
    let path = CString::new("/data").unwrap();
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path.as_ptr(), &mut stat) == 0 {
            let bs = stat.f_frsize as f64;
            let total = (stat.f_blocks as f64 * bs) / (1024.0 * 1024.0 * 1024.0);
            let free = (stat.f_bavail as f64 * bs) / (1024.0 * 1024.0 * 1024.0);
            (free as f32, total as f32)
        } else {
            (0.0, 0.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers — rish output.
// ---------------------------------------------------------------------------

/// Parse the first whitespace-delimited token as `f32`, defaulting to `0.0`.
#[inline]
fn parse_or_zero(s: &str) -> f32 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// Parse a `/proc/stat` CPU line's numeric fields into (total, idle).
#[inline]
fn parse_cpu_stat(rest: &str) -> (u64, u64) {
    let mut total = 0_u64;
    let mut idle = 0_u64;
    for (i, tok) in rest.split_whitespace().take(8).enumerate() {
        if let Ok(v) = tok.parse::<u64>() {
            total += v;
            // Fields 3 = idle, 4 = iowait.
            if i == 3 || i == 4 {
                idle += v;
            }
        }
    }
    (total, idle)
}