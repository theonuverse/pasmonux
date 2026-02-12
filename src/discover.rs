use std::process::Command;
use std::sync::Arc;

use crate::types::{DevicePaths, StaticCoreInfo, StaticDeviceInfo};

// ---------------------------------------------------------------------------
// One-shot device discovery — runs at startup, never again.
// ---------------------------------------------------------------------------

pub fn discover_device_layout() -> (DevicePaths, StaticDeviceInfo) {
    let (cpu_temp, gpu_temp, core_count) = probe_thermal_and_cores();
    let (manufacturer, product_model, soc_model) = probe_device_props();
    let cores = probe_core_info(core_count);

    let paths = DevicePaths {
        cpu_temp: cpu_temp.into_boxed_str(),
        gpu_temp: gpu_temp.into_boxed_str(),
    };

    let static_info = StaticDeviceInfo {
        manufacturer: Arc::from(manufacturer),
        product_model: Arc::from(product_model),
        soc_model: Arc::from(soc_model),
        cores: cores.into_boxed_slice(),
    };

    (paths, static_info)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Probe sysfs thermal zones and CPU topology via `rish`.
fn probe_thermal_and_cores() -> (String, String, usize) {
    let output = Command::new("rish")
        .args([
            "-c",
            "for f in /sys/class/thermal/thermal_zone*/type; do \
                 echo \"$f:$(cat $f)\"; \
             done; \
             ls -d /sys/devices/system/cpu/cpu[0-9]* | sort -V",
        ])
        .output()
        .expect("sysfs probe via rish failed");

    let raw = String::from_utf8_lossy(&output.stdout);

    let mut cpu_temp = "/sys/class/thermal/thermal_zone0/temp".to_owned();
    let mut gpu_temp = "/sys/class/thermal/thermal_zone1/temp".to_owned();
    let mut core_count = 0_usize;

    for line in raw.lines() {
        if line.contains("/thermal_zone") {
            let lower = line.to_ascii_lowercase();
            if lower.contains("cpuss-0") || lower.contains("aoss-0") {
                if let Some(path) = line.split(':').next() {
                    cpu_temp = path.replace("type", "temp");
                }
            } else if lower.contains("gpuss-0")
                && let Some(path) = line.split(':').next()
            {
                gpu_temp = path.replace("type", "temp");
            }
        } else if line.contains("/cpu/cpu") {
            core_count += 1;
        }
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