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
    let mut core_snaps: Vec<CpuSnap> = vec![CpuSnap { total: 0, idle: 0 }; paths.core_count];

    // Full batch command: uptime, temps, /proc/stat, meminfo, battery, and current CPU freqs via rish
    let full_cmd = format!(
        "cat /proc/uptime {} {}; cat /proc/stat; grep -E 'MemTotal|MemAvailable' /proc/meminfo; dumpsys battery | grep -E 'level|status|temp'; echo CUR_FREQ_START; cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq; echo CUR_FREQ_END; echo 'END_OF_BATCH'\n",
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
        let mut core_usages: Vec<f32> = Vec::with_capacity(paths.core_count);
        let mut raw_numeric_dump: Vec<f32> = Vec::with_capacity(4);
        let mut cur_freqs: Vec<f32> = Vec::with_capacity(paths.core_count);
        let mut collecting_freqs = false;

        while let Some(Ok(line)) = lines.next() {
            if line == "END_OF_BATCH" { break; }
            if line == "CUR_FREQ_START" { collecting_freqs = true; continue; }
            if line == "CUR_FREQ_END" { collecting_freqs = false; continue; }
            if collecting_freqs {
                if let Ok(v) = line.trim().parse::<f32>() {
                    // scaling_cur_freq is in kHz on Android, convert to MHz
                    cur_freqs.push(v / 1000.0);
                }
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() { continue; }

            match parts[0] {
                "cpu" => {
                    let (t, i) = calculate_usage(&parts);
                    if prev_total > 0 {
                        let dt = t.saturating_sub(prev_total);
                        let di = i.saturating_sub(prev_idle);
                        if dt > 0 { total_cpu = (dt - di) as f32 / dt as f32 * 100.0; }
                    }
                    prev_total = t; prev_idle = i;
                }
                p if p.starts_with("cpu") && p.len() > 3 => {
                    if let Ok(core_idx) = p[3..].parse::<usize>() {
                        let (t, i) = calculate_usage(&parts);
                        let mut usage = 0.0;
                        if core_idx < core_snaps.len() {
                            let dt = t.saturating_sub(core_snaps[core_idx].total);
                            let di = i.saturating_sub(core_snaps[core_idx].idle);
                            if dt > 0 { usage = (dt - di) as f32 / dt as f32 * 100.0; }
                            core_snaps[core_idx] = CpuSnap { total: t, idle: i };
                        }
                        // Ensure vector is large enough
                        while core_usages.len() <= core_idx {
                            core_usages.push(0.0);
                        }
                        core_usages[core_idx] = usage;
                    }
                }
                "MemTotal:" => memory_total_gb = parts[1].parse::<f32>().unwrap_or(0.0) / 1048576.0,
                "MemAvailable:" => memory_available_gb = parts[1].parse::<f32>().unwrap_or(0.0) / 1048576.0,
                "level:" => battery_level = parts[1].parse().unwrap_or(0),
                "status:" => battery_status = map_status(parts[1].parse().unwrap_or(0)),
                "temperature:" => battery_temp = parts[1].parse::<f32>().unwrap_or(0.0) / 10.0,
                _ => {
                    if let Ok(val) = parts[0].parse::<f32>() {
                        raw_numeric_dump.push(val);
                    }
                }
            }
        }

        // Mapping: uptime, cpu_temp, gpu_temp
        if raw_numeric_dump.len() >= 3 {
            uptime_seconds = raw_numeric_dump[0] as u64;
            cpu_temp = raw_numeric_dump[1] / 1000.0;
            gpu_temp = raw_numeric_dump[2] / 1000.0;
        }

        // Build cores with static info + dynamic usage + current frequency
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
            memory_used_gb: memory_total_gb - memory_available_gb,
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
    let vals: Vec<u64> = p.iter().skip(1).take(8).filter_map(|s| s.parse().ok()).collect();
    if vals.len() < 5 { return (0, 0); }
    (vals.iter().sum(), vals[3] + vals.get(4).unwrap_or(&0))
}

fn map_status(c: i32) -> &'static str {
    match c { 2 => "Charging", 3 => "Discharging", 4 => "Not Charging", 5 => "Full", _ => "N/A" }
}
