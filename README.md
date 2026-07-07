# DevTerminal

**Terminal Workspace Manager para Linux.** Una TUI que centraliza la administración de tu entorno de desarrollo en una única interfaz navegable solo con teclado.

DevTerminal no reimplementa nada: actúa como un panel de control unificado sobre las herramientas que ya usas — `git`, `gh`, `docker`, `kubectl`, `pacman`, `journalctl`, `ssh` — para que dejes de saltar entre terminales y recordar flags.

```
┌ DevTerminal ────────────────────────────────────────────────────────────┐
│ 1 Dashboard  2 Git  3 GitHub  4 Docker  5 K8s  6 Servidores  7 Procesos │
│              8 Pacman  9 Sistema                                        │
└─────────────────────────────────────────────────────────────────────────┘
```

## Módulos

| # | Módulo | Qué hace |
|---|--------|----------|
| 1 | **Dashboard** | CPU, RAM, swap y discos con gauges; uptime, carga, kernel, contenedores Docker activos y fecha |
| 2 | **Git** | Contextual: detecta el repo del directorio actual o te ofrece los de tus workspaces. Status con stage/unstage, diff (con [`delta`](https://github.com/dandavison/delta) a pantalla completa si está instalado), editar con `$EDITOR`, commit, pull, push, fetch, log con grafo y checkout de ramas |
| 3 | **GitHub** | Repos, issues, PRs y Actions vía `gh`: detalles, abrir en el navegador, reejecutar runs. Fija un repo de contexto con `Space` para operar fuera del cwd |
| 4 | **Docker** | Contenedores, imágenes, volúmenes, redes y proyectos Compose: start/stop/restart, logs, shell, inspect, eliminar, prune, `compose up/down` |
| 5 | **K8s** | Pods, deployments, services, nodes y namespaces (`-A`): logs, exec, rollout restart, escalar réplicas, delete |
| 6 | **Servidores** | Puertos de desarrollo en escucha (filtrados por una lista blanca de ~600 puertos típicos: vite, flask, postgres, n8n, ollama…) mostrados como URLs clicables, con aviso si están expuestos a la red; hosts SSH de `~/.ssh/config` y del config propio (conectar, `ssh-copy-id`, `ssh-keygen`); y red: interfaces, gateway, DNS, ping y traceroute |
| 7 | **Procesos** | Ordenados por CPU con filtro en vivo; kill, SIGKILL, renice |
| 8 | **Pacman** | Actualizaciones pendientes (`checkupdates`), buscar, instalar, eliminar y limpiar caché; usa `paru` o `yay` automáticamente si existen (AUR) |
| 9 | **Sistema** | `journalctl` con scroll y filtro de solo-errores + servicios systemd: logs por unidad, start/stop/restart |

## Filosofía

- **Envolver, no reimplementar.** Nada de bindings ni SDKs: cada módulo lanza el CLI oficial (`git`, `gh`, `docker`, `kubectl`…) y presenta su salida. Si sabes usar la herramienta, ya sabes qué hace DevTerminal por debajo.
- **100 % teclado.** Ningún flujo requiere ratón (aunque las URLs de puertos se pueden abrir con Ctrl+clic).
- **Sin secretos propios.** DevTerminal no almacena tokens: GitHub se usa a través de `gh`, que ya gestiona su autenticación en el keyring del sistema.
- **Acciones destructivas siempre con confirmación** (eliminar contenedores, matar procesos, `compose down`, prune…).

## Instalación

### Requisitos

- **Linux** (desarrollado y probado en Arch/CachyOS; el módulo Pacman es específico de Arch, el resto funciona en cualquier distro)
- **Rust estable** — `pacman -S rustup && rustup default stable`

### Compilar e instalar

```bash
git clone https://github.com/tomgutgar/DevTerminal.git
cd DevTerminal
cargo install --path .   # deja el binario `devc` en ~/.cargo/bin
devc
```

O sin instalar: `cargo run --release`.

### Herramientas opcionales

Cada módulo detecta si su herramienta falta y lo indica en pantalla; instala solo lo que uses:

| Herramienta | Para qué |
|---|---|
| `gh` | Módulo GitHub |
| `docker` (+ plugin compose) | Módulo Docker |
| `kubectl` | Módulo K8s |
| `paru` / `yay` | Paquetes AUR en el módulo Pacman |
| `pacman-contrib` | `checkupdates` (consulta sin tocar la BD de pacman) |
| `delta` | Diffs de Git con color a pantalla completa |
| `wl-clipboard` / `xclip` / `xsel` | Copiar con `y` (Wayland / X11) |
| `libnotify` | Aviso de escritorio si un comando tarda >10 s |
| `xdg-utils` | Abrir URLs de puertos en el navegador |
| `traceroute` | Traceroute en la vista Red |

## Uso

### Teclado global

| Tecla | Acción |
|---|---|
| `Tab` / `D` · `Shift+Tab` / `A` | Módulo siguiente · anterior |
| `1`–`9` | Saltar directamente a un módulo |
| `W` / `S` (o flechas) | Mover selección en listas |
| `/` o `F` | Filtrar la lista en vivo (`Esc` limpia) |
| `v` | Cambiar de vista dentro del módulo (ej. Docker: contenedores → imágenes → volúmenes…) |
| `Enter` | Acción principal (diff, logs, conectar, detalles…) |
| `y` | Copiar la selección al portapapeles (pid, hash, URL, ruta…) |
| `R` | Refrescar |
| `Esc` | Cerrar overlay (panel, confirmación, prompt) |
| `Q` | Salir |

Cada módulo muestra **sus atajos propios en la barra inferior**. Algunos ejemplos: en Git `Space` hace stage/unstage y `c` abre el prompt de commit; en Docker `u`/`x`/`t` son start/stop/restart y `e` abre un shell dentro del contenedor; en Procesos `k` mata el proceso seleccionado.

Los comandos interactivos (ssh, shells, `pacman -Syu`, ping…) suspenden la TUI, te ceden el terminal completo y al salir vuelves donde estabas.

### Configuración (opcional)

Archivo en `~/.config/devterminal/config.toml` (se respeta `$XDG_CONFIG_HOME`). Si no existe, se usan valores por defecto.

```toml
# Tema de color: re-colorea toda la TUI
theme = "nord"           # nord (por defecto) · catppuccin · gruvbox

# Directorios donde buscar repos cuando lanzas devc fuera de uno
# (solo un nivel; nunca se escanea el disco)
workspaces = ["~/Projects"]

# Servidores del módulo Servidores, además de los de ~/.ssh/config
[[servers]]
alias = "vps"
host = "1.2.3.4"
user = "root"
port = 22
desc = "VPS principal"
```

## Arquitectura

Crate binario único en Rust sobre [ratatui](https://ratatui.rs):

```
src/main.rs           Bucle de eventos, App y overlays (confirmar / prompt / panel)
src/runner.rs         Ejecución de comandos, detección de binarios, portapapeles, notificaciones
src/theme.rs          Paletas de color (nord / catppuccin / gruvbox)
src/ui.rs             ListView compartida (selección + filtro) y helpers de dibujo
src/config.rs         Config TOML
src/modules/          trait Module + un archivo por módulo (9 módulos)
```

Cada módulo implementa el trait `Module` (`refresh`, `draw`, `on_key`, `footer`) y devuelve acciones (`Run`, `Interactive`, `Prompt`, `Show`) que el bucle principal ejecuta — los diálogos de confirmación y los paneles de salida son una única implementación compartida.

Dependencias mínimas a propósito: `ratatui`, `sysinfo`, `serde` + `toml` y `anyhow`. Los parsers (porcelain de git, `ss`, `compose ls`, `~/.ssh/config`) son texto plano con tests, sin serde_json.

## Tests

```bash
cargo test
```

## Licencia

[PolyForm Noncommercial 1.0.0](LICENSE.md) — puedes usar, modificar y redistribuir DevTerminal libremente para cualquier propósito **no comercial**. No está permitido venderlo ni usarlo con fines comerciales.
