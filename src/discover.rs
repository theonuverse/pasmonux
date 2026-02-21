use std::fs;
use std::process::Command;
use std::sync::Arc;

use crate::types::{DevicePaths, StaticCoreInfo, StaticDeviceInfo};

// ---------------------------------------------------------------------------
// One-shot device discovery — runs at startup, never again.
// ---------------------------------------------------------------------------

pub fn discover_device_layout() -> (DevicePaths, StaticDeviceInfo) {
    let (cpu_temp, gpu_temp, core_count) = probe_thermal_and_cores();
    let (manufacturer, product_model, soc_model) = probe_device_props();
    let (kernel_version, android_version) = probe_system_versions();
    let cores = probe_core_info(core_count);

    let paths = DevicePaths {
        cpu_temp: cpu_temp.into_boxed_str(),
        gpu_temp: gpu_temp.into_boxed_str(),
    };

    let static_info = StaticDeviceInfo {
        manufacturer: Arc::from(manufacturer),
        product_model: Arc::from(product_model),
        soc_model: Arc::from(soc_model),
        kernel_version: Arc::from(kernel_version),
        android_version: Arc::from(android_version),
        cores: cores.into_boxed_slice(),
    };

    (paths, static_info)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Probe sysfs thermal zones and CPU topology directly (no `rish` needed).
fn probe_thermal_and_cores() -> (String, String, usize) {
    let mut cpu_temp = "/sys/class/thermal/thermal_zone0/temp".to_owned();
    let mut gpu_temp = "/sys/class/thermal/thermal_zone1/temp".to_owned();
    let mut core_count = 0_usize;

    // Scan thermal zones directly from sysfs.
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        let mut zones: Vec<_> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("thermal_zone"))
            .collect();
        zones.sort_by_key(|e| e.file_name());

        for entry in zones {
            let type_path = entry.path().join("type");
            let Ok(zone_type) = fs::read_to_string(&type_path) else { continue };
            let lower = zone_type.trim().to_ascii_lowercase();
            let temp_path = entry.path().join("temp").to_string_lossy().into_owned();

            if lower.contains("cpuss-0") || lower.contains("aoss-0") {
                cpu_temp = temp_path;
            } else if lower.contains("gpuss-0") {
                gpu_temp = temp_path;
            }
        }
    }

    // Count CPU cores directly from sysfs.
    // The glob `/cpu[0-9]*` matches cpu0, cpu1, …, cpu10, cpu99, etc.
    // The [0-9] prefix filters out non-core dirs like cpufreq and cpuidle.
    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        core_count = entries
            .filter_map(Result::ok)
            .filter(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("cpu")
                    && s.as_bytes().get(3).is_some_and(|b| b.is_ascii_digit())
            })
            .count();
    }

    (cpu_temp, gpu_temp, core_count)
}

/// Read device identity via Android `getprop`.
fn probe_device_props() -> (String, String, String) {
    let get = |key| -> String {
        Command::new("getprop")
            .arg(key)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default()
    };

    (
        get("ro.product.manufacturer"),
        get("ro.product.model"),
        get("ro.soc.model"),
    )
}

/// Read kernel and Android version (static, called once at startup).
fn probe_system_versions() -> (String, String) {
    let kernel_version = std::fs::read_to_string("/proc/version")
        .unwrap_or_default()
        .split_whitespace()
        .nth(2)
        .unwrap_or("unknown")
        .to_owned();

    let android_version = Command::new("getprop")
        .arg("ro.build.version.release")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();

    (kernel_version, android_version)
}

/// Gather static per-core info from `lscpu`.
fn probe_core_info(hint: usize) -> Vec<StaticCoreInfo> {
    let output = Command::new("lscpu")
        .args(["-e=cpu,modelname,minmhz,maxmhz"])
        .output()
        .expect("lscpu failed");

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut cores = Vec::with_capacity(hint);

    for line in raw.lines().skip(1) {
        let mut it = line.split_whitespace();
        let Some(cpu_str) = it.next() else { continue };
        let rest: Vec<&str> = it.collect();
        if rest.len() < 3 {
            continue;
        }

        let model_name = rest[..rest.len() - 2].join(" ");
        let min_freq: f32 = rest[rest.len() - 2].parse().unwrap_or(0.0);
        let max_freq: f32 = rest[rest.len() - 1].parse().unwrap_or(0.0);

        cores.push(StaticCoreInfo {
            name: Arc::from(format!("cpu{}", cpu_str).as_str()),
            model_name: Arc::from(model_name.as_str()),
            min_freq,
            max_freq,
        });
    }

    cores.sort_unstable_by(|a, b| {
        let num = |s: &str| s.get(3..).and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
        num(&a.name).cmp(&num(&b.name))
    });

    cores
}