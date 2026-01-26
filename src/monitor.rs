use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use tokio::sync::watch;

use crate::types::{SystemStats, CoreData, StaticDeviceInfo, CpuSnap, DevicePaths};

pub async fn run_super_fast_monitor(tx: watch::Sender<SystemStats>, paths: DevicePaths, static_info: Arc<StaticDeviceInfo>) {
    let mut child = Command::new("rish").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let reader = BufReader::new(child.stdout.take().unwrap());
    let mut lines = reader.lines();

    // Removed global prev_total/prev_idle to stop warnings
    let mut core_snaps: Vec<CpuSnap> = vec![CpuSnap { total: 0, idle: 0 }; static_info.cores.len()];

    let full_cmd = format!(
        "echo UPTIME $(cat /proc/uptime); \
         echo CPU_TEMP $(cat {}); \
         echo GPU_TEMP $(cat {}); \
         echo GPU_BUSY $(cat /sys/class/kgsl/kgsl-3d0/gpubusy); \
         cat /proc/stat; \
         grep -E 'MemTotal|MemAvailable' /proc/meminfo; \
         dumpsys battery | grep -E 'level|status|temp'; \
         echo CUR_FREQ_START; cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq; echo CUR_FREQ_END; \
         echo 'END_OF_BATCH'\n",
        paths.cpu_temp, paths.gpu_temp
    );

    loop {
        if stdin.write_all(full_cmd.as_bytes()).is_err() { break; }

        let mut gpu_load = 0.0f32;
        let mut memory_total_mb = 0.0f32;
        let mut memory_available_mb = 0.0f32;
        let mut cpu_temp = 0.0f32;
        let mut gpu_temp = 0.0f32;
        let mut battery_temp = 0.0f32;
        let mut battery_level = 0i32;
        let mut battery_status: &'static str = "N/A";
        let mut uptime_seconds = 0u64;
        let mut core_usages: Vec<f32> = vec![0.0; static_info.cores.len()];
        let mut cur_freqs: Vec<f32> = Vec::with_capacity(static_info.cores.len());
        let mut collecting_freqs = false;

        while let Some(Ok(line)) = lines.next() {
            let line = line.trim();
            if line == "END_OF_BATCH" { break; }
            if line == "CUR_FREQ_START" { collecting_freqs = true; continue; }
            if line == "CUR_FREQ_END" { collecting_freqs = false; continue; }

            if collecting_freqs {
                if let Ok(v) = line.parse::<f32>() {
                    cur_freqs.push(v / 1000.0);
                }
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() { continue; }

            match parts[0] {
                "UPTIME" => uptime_seconds = parts.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) as u64,
                "CPU_TEMP" => cpu_temp = parts.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 1000.0,
                "GPU_TEMP" => gpu_temp = parts.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 1000.0,
                "GPU_BUSY" => {
                    if parts.len() >= 3 {
                        let busy: u64 = parts[1].parse().unwrap_or(0);
                        let total: u64 = parts[2].parse().unwrap_or(0);
                        if total > 0 {
                            gpu_load = (busy as f32 / total as f32) * 100.0;
                        } else {
                            gpu_load = 0.0;
                        }
                    }
                }
                "cpu" => {
                    // We skip the aggregate "cpu" line now as we track per-core
                    continue;
                }
                p if p.starts_with("cpu") && p.len() > 3 => {
                    if let Ok(core_idx) = p[3..].parse::<usize>() {
                        if core_idx < core_snaps.len() {
                            let (t, i) = calculate_usage(&parts);
                            let dt = t.saturating_sub(core_snaps[core_idx].total);
                            let di = i.saturating_sub(core_snaps[core_idx].idle);
                            if dt > 0 {
                                core_usages[core_idx] = (dt - di) as f32 / dt as f32 * 100.0;
                            }
                            core_snaps[core_idx] = CpuSnap { total: t, idle: i };
                        }
                    }
                }
                "MemTotal:" => memory_total_mb = parts.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 1024.0,
                "MemAvailable:" => memory_available_mb = parts.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 1024.0,
                "level:" => battery_level = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0),
                "status:" => battery_status = map_status(parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0)),
                "temperature:" => battery_temp = parts.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 10.0,
                _ => {}
            }
        }

        let cores: Vec<CoreData> = static_info.cores.iter().enumerate().map(|(i, info)| {
            CoreData {
                name: info.name.clone(),
                usage: core_usages.get(i).copied().unwrap_or(0.0),
                model_name: info.model_name.clone(),
                cur_freq: cur_freqs.get(i).copied().unwrap_or(0.0),
                min_freq: info.min_freq,
                max_freq: info.max_freq,
            }
        }).collect();

        let stats = SystemStats {
            manufacturer: static_info.manufacturer.clone(),
            product_model: static_info.product_model.clone(),
            soc_model: static_info.soc_model.clone(),
            uptime_seconds,
            battery_level,
            battery_status,
            battery_temp,
            cpu_temp,
            gpu_temp,
            gpu_load,
            memory_used_mb: (memory_total_mb - memory_available_mb).max(0.0),
            memory_total_mb,
            cores,
        };

        let _ = tx.send(stats);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

fn calculate_usage(p: &[&str]) -> (u64, u64) {
    let vals: Vec<u64> = p.iter().skip(1).filter_map(|s| s.parse().ok()).collect();
    if vals.len() < 4 { return (0, 0); }
    let total: u64 = vals.iter().take(8).sum();
    let idle: u64 = vals[3] + vals.get(4).unwrap_or(&0);
    (total, idle)
}

fn map_status(c: i32) -> &'static str {
    match c { 2 => "Charging", 3 => "Discharging", 4 => "Not Charging", 5 => "Full", _ => "N/A" }
}