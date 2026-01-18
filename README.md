# pasmonux
Device Monitoring API for Termux using Shizuku

Prerequisites
- Termux, Shizuku set up

Quick install

```sh
yes | pkg up
pkg install wget -y
wget -c https://github.com/theonuverse/pasmonux/releases/download/v0.1.0/{pasmonux,rish,rish_shizuku.dex}
chmod +x pasmonux rish
cp pasmonux, rish, rish_shizuku.dex $PREFIX/bin/
```

Usage
```sh
pasmonux
```

Project layout
- `Cargo.toml` — Cargo manifest
- `src/main.rs` — program entry
- `src/discover.rs`, `src/monitor.rs`, `src/types.rs` — modules