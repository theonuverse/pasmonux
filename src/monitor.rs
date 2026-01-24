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

    let mut prev_total: u64 = 0;
    let mut prev_idle: u64 = 0;
    // Initialize core snaps based on static_info to ensure we track all cores (0-7)
    let mut core_snaps: Vec<CpuSnap> = vec![CpuSnap { total: 0, idle: 0 }; static_info.cores.len()];

    // Improved Batch Command with explicit labels for safer parsing
    let full_cmd = format!(
        "echo UPTIME $(cat /proc/uptime); \
         echo CPU_TEMP $(cat {}); \
         echo GPU_TEMP $(cat {}); \
         cat /proc/stat; \
         grep -E 'MemTotal|MemAvailable' /proc/meminfo; \
         dumpsys battery | grep -E 'level|status|temp'; \
         echo CUR_FREQ_START; cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq; echo CUR_FREQ_END; \
         echo 'END_OF_BATCH'\n",
        paths.cpu_temp, paths.gpu_temp
    );

    loop {
        if stdin.write_all(full_cmd.as_bytes()).is_err() { break; }

        let mut total_cpu = 0.0f32;
        let mut memory_total_gb = 0.0f32;
        let mut memory_available_gb = 0.0f32;
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
                "cpu" => {
                    let (t, i) = calculate_usage(&parts);
                    if prev_total > 0 {
                        let dt = t.saturating_sub(prev_total);
                        let di = i.saturating_sub(prev_idle);
                        if dt > 0 { total_cpu = (dt - di) as f32 / dt as f32 * 100.0; }
                    }
                    prev_total = t; prev_idle = i;
                }
                // Fix: Specifically target cpu0 through cpuN
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
                "MemTotal:" => memory_total_gb = parts.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 1048576.0,
                "MemAvailable:" => memory_available_gb = parts.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 1048576.0,
                "level:" => battery_level = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0),
                "status:" => battery_status = map_status(parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0)),
                "temperature:" => battery_temp = parts.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 10.0,
                _ => {}
            }
        }

        // Build core data using the fixed-size vector
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
            total_cpu,
            memory_used_gb: (memory_total_gb - memory_available_gb).max(0.0),
            memory_total_gb,
            cpu_temp,
            gpu_temp,
            battery_temp,
            battery_level,
            battery_status,
            uptime_seconds,
            cores,
        };

        let _ = tx.send(stats);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

fn calculate_usage(p: &[&str]) -> (u64, u64) {
    // /proc/stat columns: user, nice, system, idle, iowait, irq, softirq, steal
    let vals: Vec<u64> = p.iter().skip(1).filter_map(|s| s.parse().ok()).collect();
    if vals.len() < 4 { return (0, 0); }
    let total: u64 = vals.iter().take(8).sum();
    let idle: u64 = vals[3] + vals.get(4).unwrap_or(&0); // idle + iowait
    (total, idle)
}

fn map_status(c: i32) -> &'static str {
    match c { 2 => "Charging", 3 => "Discharging", 4 => "Not Charging", 5 => "Full", _ => "N/A" }
}
