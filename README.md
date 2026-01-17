# pasmonux
Device Monitoring API for Termux using Shizuku

Prerequisites
- Termux, Shizuku
- `git` and `rust` (see quick install below)
- Shizuku and `rish` set up

```sh
yes | pkg up
pkg install git rust
```

Clone the repo

```sh
git clone https://github.com/theonuverse/pasmonux.git
cd pasmonux
```

Install

```sh
echo "export PATH=$PATH:$HOME/.cargo/bin" > $HOME/.bashrc
source $HOME/.bashrc
cargo install --path .
```

Usage
```sh
pasmonux
```

Project layout
- `Cargo.toml` — Cargo manifest
- `src/main.rs` — program entry
- `src/discover.rs`, `src/monitor.rs`, `src/types.rs` — modules