# VALORANT Local Tracker

Herramienta de línea de comandos (con panel TUI opcional) escrita en Rust que lee el estado local del cliente de Riot Games para VALORANT, guarda el historial de partidas en una base de datos SQLite y calcula métricas de rendimiento (HS%, KDA, ADR) sin depender de ningún servicio en la nube.

> Proyecto no oficial, hecho por y para fans. No está afiliado, respaldado ni patrocinado por Riot Games, Inc. "VALORANT" es una marca registrada de Riot Games, Inc.

## ¿Qué hace?

Mientras VALORANT está abierto, el Riot Client escribe en disco un archivo `lockfile` con el puerto y la contraseña de una API HTTPS local (`127.0.0.1`). Esta aplicación:

1. Lee y parsea ese `lockfile` para obtener el puerto, la contraseña y el protocolo.
2. Construye la cabecera de autenticación HTTP Basic (`Authorization: Basic base64("riot:<password>")`) que exige la API local.
3. Habla con esa API local (aceptando su certificado TLS autofirmado) para obtener el token de sesión, el `puuid` del jugador activo y su historial de partidas, deserializando **solo** los campos que necesita (no todo el JSON de Riot).
4. Persiste partidas y estadísticas por jugador en SQLite, usando transacciones para las inserciones masivas.
5. Calcula métricas agregadas (headshot %, KDA, ADR) con consultas SQL analíticas.
6. Sondea periódicamente el estado del juego (abierto/cerrado) y detecta cambios de cuenta (multiusuario) por `puuid`.
7. Expone todo esto vía CLI (`--summary`, `--history`) y, opcionalmente, un panel de terminal interactivo con `ratatui`.

## Áreas del proyecto

| Módulo | Responsabilidad |
|---|---|
| `src/lockfile.rs` | Localiza y parsea el `lockfile` del Riot Client (ruta real en Windows vía `%LOCALAPPDATA%`, mock en macOS/Linux). Lectura síncrona y asíncrona. |
| `src/riot_client.rs` | Cliente HTTP (`reqwest`, certificados autofirmados aceptados) y construcción de la cabecera Basic Auth. |
| `src/models.rs` | Structs `serde` de deserialización *cherry-pick*: solo los campos de la API de Riot que la app realmente usa. |
| `src/db.rs` | Esquema SQLite (`users`, `matches`, `player_match_stats`), inserciones masivas transaccionales y consultas analíticas (HS%, KDA, ADR). Todo el I/O de base de datos corre en `tokio::task::spawn_blocking`. |
| `src/daemon.rs` | Máquina de estados que sondea el juego (`tokio::time::interval`), detecta apertura/cierre y cambios de cuenta, silenciando errores de conexión cuando el juego está cerrado. |
| `src/cli.rs` / `src/main.rs` | Interfaz de línea de comandos (`clap`). |
| `src/tui.rs` | Panel de terminal opcional (`ratatui` + `crossterm`), con el estado (`App`) separado del bucle de dibujo/eventos para que sea testeable. |
| `mocks/` | Lockfile y respuestas JSON de ejemplo usadas por los tests y por la app mientras el juego está cerrado. |

## Filosofía de desarrollo: todo contra mocks

Este proyecto se desarrolla asumiendo que VALORANT y el Riot Client **no están abiertos**. Todos los tests, y por ahora también los comandos de la CLI, leen datos estáticos de `mocks/` en lugar de hacer peticiones reales a `127.0.0.1`. Esto evita bucles de "arreglar" código que en realidad falla por *connection refused*, y permite iterar sin tener el juego corriendo.

**Estado actual del flujo real de red:** el cliente HTTP, la cabecera Basic Auth y los modelos de deserialización ya están implementados y testeados, pero el flujo CLI todavía consume las funciones `fetch_token_mock()` / `fetch_match_history_mock()`. Conectar esas piezas a peticiones reales contra la API local del Riot Client (con el juego abierto) es el siguiente paso pendiente, no cubierto todavía por este README.

## Requisitos

- [Rust](https://rustup.rs) (edición 2021, toolchain estable).
- Para compilar el `.exe` de Windows desde macOS o Linux: el toolchain de cross-compilación `mingw-w64`.
  - macOS: `brew install mingw-w64`
  - Debian/Ubuntu: `sudo apt install gcc-mingw-w64-x86-64`

## Instalación

```bash
git clone https://github.com/hugoeljugo/LocalTracker.git
cd LocalTracker
cargo build
```

## Ejecutar los tests

```bash
cargo test
```

Todos los tests (unitarios y de integración) corren exclusivamente contra los archivos de `mocks/` o bases de datos SQLite en memoria (`:memory:`) — no abren conexiones de red ni tocan un `lockfile` real.

## Uso

```bash
# Estado del lockfile + cuenta activa (puuid)
cargo run -- --summary

# Historial de partidas reciente
cargo run -- --history

# Panel de terminal interactivo (pulsa 'q' para salir)
cargo run -- --tui

# Combinar flags
cargo run -- --summary --history
```

## Compilación cruzada para Windows

El binario final está pensado para distribuirse como `.exe` de Windows, aunque el desarrollo se haga en macOS/Linux.

```bash
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64   # o el equivalente en tu distro de Linux
cargo build --release --target x86_64-pc-windows-gnu
```

El repositorio ya incluye `.cargo/config.toml` con el linker configurado para ese target, así que no hace falta ninguna otra configuración manual. El binario resultante queda en:

```
target/x86_64-pc-windows-gnu/release/local_tracker.exe
```

## Esquema de la base de datos

```
users               (puuid PK)
matches             (match_id PK, game_start_time, queue_id, rounds_played)
player_match_stats  (match_id, puuid, kills, deaths, assists, headshots, bodyshots, legshots, damage_dealt)
```

Las inserciones usan `INSERT OR IGNORE` dentro de una transacción por partida, por lo que reprocesar la misma partida es idempotente.

## Privacidad y seguridad

- Toda la información (lockfile, tokens de sesión, historial de partidas) se procesa y almacena **localmente**; la aplicación no envía datos a ningún servidor propio ni de terceros.
- El código evita imprimir en terminal payloads JSON completos o tokens sin truncar, para no filtrar accidentalmente credenciales de sesión en logs.
- Los archivos de `mocks/` contienen únicamente datos ficticios (contraseñas, tokens y UUIDs de ejemplo) — no corresponden a ninguna cuenta ni sesión real.

## Licencia

Todavía no se ha definido una licencia formal para este proyecto.
