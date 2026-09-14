# Installing DevTerminal

DevTerminal runs on **Linux** (any distribution) and **Windows 10/11**. Pick one path:

- [Prebuilt binary](#prebuilt-binary) — download and run, nothing to compile.
- [Build from source](#build-from-source) — for other architectures, older glibc, or to hack on it.

Then install only the [optional tools](#optional-tools) for the modules you use.

---

## Prebuilt binary

Tagged releases ship `devc-linux-x86_64.tar.gz` and `devc-windows-x86_64.zip` on the [Releases page](https://github.com/tomgutgar/DevTerminal/releases).

### Linux

The binary is linked against glibc 2.35, so it runs on Ubuntu 22.04+, Debian 12+, Fedora 36+ and anything newer. On musl distros (Alpine) or older glibc, [build from source](#build-from-source).

```bash
mkdir -p ~/.local/bin
curl -fsSL https://github.com/tomgutgar/DevTerminal/releases/latest/download/devc-linux-x86_64.tar.gz \
  | tar -xz -C ~/.local/bin
devc
```

`~/.local/bin` is on `PATH` by default on Ubuntu, Debian and Fedora once the directory exists (log out and back in the first time). If `devc` isn't found, add `export PATH="$HOME/.local/bin:$PATH"` to your `~/.bashrc` or `~/.zshrc`.

### Windows

In PowerShell (no administrator needed):

```powershell
$dir = "$env:LOCALAPPDATA\Programs\devc"
New-Item -ItemType Directory -Force $dir | Out-Null
Invoke-WebRequest https://github.com/tomgutgar/DevTerminal/releases/latest/download/devc-windows-x86_64.zip -OutFile "$env:TEMP\devc.zip"
Expand-Archive "$env:TEMP\devc.zip" -DestinationPath $dir -Force

# Add it to your user PATH (only once), then open a new terminal
[Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ";$dir", 'User')
```

Open a **new** Windows Terminal tab and run `devc`.

- The executable isn't code-signed. If you downloaded the zip with a browser, SmartScreen may warn: *More info → Run anyway*, or run `Unblock-File "$dir\devc.exe"` once.
- It needs the Microsoft Visual C++ runtime, which almost every Windows install already has. If it complains about `VCRUNTIME140.dll`: `winget install Microsoft.VCRedist.2015+.x64`.

---

## Build from source

You need Git, a C linker and stable Rust. Use **rustup** rather than your distro's `rustc` package — Debian's and Ubuntu LTS's are usually too old for the dependencies.

### Debian / Ubuntu / Linux Mint / Pop!_OS

```bash
sudo apt update
sudo apt install -y build-essential curl git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
```

### Fedora / RHEL / Rocky / Alma

```bash
sudo dnf install -y gcc git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
```

### openSUSE

```bash
sudo zypper install -y gcc git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
```

### Alpine

```bash
sudo apk add build-base git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
```

### Arch / CachyOS / Manjaro

```bash
sudo pacman -S --needed base-devel git rustup
rustup default stable
```

### Windows

Install Git and rustup:

```powershell
winget install Git.Git Rustlang.Rustup
```

Rust on Windows needs a linker. Choose **one** of the two toolchains:

**A) MSVC (Rust's default)** — needs the Visual Studio Build Tools, a few GB:

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
rustup default stable-x86_64-pc-windows-msvc
```

**B) GNU (lighter)** — MinGW instead of Visual Studio:

```powershell
winget install BrechtSanders.WinLibs.POSIX.MSVCRT
rustup default stable-x86_64-pc-windows-gnu
rustup component add rust-mingw
```

With toolchain B, WinLibs' `mingw64\bin` folder must be on `PATH` while compiling (winget installs it under `%LOCALAPPDATA%\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.MSVCRT_*\mingw64\bin`). For the current PowerShell session:

```powershell
$env:Path = (Resolve-Path "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.MSVCRT_*\mingw64\bin").Path + ";$env:Path"
```

Open a new terminal after installing so `cargo` and `git` are on `PATH`.

### Compile and install (all platforms)

```bash
git clone https://github.com/tomgutgar/DevTerminal.git
cd DevTerminal
cargo install --path .    # puts devc in ~/.cargo/bin (%USERPROFILE%\.cargo\bin on Windows)
devc
```

Or run it without installing: `cargo run --release`. To update later: `git pull && cargo install --path .`.

---

## Optional tools

Every module checks whether its tool is present and says so on screen, so install only what you use. What each one is for:

| Tool | Module | Linux | Windows |
|---|---|---|---|
| `gh` | GitHub | ✔ | ✔ |
| `docker` + compose plugin | Docker | ✔ | ✔ (Docker Desktop) |
| `kubectl` | K8s | ✔ | ✔ |
| `delta` | Git (colored full-screen diffs) | ✔ | ✔ |
| `ss`, `ip` (iproute2) | Servers (ports, network) | usually preinstalled | not needed |
| `wl-clipboard` / `xclip` | `y` copy | ✔ | not needed (`clip`) |
| `notify-send` | notification when a command takes >10 s | ✔ | not needed |
| `xdg-open` | open port URLs | ✔ | not needed |
| `traceroute` | Servers › Network | ✔ | not needed (`tracert`) |
| `ssh`, `ssh-keygen` | Servers › SSH | ✔ | built in (OpenSSH Client) |
| `checkupdates` | Packages (Arch only) | `pacman-contrib` | — |

### Debian / Ubuntu

```bash
sudo apt install -y gh git-delta iproute2 wl-clipboard xclip libnotify-bin xdg-utils traceroute openssh-client
```

`git-delta` is packaged from Debian 13 and Ubuntu 24.04; on older releases grab the `.deb` from [delta's releases](https://github.com/dandavison/delta/releases). For Docker and kubectl use the official repositories: [Docker Engine](https://docs.docker.com/engine/install/) · [kubectl](https://kubernetes.io/docs/tasks/tools/install-kubectl-linux/).

### Fedora

```bash
sudo dnf install -y gh git-delta iproute wl-clipboard xclip libnotify xdg-utils traceroute openssh-clients
```

kubectl is packaged per Kubernetes version (`kubernetes1.36-client`, `kubernetes1.35-client`…): `dnf search kubernetes client` and install the one matching your cluster. Docker: [Docker Engine for Fedora](https://docs.docker.com/engine/install/fedora/) (or `podman-docker`, which provides a compatible `docker` command).

### openSUSE

```bash
sudo zypper install -y gh git-delta iproute2 wl-clipboard xclip libnotify-tools xdg-utils traceroute openssh-clients docker docker-compose
```

kubectl: [install kubectl on Linux](https://kubernetes.io/docs/tasks/tools/install-kubectl-linux/).

### Arch

```bash
sudo pacman -S --needed github-cli git-delta iproute2 wl-clipboard xclip libnotify xdg-utils traceroute openssh docker docker-compose kubectl pacman-contrib
```

### Windows

```powershell
winget install GitHub.cli dandavison.delta Kubernetes.kubectl Docker.DockerDesktop Microsoft.WindowsTerminal
```

- **Windows Terminal** is strongly recommended (it's preinstalled on Windows 11): the classic console host renders neither RGB colors nor rounded borders properly.
- **OpenSSH Client** ships with Windows 10 1809+ and Windows 11. If `ssh` is missing, as administrator: `Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0`.
- **Package managers**: `winget` comes with the system. [Scoop](https://scoop.sh) (`irm get.scoop.sh | iex`) and [Chocolatey](https://chocolatey.org/install) are also detected, in the order winget › scoop › choco.

---

## Platform notes

**Linux**

- Package installs/removals and service start/stop/restart go through `sudo`, which prompts for your password in the terminal.
- The Packages module uses the first manager it finds: `paru` › `yay` › `pacman` › `apt` › `dnf` › `zypper` › `apk`.
- The System module needs **systemd** (`journalctl`, `systemctl`). On distros without it (Alpine, Void, Devuan, WSL with systemd disabled) that module reports it's unavailable; everything else works.
- WSL counts as Linux. To get the System module there, enable systemd in `/etc/wsl.conf` (`[boot]` → `systemd=true`).

**Windows**

- Service start/stop/restart opens a UAC prompt. `winget` raises UAC by itself when a package needs it.
- The System module reads the *System* and *Application* event logs; a service's "log" is its details plus what the Service Control Manager recorded about it.
- Windows' OpenSSH has no `ssh-copy-id`; pressing `c` in Servers › SSH shows the equivalent PowerShell command instead.
- The Dashboard shows load average as *n/a* — Windows doesn't have one.

---

## Configuration

Optional. If the file doesn't exist, defaults are used.

| Platform | Path |
|---|---|
| Linux | `~/.config/devterminal/config.toml` (or `$XDG_CONFIG_HOME/devterminal/config.toml`) |
| Windows | `%APPDATA%\devterminal\config.toml` |

`~` inside the file expands to `$HOME` on Linux and `%USERPROFILE%` on Windows. See the [README](README.md#configuration-optional) for the available keys.

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| `devc: command not found` / `devc is not recognized` | The install folder isn't on `PATH`: `~/.local/bin` or `~/.cargo/bin` on Linux, `%LOCALAPPDATA%\Programs\devc` or `%USERPROFILE%\.cargo\bin` on Windows. Open a new terminal after changing it. |
| `version 'GLIBC_2.xx' not found` | Your glibc is older than the prebuilt binary's. [Build from source](#build-from-source). |
| Windows build: `linker 'link.exe' not found` | MSVC toolchain without Build Tools. Install them (option A) or switch to the GNU toolchain (option B). |
| Windows build: `error calling dlltool 'dlltool.exe': program not found` or `dlltool ... CreateProcess` | GNU toolchain without WinLibs on `PATH`. Add its `mingw64\bin` folder (see option B). |
| Broken borders, wrong colors | Use Windows Terminal (Windows) or any truecolor terminal (Linux). |
| `VCRUNTIME140.dll was not found` | `winget install Microsoft.VCRedist.2015+.x64` |
| A module says its tool is missing | Install it from [Optional tools](#optional-tools) and press `R`. |

---

## Uninstall

**Linux**

```bash
rm ~/.local/bin/devc            # prebuilt
cargo uninstall devterminal     # built from source
rm -rf ~/.config/devterminal    # optional: config
```

**Windows**

```powershell
Remove-Item -Recurse "$env:LOCALAPPDATA\Programs\devc"   # prebuilt (also remove it from your user PATH)
cargo uninstall devterminal                               # built from source
Remove-Item -Recurse "$env:APPDATA\devterminal"           # optional: config
```
