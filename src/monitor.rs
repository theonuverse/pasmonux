use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use crate::types::{
    BatteryStatus, CoreData, CpuSnap, DevicePaths, StaticDeviceInfo, SystemStats,
};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Hot monitoring loop — spawned once, runs forever.
// ---------------------------------------------------------------------------

pub async fn run_monitor(
    tx: watch::Sender<SystemStats>,
    paths: DevicePaths,
    static_info: Arc<StaticDeviceInfo>,
) {
    let core_len = static_info.cores.len();

    // Build the shell command string once.
    let cmd = format!(
        "echo UPTIME $(cat /proc/uptime); \
         echo CPU_TEMP $(cat {cpu_temp}); \
         echo GPU_TEMP $(cat {gpu_temp}); \
         echo GPU_BUSY $(cat /sys/class/kgsl/kgsl-3d0/gpubusy); \
         cat /proc/stat; \
         grep -E 'MemTotal|MemAvailable' /proc/meminfo; \
         dumpsys battery | grep -E 'level|status|temp'; \
         echo CUR_FREQ_START; \
         cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq; \
         echo CUR_FREQ_END; \
         echo 'END_OF_BATCH'\n",
        cpu_temp = paths.cpu_temp,
        gpu_temp = paths.gpu_temp,
    );
    let cmd_bytes = cmd.as_bytes();

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
    let mut cur_freqs = Vec::<f32>::with_capacity(core_len);

    loop {
        // Send the batch command.
        if stdin.write_all(cmd_bytes).is_err() || stdin.flush().is_err() {
            break;
        }

        // Reset per-tick scratch.
        core_usages.iter_mut().for_each(|u| *u = 0.0);
        cur_freqs.clear();

        let mut gpu_load = 0.0_f32;
        let mut memory_total_mb = 0.0_f32;
        let mut memory_avail_mb = 0.0_f32;
        let mut cpu_temp = 0.0_f32;
        let mut gpu_temp = 0.0_f32;
        let mut battery_temp = 0.0_f32;
        let mut battery_level = 0_i32;
        let mut battery_status = BatteryStatus::Unknown;
        let mut uptime_seconds = 0_u64;
        let mut collecting_freqs = false;

        while let Some(Ok(raw_line)) = lines.next() {
            let line = raw_line.trim();

            if line == "END_OF_BATCH" {
                break;
            }
            if line == "CUR_FREQ_START" {
                collecting_freqs = true;
                continue;
            }
            if line == "CUR_FREQ_END" {
                collecting_freqs = false;
                continue;
            }

            if collecting_freqs {
                if let Ok(v) = line.parse::<f32>() {
                    cur_freqs.push(v / 1000.0);
                }
                continue;
            }

            // Fast: grab the first token without allocating a Vec.
            let (tag, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));

            match tag {
                "UPTIME" => {
                    uptime_seconds = rest
                        .split_whitespace()
                        .next()
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(0.0) as u64;
                }
                "CPU_TEMP" => {
                    cpu_temp = parse_or_zero(rest.trim()) / 1000.0;
                }
                "GPU_TEMP" => {
                    gpu_temp = parse_or_zero(rest.trim()) / 1000.0;
                }
                "GPU_BUSY" => {
                    let mut it = rest.split_whitespace();
                    let busy: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                    let total: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                    gpu_load = if total > 0 {
                        busy as f32 / total as f32 * 100.0
                    } else {
                        0.0
                    };
                }
                "cpu" => { /* aggregate line — skip */ }
                "MemTotal:" => {
                    memory_total_mb = parse_or_zero(rest.trim()) / 1024.0;
                }
                "MemAvailable:" => {
                    memory_avail_mb = parse_or_zero(rest.trim()) / 1024.0;
                }
                "level:" => {
                    battery_level = rest.trim().parse().unwrap_or(0);
                }
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
            uptime_seconds,
            battery_level,
            battery_status,
            battery_temp,
            cpu_temp,
            gpu_temp,
            gpu_load,
            memory_used_mb: (memory_total_mb - memory_avail_mb).max(0.0),
            memory_total_mb,
            cores,
        };

        let _ = tx.send(stats);
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ---------------------------------------------------------------------------
// Tiny helpers — kept out of the hot path's match for readability.
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