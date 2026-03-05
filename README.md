# Git Sync

Daemon ligero que mantiene múltiples repositorios Git al día. Nació para reemplazar pipelines de CI/CD en entornos con pocas herramientas disponibles: se instala como servicio `systemd`, vigila tus repositorios locales y ejecuta sincronizaciones sin depender de servidores externos.

> Soporte de plataforma: **solo Linux con systemd**.

---

## Características destacadas

- 🔁 **Sincronización automática** de cualquier número de repositorios Git locales desde su `origin`.
- ✅ **Flujo único de producción**: no existe modo `Development`; el daemon sincroniza repositorios usando rutas locales configuradas.
- 🧭 **Detección de rama**: usa la rama remota declarada como HEAD (`origin/main`, `origin/master`, etc.); si no existe, intenta con `main` y luego con `master`.
- 🖥️ **Interfaz TUI** (terminal) para añadir, editar o eliminar repositorios sin tocar archivos manualmente.
- 🪵 **Logging persistente** en `/var/log/git-sync/git-sync.log` con marcas de tiempo y mensajes claros (emojis incluidos).
- ⚙️ **Configuración declarativa** en `/etc/git-sync`, creada automáticamente con permisos apropiados.
- ♻️ **Modo demonio continuo** o ejecución única configurable, con relectura automática de ajustes entre ciclos.
- 📦 **Artefactos oficiales**: binarios estáticos para Linux glibc (`git-sync-linux-x86_64-glibc.tar.gz`) y musl (`git-sync-linux-x86_64-musl.tar.gz`).

---

## Instalación rápida

### Compilar desde el código fuente

```bash
cargo build --release
sudo cp target/release/git-sync /usr/local/bin/
sudo git-sync          # Primera ejecución: crea la configuración y abre la TUI
```

### Usar un release publicado

1. Descarga el artefacto deseado desde la sección **Releases**.
2. Descomprime y mueve el binario a tu `PATH`:

```bash
tar -xzf git-sync-linux-x86_64-glibc.tar.gz
sudo mv git-sync /usr/local/bin/
sudo git-sync
```

Para sistemas antiguos (CentOS 7, Alpine, etc.) utiliza el artefacto musl:

```bash
tar -xzf git-sync-linux-x86_64-musl.tar.gz
sudo mv git-sync /usr/local/bin/
sudo git-sync
```

La primera ejecución instala el servicio `systemd`, crea los directorios necesarios y abre la TUI para que cargues repositorios.

---

## Estructura de configuración

```
/etc/git-sync/
├── config.toml        # Ajustes generales
└── repositories.txt   # Repositorios sincronizados

/var/log/git-sync/
├── git-sync.log       # Registro persistente del daemon
└── state.toml         # Estado de último intento/éxito/error por repositorio
```

### `config.toml`

```toml
sync_interval = 60          # Segundos entre ciclos de sincronización
stop_on_error = true        # Detener el daemon ante el primer error
git_timeout = 300           # Timeout para operaciones Git
max_retries = 0             # Reintentos para fallos transitorios
verbose = true              # Incluir mensajes detallados en el log
continuous_mode = true      # Ciclos infinitos (false = una sola pasada)
```

### `repositories.txt`

Formato soportado:

```
/home/deploy/repos/mi-api

# Desactivar temporalmente el sync para un repo
! /home/deploy/repos/mi-api-pausada
```

- Cada línea debe contener la ruta absoluta a un repositorio Git válido ya clonado en el servidor.
- Prefijo `!` = repositorio pausado (no se sincroniza hasta volver a activarlo).
- Entradas con formato antiguo `origen => destino` se leen, pero el destino se ignora.

Puedes editar el archivo a mano o usar la TUI (`sudo git-sync`) para que el formato se mantenga sin errores.

---

## Interfaz TUI

Ejecuta `sudo git-sync` (sin argumentos) para abrir la consola interactiva:

- `↑/↓` navegar, `Enter` o `e` editar, `a` añadir, `d` eliminar, `s` activar/pausar sync, `Espacio` ver detalles, `q/Esc` salir.
- Al añadir un repositorio:
  1. Ingresas la ruta absoluta al directorio del repositorio **ya clonado** (no la URL remota).
- Los mensajes de estado aparecen en la parte inferior con colores y emojis.
- La vista de detalles muestra rama detectada, último commit aplicado por pull, último error y últimos commits locales.

---

## Servicio `systemd`

La primera ejecución instala y habilita la unidad `git-sync`:

```bash
sudo systemctl status git-sync        # Ver estado
sudo systemctl restart git-sync       # Reiniciar después de cambios
sudo git-sync uninstall-service       # Deshabilitar y borrar la unidad
```

Los comandos `systemctl` exitosos no imprimen nada para evitar ruido; cualquier advertencia o error aparece con marca temporal:

```
[2025-02-14 10:12:33] ⚠️ systemctl enable --now git-sync finalizó con el estado 1. Es posible que deba ejecutarlo manualmente.
```

---

## Actualización del binario

Puedes actualizar `git-sync` directamente desde GitHub Releases:

```bash
git-sync update                    # Actualiza a la última versión estable
git-sync --add-current             # Pregunta si agrega el directorio actual
```

`git-sync` detecta automáticamente si usar artefacto `glibc` o `musl` en Linux x86_64.

---

## Funcionamiento interno

1. **Detección de rama**: se intenta leer `refs/remotes/origin/HEAD`. Si no existe, se prueba `origin/main`; si tampoco, `origin/master`.
2. **Sincronización**:
   - `git fetch`
   - Contar commits pendientes (`rev-list HEAD..origin/<branch>`)
   - Si hay diferencias, `git pull origin <branch>`
3. **Registro**: todas las acciones se anotan en `/var/log/git-sync/git-sync.log` con hora y emojis para ubicar fácilmente éxitos (`✅`), advertencias (`⚠️`) y fallos (`❌`).

---

## Desarrollo

Requisitos locales:

- Rust estable (edición 2024).
- `cargo fmt` y `cargo clippy` para formatear y analizar.
- Para el target musl (`cargo build --target x86_64-unknown-linux-musl`) instala previamente `musl-tools`.

Publicación:

- El workflow `.github/workflows/build.yml` compila dos artefactos (`glibc` y `musl`) y los adjunta al release correspondiente (`v*`).
- Después de subir un tag `vX.Y.Z`, GitHub Actions generará automáticamente los paquetes `.tar.gz`.

---

## Licencia

Este proyecto se distribuye bajo la licencia MIT. Revisa el archivo `LICENSE` para más detalles.
