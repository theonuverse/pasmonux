use std::process::Command;
use crate::types::{StaticCoreInfo, StaticDeviceInfo, DevicePaths};

pub fn discover_device_layout() -> (DevicePaths, StaticDeviceInfo) {
    // Get thermal zones and CPU paths via rish (we still use rish for sysfs probing)
    let output = Command::new("rish")
        .args(["-c", "for f in /sys/class/thermal/thermal_zone*/type; do echo \"$f:$(cat $f)\"; done; ls -d /sys/devices/system/cpu/cpu[0-9]* | sort -V"])
        .output()
        .expect("Probing failed");

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut cpu_temp = "/sys/class/thermal/thermal_zone0/temp".to_string();
    let mut gpu_temp = "/sys/class/thermal/thermal_zone1/temp".to_string();
    let mut core_count = 0usize;

    for line in raw.lines() {
        if line.contains("/thermal_zone") {
            let l = line.to_lowercase();
            if l.contains("cpuss-0") || l.contains("aoss-0") {
                cpu_temp = line.split(':').next().unwrap().replace("type", "temp");
            } else if l.contains("gpuss-0") {
                gpu_temp = line.split(':').next().unwrap().replace("type", "temp");
            }
        } else if line.contains("/cpu/cpu") {
            core_count += 1;
        }
    }

    // Get device properties (manufacturer, model, soc) - run individually in Termux (not via rish)
    let manufacturer = Command::new("getprop")
        .arg("ro.product.manufacturer")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let product_model = Command::new("getprop")
        .arg("ro.product.model")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let soc_model = Command::new("getprop")
        .arg("ro.soc.model")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // Get CPU core info (model_name, min_freq, max_freq) - run directly in Termux, not via rish
    let lscpu_output = Command::new("lscpu")
        .args(["-e=cpu,modelname,minmhz,maxmhz"])
        .output()
        .expect("lscpu failed");

    let lscpu_raw = String::from_utf8_lossy(&lscpu_output.stdout);
    let mut cores: Vec<StaticCoreInfo> = Vec::with_capacity(core_count);

    for line in lscpu_raw.lines().skip(1) { // Skip header
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let cpu_num: usize = parts[0].parse().unwrap_or(0);
            let model_name = parts[1..parts.len()-2].join(" ");
            let min_freq: f32 = parts[parts.len()-2].parse().unwrap_or(0.0);
            let max_freq: f32 = parts[parts.len()-1].parse().unwrap_or(0.0);

            cores.push(StaticCoreInfo {
                name: format!("cpu{}", cpu_num),
                model_name,
                min_freq,
                max_freq,
            });
        }
    }

    // Sort cores by CPU number
    cores.sort_by(|a, b| {
        let a_num: usize = a.name[3..].parse().unwrap_or(0);
        let b_num: usize = b.name[3..].parse().unwrap_or(0);
        a_num.cmp(&b_num)
    });

    let paths = DevicePaths { cpu_temp, gpu_temp, core_count };
    let static_info = StaticDeviceInfo {
        manufacturer,
        product_model,
        soc_model,
        cores,
    };

    (paths, static_info)
}
