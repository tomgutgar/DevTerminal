# DevTerminal

**Terminal Workspace Manager for Linux.** A TUI that centralizes the administration of your development environment in a single, fully keyboard-driven interface.

DevTerminal reimplements nothing: it acts as a unified control panel over the tools you already use — `git`, `gh`, `docker`, `kubectl`, `pacman`, `journalctl`, `ssh` — so you can stop juggling terminals and remembering flags.

## Screenshots

![DevTerminal dashboard](assets/screenshot-dashboard.png)

## Modules

| # | Module | What it does |
|---|--------|--------------|
| 1 | **Dashboard** | CPU, RAM, swap and disks with gauges; uptime, load, kernel, running Docker containers and date |
| 2 | **Git** | Contextual: detects the repo of the current directory or offers the ones in your workspaces. Status with stage/unstage, diff (full-screen via [`delta`](https://github.com/dandavison/delta) if installed), edit with `$EDITOR`, commit, pull, push, fetch, graph log and branch checkout |
| 3 | **GitHub** | Repos, issues, PRs and Actions via `gh`: details, open in browser, re-run workflows. Pin a context repo with `Space` to operate outside the cwd |
| 4 | **Docker** | Containers, images, volumes, networks and Compose projects: start/stop/restart, logs, shell, inspect, delete, prune, `compose up/down` |
| 5 | **K8s** | Pods, deployments, services, nodes and namespaces (`-A`): logs, exec, rollout restart, scale replicas, delete |
| 6 | **Servers** | Listening development ports (filtered by a whitelist of ~600 typical ports: vite, flask, postgres, n8n, ollama…) shown as clickable URLs, with a warning when exposed to the network; SSH hosts from `~/.ssh/config` and your own config (connect, `ssh-copy-id`, `ssh-keygen`); and networking: interfaces, gateway, DNS, ping and traceroute |
| 7 | **Processes** | Sorted by CPU with live filter; kill, SIGKILL, renice |
| 8 | **Pacman** | Pending updates (`checkupdates`), search, install, remove and clean cache; automatically uses `paru` or `yay` if present (AUR) |
| 9 | **System** | `journalctl` with scrolling and errors-only filter + systemd services: per-unit logs, start/stop/restart |

## Philosophy

- **Wrap, don't reimplement.** No bindings or SDKs: each module launches the official CLI (`git`, `gh`, `docker`, `kubectl`…) and presents its output. If you know the tool, you already know what DevTerminal does under the hood.
- **100% keyboard.** No workflow requires the mouse (although port URLs can be opened with Ctrl+click).
- **No secrets of its own.** DevTerminal stores no tokens: GitHub is used through `gh`, which already manages its authentication in the system keyring.
- **Destructive actions always ask for confirmation** (deleting containers, killing processes, `compose down`, prune…).

## Installation

### Requirements

- **Linux** (developed and tested on Arch/CachyOS; the Pacman module is Arch-specific, everything else works on any distro)
- **Stable Rust** — `pacman -S rustup && rustup default stable`

### Build and install

```bash
git clone https://github.com/tomgutgar/DevTerminal.git
cd DevTerminal
cargo install --path .   # puts the `devc` binary in ~/.cargo/bin
devc
```

Or without installing: `cargo run --release`.

### Optional tools

Each module detects whether its tool is missing and says so on screen; install only what you use:

| Tool | Used for |
|---|---|
| `gh` | GitHub module |
| `docker` (+ compose plugin) | Docker module |
| `kubectl` | K8s module |
| `paru` / `yay` | AUR packages in the Pacman module |
| `pacman-contrib` | `checkupdates` (queries without touching the pacman DB) |
| `delta` | Full-screen colored Git diffs |
| `wl-clipboard` / `xclip` / `xsel` | Copy with `y` (Wayland / X11) |
| `libnotify` | Desktop notification when a command takes >10 s |
| `xdg-utils` | Open port URLs in the browser |
| `traceroute` | Traceroute in the Network view |

## Usage

### Global keys

| Key | Action |
|---|---|
| `Tab` / `D` · `Shift+Tab` / `A` | Next · previous module |
| `1`–`9` | Jump straight to a module |
| `W` / `S` (or arrows) | Move selection in lists |
| `/` or `F` | Filter the list live (`Esc` clears) |
| `v` | Switch views inside a module (e.g. Docker: containers → images → volumes…) |
| `Enter` | Main action (diff, logs, connect, details…) |
| `y` | Copy the selection to the clipboard (pid, hash, URL, path…) |
| `R` | Refresh |
| `Esc` | Close overlay (pane, confirmation, prompt) |
| `Q` | Quit |

Every module shows **its own shortcuts in the bottom bar**. Some examples: in Git `Space` stages/unstages and `c` opens the commit prompt; in Docker `u`/`x`/`t` are start/stop/restart and `e` opens a shell inside the container; in Processes `k` kills the selected process.

Interactive commands (ssh, shells, `pacman -Syu`, ping…) suspend the TUI, hand you the full terminal, and return you where you were when they exit.

### Configuration (optional)

File at `~/.config/devterminal/config.toml` (`$XDG_CONFIG_HOME` is honored). If it doesn't exist, defaults are used.

```toml
# Color theme: re-colors the whole TUI
theme = "nord"           # nord (default) · catppuccin · gruvbox

# Directories to look for repos in when devc is launched outside one
# (one level only; the disk is never scanned)
workspaces = ["~/Projects"]

# Servers for the Servers module, in addition to ~/.ssh/config
[[servers]]
alias = "vps"
host = "1.2.3.4"
user = "root"
port = 22
desc = "Main VPS"
```

## Architecture

A single binary crate in Rust on top of [ratatui](https://ratatui.rs):

```
src/main.rs           Event loop, App and overlays (confirm / prompt / pane)
src/runner.rs         Command execution, binary detection, clipboard, notifications
src/theme.rs          Color palettes (nord / catppuccin / gruvbox)
src/ui.rs             Shared ListView (selection + filter) and drawing helpers
src/config.rs         TOML config
src/modules/          Module trait + one file per module (9 modules)
```

Each module implements the `Module` trait (`refresh`, `draw`, `on_key`, `footer`) and returns actions (`Run`, `Interactive`, `Prompt`, `Show`) that the main loop executes — confirmation dialogs and output panes are a single shared implementation.

Dependencies are minimal on purpose: `ratatui`, `sysinfo`, `serde` + `toml` and `anyhow`. The parsers (git porcelain, `ss`, `compose ls`, `~/.ssh/config`) are plain-text with tests, no serde_json.

## Tests

```bash
cargo test
```

## License

[GPL-3.0-or-later](LICENSE) — free software: you can use, study, modify and redistribute it; derivative works must remain under the same license.
