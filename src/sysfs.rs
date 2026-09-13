use std::fs;
use std::io;
use std::path::PathBuf;

pub const EC_BASE: &str = "/sys/class/ec_su_axb35";
pub const HWMON_ROOT: &str = "/sys/class/hwmon";

pub fn driver_present() -> bool {
    PathBuf::from(EC_BASE).exists()
}

fn read_trim(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_u32(path: &str) -> Option<u32> {
    read_trim(path).and_then(|s| s.parse().ok())
}

pub fn write_str(path: &str, value: &str) -> io::Result<()> {
    fs::write(path, value)
}

pub fn join_csv(v: &[i32]) -> String {
    v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
}

pub fn power_mode_path() -> String {
    format!("{}/apu/power_mode", EC_BASE)
}

pub fn fan_base(i: usize) -> String {
    format!("{}/fan{}", EC_BASE, i + 1)
}

pub fn read_power_mode() -> Option<String> {
    read_trim(&power_mode_path())
}

pub fn read_cpu_temp() -> Option<u32> {
    read_u32(&format!("{}/temp1/temp", EC_BASE))
}

pub fn read_cpu_min() -> Option<u32> {
    read_u32(&format!("{}/temp1/min", EC_BASE))
}

pub fn read_cpu_max() -> Option<u32> {
    read_u32(&format!("{}/temp1/max", EC_BASE))
}

pub fn find_amdgpu_hwmon() -> Option<PathBuf> {
    let rd = fs::read_dir(HWMON_ROOT).ok()?;
    for entry in rd.flatten() {
        let p = entry.path();
        if let Ok(name) = fs::read_to_string(p.join("name")) {
            if name.trim() == "amdgpu" {
                return Some(p);
            }
        }
    }
    None
}

pub fn read_gpu_temp() -> Option<f64> {
    let h = find_amdgpu_hwmon()?;
    let raw = read_trim(&h.join("temp1_input").to_str()?)?;
    raw.parse::<f64>().ok().map(|v| v / 1000.0)
}

pub fn read_apu_power() -> Option<f64> {
    let h = find_amdgpu_hwmon()?;
    let raw = read_trim(&h.join("power1_average").to_str()?)
        .or_else(|| read_trim(&h.join("power1_input").to_str()?));
    raw?.parse::<f64>().ok().map(|v| v / 1_000_000.0)
}

#[derive(Clone, Debug)]
pub struct Fan {
    pub label: String,
    pub rpm: Option<u32>,
    pub mode: Option<String>,
    pub level: Option<u8>,
    pub rampup: Option<Vec<i32>>,
    pub rampdown: Option<Vec<i32>>,
}

fn read_curve(path: &str) -> Option<Vec<i32>> {
    let s = read_trim(path)?;
    let v: Vec<i32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
    Some(v)
}

pub fn read_fan(i: usize) -> Fan {
    let base = fan_base(i);
    Fan {
        label: format!("fan{}", i + 1),
        rpm: read_u32(&format!("{}/rpm", base)),
        mode: read_trim(&format!("{}/mode", base)),
        level: read_trim(&format!("{}/level", base)).and_then(|s| s.parse().ok()),
        rampup: read_curve(&format!("{}/rampup_curve", base)),
        rampdown: read_curve(&format!("{}/rampdown_curve", base)),
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub driver_present: bool,
    pub power_mode: Option<String>,
    pub cpu_temp: Option<u32>,
    pub cpu_min: Option<u32>,
    pub cpu_max: Option<u32>,
    pub gpu_temp: Option<f64>,
    pub apu_power: Option<f64>,
    pub fans: Vec<Fan>,
}

pub fn read_snapshot() -> Snapshot {
    let present = driver_present();
    Snapshot {
        driver_present: present,
        power_mode: if present { read_power_mode() } else { None },
        cpu_temp: if present { read_cpu_temp() } else { None },
        cpu_min: if present { read_cpu_min() } else { None },
        cpu_max: if present { read_cpu_max() } else { None },
        gpu_temp: read_gpu_temp(),
        apu_power: read_apu_power(),
        fans: if present { (0..3).map(read_fan).collect() } else { Vec::new() },
    }
}

pub fn set_power_mode(mode: &str) -> io::Result<()> {
    write_str(&power_mode_path(), mode)
}

pub fn set_fan_mode(i: usize, mode: &str) -> io::Result<()> {
    write_str(&format!("{}/mode", fan_base(i)), mode)
}

pub fn set_fan_level(i: usize, level: u8) -> io::Result<()> {
    write_str(&format!("{}/level", fan_base(i)), &level.to_string())
}

pub fn set_fan_curve(i: usize, up: bool, values: &[i32]) -> io::Result<()> {
    let field = if up { "rampup_curve" } else { "rampdown_curve" };
    let s = values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");
    write_str(&format!("{}/{}", fan_base(i), field), &s)
}
