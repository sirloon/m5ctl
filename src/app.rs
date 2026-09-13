use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::sysfs::{self, Snapshot};

pub const FAN_COUNT: usize = 3;
pub const TEMP_SERIES: usize = 2;
pub const HISTORY_SECS: f64 = 60.0;
pub const HISTORY_LEN: usize = 600;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Power,
    Fan(usize),
    Chart,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChartMode {
    Temp,
    Fan,
}

pub enum InputTarget {
    FanCurveUp(usize),
    FanCurveDown(usize),
}

pub struct Input {
    pub prompt: String,
    pub buffer: String,
    pub target: InputTarget,
}

pub struct App {
    pub snap: Snapshot,
    pub focus: Focus,
    pub status: String,
    pub running: bool,
    pub input: Option<Input>,
    pub start: Instant,
    pub temp_history: [Vec<(f64, f64)>; TEMP_SERIES],
    pub fan_history: [Vec<(f64, f64)>; FAN_COUNT],
    pub chart_mode: ChartMode,
}

impl App {
    pub fn new() -> Self {
        let snap = sysfs::read_snapshot();
        let status = if snap.driver_present {
            String::from("ready")
        } else {
            String::from("ec_su_axb35 driver not loaded")
        };
        App {
            snap,
            focus: Focus::Power,
            status,
            running: true,
            input: None,
            start: Instant::now(),
            temp_history: std::array::from_fn(|_| Vec::new()),
            fan_history: std::array::from_fn(|_| Vec::new()),
            chart_mode: ChartMode::Temp,
        }
    }

    pub fn tick(&mut self) {
        self.snap = sysfs::read_snapshot();
        if !self.snap.driver_present {
            return;
        }
        let t = self.start.elapsed().as_secs_f64();
        let cutoff = t - HISTORY_SECS;
        let temps = [self.snap.cpu_temp.map(|v| v as f64), self.snap.gpu_temp];
        for i in 0..TEMP_SERIES {
            let temp = temps[i].unwrap_or(0.0);
            let h = &mut self.temp_history[i];
            h.push((t, temp));
            while h.len() > 1 && (h[0].0 < cutoff || h.len() > HISTORY_LEN) {
                h.remove(0);
            }
        }
        for i in 0..FAN_COUNT {
            let rpm = self.snap.fans.get(i).and_then(|f| f.rpm).unwrap_or(0) as f64;
            let h = &mut self.fan_history[i];
            h.push((t, rpm));
            while h.len() > 1 && (h[0].0 < cutoff || h.len() > HISTORY_LEN) {
                h.remove(0);
            }
        }
    }

    fn focus_index(&self) -> i32 {
        match self.focus {
            Focus::Power => 0,
            Focus::Fan(i) => i as i32 + 1,
            Focus::Chart => FAN_COUNT as i32 + 1,
        }
    }

    pub fn cycle_focus(&mut self, dir: i32) {
        let total = (FAN_COUNT + 2) as i32;
        let m = ((self.focus_index() + dir) % total + total) % total;
        self.focus = if m == 0 {
            Focus::Power
        } else if m == total - 1 {
            Focus::Chart
        } else {
            Focus::Fan((m - 1) as usize)
        };
    }

    pub fn toggle_chart_mode(&mut self) {
        self.chart_mode = match self.chart_mode {
            ChartMode::Temp => ChartMode::Fan,
            ChartMode::Fan => ChartMode::Temp,
        };
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }

        if self.input.is_some() {
            self.on_input_key(key.code);
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Tab => self.cycle_focus(1),
            KeyCode::BackTab => self.cycle_focus(-1),
            KeyCode::Char('r') => {
                self.tick();
                self.status = String::from("refreshed");
            }
            KeyCode::Char('1') => self.set_power("quiet"),
            KeyCode::Char('2') => self.set_power("balanced"),
            KeyCode::Char('3') => self.set_power("performance"),
            _ => match self.focus {
                Focus::Fan(_) => self.on_fan_key(key.code),
                Focus::Chart => {
                    if key.code == KeyCode::Left || key.code == KeyCode::Right {
                        self.toggle_chart_mode();
                    }
                }
                Focus::Power => {}
            },
        }
    }

    fn on_input_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.input = None;
                self.status = String::from("input cancelled");
            }
            KeyCode::Enter => self.commit_input(),
            KeyCode::Backspace => {
                if let Some(i) = &mut self.input {
                    i.buffer.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(i) = &mut self.input {
                    i.buffer.push(c);
                }
            }
            _ => {}
        }
    }

    fn set_power(&mut self, mode: &str) {
        match sysfs::set_power_mode(mode) {
            Ok(()) => self.status = format!("power mode -> {}", mode),
            Err(e) => self.status = format!("write failed: {} (need video group?)", e),
        }
        self.tick();
    }

    fn on_fan_key(&mut self, code: KeyCode) {
        let i = match self.focus {
            Focus::Fan(i) => i,
            _ => return,
        };
        let label = self.snap.fans[i].label.clone();
        match code {
            KeyCode::Char('m') => {
                let cur = self.snap.fans[i].mode.clone().unwrap_or_default();
                let next = match cur.as_str() {
                    "auto" => "fixed",
                    "fixed" => "curve",
                    _ => "auto",
                };
                match sysfs::set_fan_mode(i, next) {
                    Ok(()) => self.status = format!("{} mode -> {}", label, next),
                    Err(e) => self.status = format!("write failed: {}", e),
                }
            }
            KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => self.adjust_level(i, 1),
            KeyCode::Left | KeyCode::Char('-') => self.adjust_level(i, -1),
            KeyCode::Char('u') => self.start_curve_input(i, true),
            KeyCode::Char('d') => self.start_curve_input(i, false),
            _ => {}
        }
        self.tick();
    }

    fn adjust_level(&mut self, i: usize, delta: i8) {
        let cur = self.snap.fans[i].level.unwrap_or(0) as i8;
        let ni = (cur + delta).clamp(0, 5) as u8;
        let label = self.snap.fans[i].label.clone();
        match sysfs::set_fan_level(i, ni) {
            Ok(()) => self.status = format!("{} level -> {}", label, ni),
            Err(e) => self.status = format!("write failed: {}", e),
        }
    }

    fn start_curve_input(&mut self, i: usize, up: bool) {
        let cur = if up {
            self.snap.fans[i].rampup.clone()
        } else {
            self.snap.fans[i].rampdown.clone()
        };
        let curstr = cur.map(|v| sysfs::join_csv(&v)).unwrap_or_default();
        let field = if up { "ramp-up" } else { "ramp-down" };
        let label = self.snap.fans[i].label.clone();
        self.input = Some(Input {
            prompt: format!("{} {} curve (5 vals 0-100, comma-sep): ", label, field),
            buffer: curstr,
            target: if up {
                InputTarget::FanCurveUp(i)
            } else {
                InputTarget::FanCurveDown(i)
            },
        });
    }

    fn commit_input(&mut self) {
        let input = match self.input.take() {
            Some(i) => i,
            None => return,
        };
        let vals: Vec<i32> = input
            .buffer
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<i32>().unwrap_or(-1))
            .collect();
        if vals.len() != 5 || vals.iter().any(|v| *v < 0 || *v > 100) {
            self.status = format!(
                "invalid curve: need 5 values 0-100 (got '{}')",
                input.buffer
            );
            return;
        }
        let (i, up) = match input.target {
            InputTarget::FanCurveUp(i) => (i, true),
            InputTarget::FanCurveDown(i) => (i, false),
        };
        let label = self.snap.fans[i].label.clone();
        match sysfs::set_fan_curve(i, up, &vals) {
            Ok(()) => self.status = format!("{} curve set to {}", label, sysfs::join_csv(&vals)),
            Err(e) => self.status = format!("write failed: {}", e),
        }
        self.tick();
    }
}
