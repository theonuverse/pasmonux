use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct SystemStats {
    pub manufacturer: String,
    pub product_model: String,
    pub soc_model: String,
    pub total_cpu: f32,
    pub memory_used_gb: f32,
    pub memory_total_gb: f32,
    pub cpu_temp: f32,
    pub gpu_temp: f32,
    pub battery_temp: f32,
    pub battery_level: i32,
    pub battery_status: &'static str,
    pub uptime_seconds: u64,
    pub cores: Vec<CoreData>,
}

impl Default for SystemStats {
    fn default() -> Self {
        Self {
            manufacturer: String::new(),
            product_model: String::new(),
            soc_model: String::new(),
            total_cpu: 0.0,
            memory_used_gb: 0.0,
            memory_total_gb: 0.0,
            cpu_temp: 0.0,
            gpu_temp: 0.0,
            battery_temp: 0.0,
            battery_level: 0,
            battery_status: "N/A",
            uptime_seconds: 0,
            cores: Vec::new(),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct CoreData {
    pub name: String,
    pub usage: f32,
    pub model_name: String,
    pub cur_freq: f32,
    pub min_freq: f32,
    pub max_freq: f32,
}

#[derive(Clone)]
pub struct StaticCoreInfo {
    pub name: String,
    pub model_name: String,
    pub min_freq: f32,
    pub max_freq: f32,
}

#[derive(Clone)]
pub struct CpuSnap {
    pub total: u64,
    pub idle: u64,
}

pub struct StaticDeviceInfo {
    pub manufacturer: String,
    pub product_model: String,
    pub soc_model: String,
    pub cores: Vec<StaticCoreInfo>,
}

pub struct DevicePaths {
    pub cpu_temp: String,
    pub gpu_temp: String,
    pub core_count: usize,
}
