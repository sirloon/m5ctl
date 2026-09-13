# m5ctl

A tiny terminal UI (and CLI) for **power mode + fan control** on the **Sixunited
AXB35-02** board — used in the **Bosgame M5**, GMKtec EVO-X2, FEVM FA-EX9,
Peladn YO1, NIMO AI MiniPC and Corsair AI Workstation 300.

Built with [ratatui](https://ratatui.rs) + [crossterm](https://crates.io/crates/crossterm).
**No GNOME / GTK / desktop environment required** — it runs in any terminal
(alacritty, kitty, xterm, a raw TTY, or over SSH).

## Features

- Live power-mode switching: `quiet` / `balanced` / `performance`
- Per-fan control: mode (`auto` / `fixed` / `curve`), level `0–5`, and editable
  ramp-up / ramp-down curves
- Rolling 60s graph — toggle between **fan RPM** and **CPU/GPU temperature**
- Sensors: CPU temp (+min/max since load), GPU temp, APU package power
- CLI for scripting (`get` / `json` / `set`)

## Requirements

`m5ctl` is a user-space program that talks to sysfs. It needs:

1. **Linux** with the [`ec_su_axb35`](https://github.com/cmetz/ec-su_axb35-linux)
   kernel module (GPL-2.0). This provides `/sys/class/ec_su_axb35/*` and is a
   **separate project** — it is *not* bundled with this app.
2. **amdgpu** — for GPU temperature and APU package power (read from `hwmon`).
3. **`video` group** — the writable EC files are `root:video` (`rw-rw-`). Add
   yourself with `sudo usermod -aG video $USER` then log out/in, or run as root.

Without the EC driver the app still launches and shows GPU sensors, with a hint
on how to install the driver (press `r` to retry once loaded).

## Install

### 1. Kernel driver (once)

```sh
git clone https://github.com/cmetz/ec-su_axb35-linux
cd ec-su_axb35-linux
make
sudo make install
sudo modprobe ec_su_axb35
```

See that repo's README for DKMS / Secure Boot details.

### 2. m5ctl

```sh
# build
cargo build --release

# install to ~/.cargo/bin (or copy the binary anywhere)
cargo install --path .
#   or: sudo cp target/release/m5ctl /usr/local/bin/
```

The result is a single static binary — no runtime dependencies beyond libc.

## Usage

```sh
m5ctl            # launch the TUI
m5ctl get        # print power mode + sensors
m5ctl json       # print full state as JSON
m5ctl set quiet  # set power mode (quiet|balanced|performance)
```

### TUI keys

| Key         | Action                                              |
|-------------|-----------------------------------------------------|
| `1` `2` `3` | Power: quiet / balanced / performance              |
| `Tab`       | Cycle focus: Power → Fan1 → Fan2 → Fan3 → Graph     |
| `m`         | (fan) cycle mode: auto / fixed / curve              |
| `←` `→`     | (fan) level 0–5 · (graph) switch fan RPM / temp     |
| `u` / `d`   | (fan) edit ramp-up / ramp-down curve                |
| `r`         | refresh / retry driver                              |
| `q` / `Ctrl+C` | quit                                            |

## Notes

- Fan levels map to duty: `0=0%, 1=20%, … 5=100%`.
- Ramp curves are 5 temperature thresholds (°C) for levels 1–5.
- This app is MIT; the `ec_su_axb35` kernel driver it talks to is GPL-2.0.
  They are separate works communicating over the sysfs interface.

## License

Released under the [MIT License](LICENSE).
