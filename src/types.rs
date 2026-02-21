use std::sync::Arc;

use serde::Serialize;

// ---------------------------------------------------------------------------
// Battery status as a proper enum — no raw `&'static str` floating around.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Copy, Default, PartialEq, Eq)]
pub enum BatteryStatus {
    Charging,
    Discharging,
    #[serde(rename = "Not Charging")]
    NotCharging,
    Full,
    #[default]
    #[serde(rename = "N/A")]
    Unknown,
}

impl BatteryStatus {
    pub fn from_code(code: i32) -> Self {
        match code {
            2 => Self::Charging,
            3 => Self::Discharging,
            4 => Self::NotCharging,
            5 => Self::Full,
            _ => Self::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// Main stats payload — sent over the watch channel every tick.
// `Arc<str>` for strings that never change: cloning is a single atomic inc.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Default)]
pub struct SystemStats {
    pub manufacturer: Arc<str>,
    pub product_model: Arc<str>,
    pub soc_model: Arc<str>,
    pub kernel_version: Arc<str>,
    pub android_version: Arc<str>,

    pub uptime_seconds: u64,
    pub battery_level: i32,
    pub battery_status: BatteryStatus,
    pub battery_temp: f32,
    pub cpu_temp: f32,
    pub gpu_temp: f32,
    pub gpu_load: f32,
    pub memory_used_mb: f32,
    pub memory_total_mb: f32,
    pub swap_used_mb: f32,
    pub swap_total_mb: f32,
    pub storage_free_gb: f32,
    pub storage_total_gb: f32,
    pub refresh_rate: f32,
    pub brightness: f32,

    pub cores: Vec<CoreData>,
}

// ---------------------------------------------------------------------------
// Per-core snapshot included in every stats payload.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct CoreData {
    pub name: Arc<str>,
    pub usage: f32,
    pub model_name: Arc<str>,
    pub cur_freq: f32,
    pub min_freq: f32,
    pub max_freq: f32,
}

// ---------------------------------------------------------------------------
// Discovery-time data — built once, read forever.
// ---------------------------------------------------------------------------

pub struct StaticCoreInfo {
    pub name: Arc<str>,
    pub model_name: Arc<str>,
    pub min_freq: f32,
    pub max_freq: f32,
}

#[derive(Default)]
pub struct CpuSnap {
    pub total: u64,
    pub idle: u64,
}

pub struct StaticDeviceInfo {
    pub manufacturer: Arc<str>,
    pub product_model: Arc<str>,
    pub soc_model: Arc<str>,
    pub kernel_version: Arc<str>,
    pub android_version: Arc<str>,
    pub cores: Box<[StaticCoreInfo]>,
}

pub struct DevicePaths {
    pub cpu_temp: Box<str>,
    pub gpu_temp: Box<str>,
}