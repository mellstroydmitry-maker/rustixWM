# rustixWM

**rustixWM** is a minimalist, fast X11 window manager written in pure Rust for **FreeBSD** — the spiritual successor to **six wm** (originally written in C for OpenBSD / musl libc).

## Features
- **Zero C Wrapper Bloat:** Built directly on the X11 protocol via `x11rb`.
- **Built-in Procedural Wallpaper:** Generates a smooth dark gradient grid directly into an X11 server-side `Pixmap` on startup.
- **Hybrid Floating & Master-Stack Tiling:** Move and resize windows freely with the mouse or snap workspace windows into a clean master-stack layout on demand.
- **6 Virtual Workspaces:** Instant window mapping and unmapping across workspaces `1`–`6`.
- **Display Manager Ready:** Includes an `xsessions` desktop entry for SDDM / LightDM.

## Default Keybindings (`Mod4` = Super / Win)
- `Win + Enter` — Launch terminal (`TixTerm`)
- `Win + Q` — Close focused window (`WM_DELETE_WINDOW`)
- `Win + Tab` — Cycle focus through workspace windows
- `Win + T` — Tile current workspace windows (Master + Stack)
- `Win + F` — Fullscreen focused window
- `Win + 1..6` — Switch to workspace `1`–`6`
- `Win + Shift + 1..6` — Move focused window to workspace `1`–`6`
- `Win + Left Click` — Move window
- `Win + Right Click` — Resize window
- `Win + Shift + E` — Exit `rustixWM`

## Quick Install (FreeBSD 15 amd64)
Download `rustixWM-0.1.0.pkg` from **[Releases](https://github.com)** and install:
```sh
sudo pkg add rustixWM-0.1.0.pkg
```

## Building from Source
```sh
sudo pkg install -y rust libxcb xorg-server
cargo build --release
sudo cp target/release/rustixWM /usr/local/bin/rustixWM
```

## License
Licensed under the **BSD 3-Clause License**. See `LICENSE` for details.
