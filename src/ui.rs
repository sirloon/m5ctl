use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, Gauge, GraphType, Paragraph};
use ratatui::Frame;

use crate::app::{App, ChartMode, Focus, FAN_COUNT, HISTORY_SECS};
use crate::sysfs;

fn power_color(mode: &str) -> Color {
    match mode {
        "quiet" => Color::Green,
        "balanced" => Color::Yellow,
        "performance" => Color::Red,
        _ => Color::DarkGray,
    }
}

fn or_dash<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string())
}

fn focus_border(focused: bool) -> Style {
    Style::default().fg(if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    })
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    if !app.snap.driver_present {
        let mut lines = vec![
            Line::from(Span::styled(
                "EC driver (ec_su_axb35) not loaded.",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Power mode and fan control need the kernel driver."),
            Line::from(""),
            Line::from(Span::styled(
                "Get it: github.com/cmetz/ec-su_axb35-linux",
                Style::default().fg(Color::Cyan),
            )),
            Line::from("  make && sudo make install && sudo modprobe ec_su_axb35"),
            Line::from(""),
        ];
        let gpu = app
            .snap
            .gpu_temp
            .map(|v| format!("{:.0} °C", v))
            .unwrap_or_else(|| "-".into());
        let ppt = app
            .snap
            .apu_power
            .map(|v| format!("{:.1} W", v))
            .unwrap_or_else(|| "-".into());
        if app.snap.gpu_temp.is_some() || app.snap.apu_power.is_some() {
            lines.push(Line::from(vec![
                Span::styled("GPU temp  ", Style::default().fg(Color::DarkGray)),
                Span::styled(gpu, Style::default()),
                Span::styled("   APU PPT  ", Style::default().fg(Color::DarkGray)),
                Span::styled(ppt, Style::default()),
            ]));
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            "Press r to retry, q to quit.",
            Style::default().fg(Color::DarkGray),
        )));
        let p = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .title(" m5ctl "),
            )
            .alignment(Alignment::Center);
        f.render_widget(p, area);
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(12),
        Constraint::Min(8),
        Constraint::Length(4),
    ])
    .split(area);

    draw_header(f, chunks[0], app);

    let body = Layout::horizontal([Constraint::Length(30), Constraint::Min(1)]).split(chunks[1]);
    draw_sensors(f, body[0], app);
    draw_fans(f, body[1], app);

    draw_chart(f, chunks[2], app);

    draw_footer(f, chunks[3], app);
}

fn draw_chart(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Chart;
    let (title, series_names, series_colors, series_data, unit): (
        String,
        &[&str],
        &[Color],
        &[Vec<(f64, f64)>],
        &str,
    ) = match app.chart_mode {
        ChartMode::Temp => (
            format!(" Temperature · last {:.0}s ", HISTORY_SECS),
            &["cpu", "gpu"],
            &[Color::Red, Color::Blue],
            &app.temp_history,
            "°C",
        ),
        ChartMode::Fan => (
            format!(" Fan RPM · last {:.0}s ", HISTORY_SECS),
            &["fan1", "fan2", "fan3"],
            &[Color::Cyan, Color::Yellow, Color::Magenta],
            &app.fan_history,
            "",
        ),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_border(focused))
        .title(title);

    if area.height < 5 || area.width < 20 {
        f.render_widget(block, area);
        return;
    }

    let t_max_raw = series_data
        .iter()
        .flatten()
        .map(|(t, _)| *t)
        .fold(0.0_f64, f64::max);
    let t_min = (t_max_raw - HISTORY_SECS).max(0.0);
    let t_max = t_max_raw.max(t_min + 1.0);
    let v_max = series_data
        .iter()
        .flatten()
        .map(|(_, v)| *v)
        .fold(0.0_f64, f64::max);
    let step = if unit == "°C" { 10.0 } else { 500.0 };
    let floor_min = if unit == "°C" { 100.0 } else { 1000.0 };
    let y_max = ((v_max.max(floor_min) + step - 1.0) / step).floor() * step;

    let datasets: Vec<Dataset> = series_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            Dataset::default()
                .name(*name)
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(series_colors[i]))
                .data(&series_data[i])
        })
        .collect();

    let x_labels = vec![
        format!("{:.0}s", t_min),
        format!("{:.0}s", (t_min + t_max) / 2.0),
        format!("{:.0}s", t_max),
    ];
    let y_labels = vec![
        format!("0{}", unit),
        format!("{:.0}{}", y_max / 2.0, unit),
        format!("{:.0}{}", y_max, unit),
    ];

    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .bounds([t_min, t_max])
                .style(Style::default().fg(Color::DarkGray))
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, y_max])
                .style(Style::default().fg(Color::DarkGray))
                .labels(y_labels),
        );
    f.render_widget(chart, area);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let mode = app
        .snap
        .power_mode
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let color = power_color(&mode);
    let focused = app.focus == Focus::Power;
    let line = Line::from(vec![
        Span::styled("Power mode: ", Style::default().fg(Color::White)),
        Span::styled(
            mode.to_uppercase(),
            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let p = Paragraph::new(line)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(focus_border(focused))
                .title(" m5ctl · Bosgame M5 (AXB35-02) "),
        );
    f.render_widget(p, area);
}

fn draw_sensors(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.snap;
    let lines = vec![
        Line::from(vec![
            Span::styled("CPU      ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} °C", or_dash(s.cpu_temp)), Style::default()),
        ]),
        Line::from(vec![
            Span::styled("GPU      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                s.gpu_temp
                    .map(|v| format!("{:.0} °C", v))
                    .unwrap_or_else(|| "-".to_string()),
                Style::default(),
            ),
        ]),
        Line::from(vec![
            Span::styled("APU PPT  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                s.apu_power
                    .map(|v| format!("{:.1} W", v))
                    .unwrap_or_else(|| "-".to_string()),
                Style::default(),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("CPU min/max ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}/{} °C", or_dash(s.cpu_min), or_dash(s.cpu_max)),
                Style::default(),
            ),
        ]),
    ];
    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(focus_border(false))
            .title(" Sensors "),
    );
    f.render_widget(p, area);
}

fn draw_fans(f: &mut Frame, area: Rect, app: &App) {
    if app.snap.fans.len() < FAN_COUNT {
        return;
    }
    let chunks = Layout::vertical([Constraint::Ratio(1, 3); FAN_COUNT]).split(area);
    for i in 0..FAN_COUNT {
        let fan = &app.snap.fans[i];
        let focused = app.focus == Focus::Fan(i);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(focus_border(focused))
            .title(format!(" {} ", fan.label));
        f.render_widget(block, chunks[i]);

        let ia = chunks[i].inner(Margin::new(1, 1));
        if ia.height < 1 || ia.width < 10 {
            continue;
        }

        let top = Rect {
            x: ia.x,
            y: ia.y,
            width: ia.width,
            height: 1,
        };
        let topline = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{:>5} rpm  ", or_dash(fan.rpm)),
                Style::default(),
            ),
            Span::styled(
                format!("[{}]", fan.mode.clone().unwrap_or_else(|| "?".to_string())),
                Style::default().fg(Color::Magenta),
            ),
        ]));
        f.render_widget(topline, top);

        if ia.height < 2 {
            continue;
        }
        let level = fan.level.unwrap_or(0);
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Rgb(20, 20, 20)))
            .ratio(level as f64 / 5.0)
            .label(format!("level {}/5", level));
        let gl = Rect {
            x: ia.x,
            y: ia.y + 1,
            width: ia.width,
            height: 1,
        };
        f.render_widget(gauge, gl);

        if ia.height < 3 {
            continue;
        }
        let up = fan
            .rampup
            .as_ref()
            .map(|v| sysfs::join_csv(v))
            .unwrap_or_else(|| "-".to_string());
        let down = fan
            .rampdown
            .as_ref()
            .map(|v| sysfs::join_csv(v))
            .unwrap_or_else(|| "-".to_string());
        let cl = Rect {
            x: ia.x,
            y: ia.y + 2,
            width: ia.width,
            height: 1,
        };
        let curves = Paragraph::new(Line::from(Span::styled(
            format!("up {}  down {}", up, down),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(curves, cl);
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let keymap = match app.focus {
        Focus::Power => {
            "1 quiet   2 balanced   3 performance   Tab focus   r refresh   q/ctrl+c quit"
        }
        Focus::Fan(_) => {
            "m mode   <- / -> level   u ramp-up   d ramp-down   Tab focus   r refresh   q/ctrl+c quit"
        }
        Focus::Chart => {
            "<- / -> switch fan/temp   Tab focus   r refresh   q/ctrl+c quit"
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Controls ");
    f.render_widget(block, area);

    let ia = area.inner(Margin::new(1, 1));
    if ia.height < 2 {
        return;
    }

    let keys = Paragraph::new(Line::from(Span::styled(
        keymap,
        Style::default().fg(Color::DarkGray),
    )));
    let kl = Rect {
        x: ia.x,
        y: ia.y,
        width: ia.width,
        height: 1,
    };
    f.render_widget(keys, kl);

    match &app.input {
        Some(inp) => {
            let shown = format!("{}{}", inp.prompt, inp.buffer);
            let il = Rect {
                x: ia.x,
                y: ia.y + 1,
                width: ia.width,
                height: 1,
            };
            let p = Paragraph::new(Line::from(Span::styled(
                shown.clone(),
                Style::default().fg(Color::White),
            )));
            f.render_widget(p, il);
            let cx = (ia.x as usize + shown.chars().count()).min((ia.x + ia.width) as usize - 1);
            f.set_cursor_position((cx as u16, ia.y + 1));
        }
        None => {
            let sl = Rect {
                x: ia.x,
                y: ia.y + 1,
                width: ia.width,
                height: 1,
            };
            let p = Paragraph::new(Line::from(Span::styled(
                app.status.clone(),
                Style::default().fg(Color::Green),
            )));
            f.render_widget(p, sl);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn draw_with_chart_does_not_panic() {
        let mut app = App::new();
        app.temp_history[0] = vec![(0.0, 45.0), (1.0, 62.0), (2.0, 58.0)];
        app.temp_history[1] = vec![(0.0, 40.0), (1.0, 51.0)];
        app.fan_history[0] = vec![(0.0, 1000.0), (1.0, 1500.0)];
        app.chart_mode = ChartMode::Fan;
        let mut t = Terminal::new(TestBackend::new(100, 32)).unwrap();
        t.draw(|f| draw(f, &app)).unwrap();
    }

    #[test]
    fn draw_empty_history_does_not_panic() {
        let app = App::new();
        let mut t = Terminal::new(TestBackend::new(100, 32)).unwrap();
        t.draw(|f| draw(f, &app)).unwrap();
    }
}
