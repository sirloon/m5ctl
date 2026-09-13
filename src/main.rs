mod app;
mod sysfs;
mod ui;

use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("tui");
    match cmd {
        "tui" | "" => run_tui(),
        "get" => cli_get(),
        "json" => cli_json(),
        "set" => cli_set(&args),
        "-h" | "--help" | "help" => print_help(),
        other => {
            eprintln!("unknown command: {other}");
            print_help();
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!(
        "m5ctl - Bosgame M5 (AXB35-02) power + fan control\n\n\
         USAGE:\n\
         \x20 m5ctl                 launch the TUI\n\
         \x20 m5ctl get             print power mode + sensors\n\
         \x20 m5ctl json            print full state as JSON\n\
         \x20 m5ctl set <mode>      set power mode (quiet|balanced|performance)\n\n\
         TUI KEYS:\n\
         \x20 1/2/3   power quiet/balanced/performance\n\
         \x20 Tab     cycle focus: Power -> Fan1 -> Fan2 -> Fan3 -> Graph\n\
         \x20 <-/->   (on graph) switch fan RPM / temperature\n\
         \x20 m       cycle focused fan mode (auto/fixed/curve)\n\
         \x20 <-/->   focused fan level 0..5\n\
         \x20 u/d     edit focused fan ramp-up / ramp-down curve\n\
         \x20 r       refresh   q / ctrl+c  quit\n"
    );
}

fn run_tui() {
    let mut app = app::App::new();
    ratatui::run(|terminal| loop {
        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
        if event::poll(Duration::from_millis(500)).unwrap() {
            match event::read().unwrap() {
                Event::Key(k) if k.kind == KeyEventKind::Press => app.on_key(k),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        if !app.running {
            break;
        }
        app.tick();
    });
}

fn cli_get() {
    let s = sysfs::read_snapshot();
    if !s.driver_present {
        eprintln!("ec_su_axb35 driver not loaded");
        std::process::exit(1);
    }
    println!("power mode: {}", s.power_mode.unwrap_or_else(|| "?".into()));
    println!(
        "cpu temp:   {}",
        s.cpu_temp.map(|v| format!("{v} C")).unwrap_or_else(|| "-".into())
    );
    println!(
        "gpu temp:   {}",
        s.gpu_temp.map(|v| format!("{v:.0} C")).unwrap_or_else(|| "-".into())
    );
    println!(
        "apu ppt:    {}",
        s.apu_power.map(|v| format!("{v:.1} W")).unwrap_or_else(|| "-".into())
    );
    for fan in &s.fans {
        println!(
            "{}: {:>5} rpm  mode {}  level {}",
            fan.label,
            fan.rpm.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            fan.mode.clone().unwrap_or_else(|| "-".into()),
            fan.level.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
        );
    }
}

fn cli_json() {
    let s = sysfs::read_snapshot();
    if !s.driver_present {
        eprintln!("ec_su_axb35 driver not loaded");
        std::process::exit(1);
    }
    let opt_str = |o: &Option<String>| match o {
        Some(v) => format!("\"{v}\""),
        None => "null".to_string(),
    };
    let opt_num = |o: Option<f64>| o.map(|v| format!("{v}")).unwrap_or_else(|| "null".into());
    let opt_int = |o: Option<u32>| o.map(|v| v.to_string()).unwrap_or_else(|| "null".into());
    let curve = |c: &Option<Vec<i32>>| match c {
        Some(v) => format!("[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")),
        None => "null".to_string(),
    };
    let fans: Vec<String> = s
        .fans
        .iter()
        .map(|fan| {
            format!(
                "{{\"label\":\"{}\",\"rpm\":{},\"mode\":{},\"level\":{},\"rampup\":{},\"rampdown\":{}}}",
                fan.label,
                opt_int(fan.rpm),
                opt_str(&fan.mode),
                fan.level.map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
                curve(&fan.rampup),
                curve(&fan.rampdown),
            )
        })
        .collect();
    println!(
        "{{\"power_mode\":{},\"cpu_temp\":{},\"cpu_min\":{},\"cpu_max\":{},\"gpu_temp\":{},\"apu_power\":{},\"fans\":[{}]}}",
        opt_str(&s.power_mode),
        opt_int(s.cpu_temp),
        opt_int(s.cpu_min),
        opt_int(s.cpu_max),
        opt_num(s.gpu_temp),
        opt_num(s.apu_power),
        fans.join(","),
    );
    io::stdout().flush().ok();
}

fn cli_set(args: &[String]) {
    let mode = match args.get(1) {
        Some(m) => m.as_str(),
        None => {
            eprintln!("usage: m5ctl set <quiet|balanced|performance>");
            std::process::exit(2);
        }
    };
    if !matches!(mode, "quiet" | "balanced" | "performance") {
        eprintln!("invalid mode: {mode}");
        std::process::exit(2);
    }
    if let Err(e) = sysfs::set_power_mode(mode) {
        eprintln!("write failed: {e} (are you in the video group?)");
        std::process::exit(1);
    }
    println!("power mode set to {mode}");
}
