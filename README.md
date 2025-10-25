# Git Sync - Repository Sync Daemon

Un daemon automatizado para mantener múltiples repositorios Git sincronizados con sus remotos.

## ¿Por qué este proyecto?

Este proyecto surgió de la necesidad de tener una solución de sincronización automática de repositorios cuando **no es posible usar CI/CD de GitLab** debido a:

- **Limitaciones de versión**: Versiones antiguas de GitLab que no soportan las funciones modernas de CI/CD
- **Problemas de configuración**: Restricciones en la infraestructura o configuración del servidor GitLab
- **Ambientes restringidos**: Entornos donde no se puede configurar o activar GitLab CI/CD

En lugar de depender de la infraestructura de GitLab, `git-sync` proporciona una solución independiente y ligera que se ejecuta localmente, permitiendo mantener sincronizados múltiples repositorios de forma automática sin necesidad de configuraciones complejas de servidor.

## Características

- ✅ Sincronización automática de múltiples repositorios
- ✅ Instalación directa como servicio `systemd`
- ✅ Configuración centralizada en `/etc/git-sync/config.toml`
- ✅ Loop continuo o ejecución única
- ✅ Detección automática de rama por defecto (main/master)
- ✅ Logging completo en `/var/log/git-sync/git-sync.log`
- ✅ **Se detiene ante cualquier error** (ideal para cron jobs)
- ✅ Recarga de configuración en caliente

## Instalación

```bash
cargo build --release
sudo cp target/release/git-sync /usr/local/bin/
sudo git-sync      # Primera ejecución: instala el servicio y abre la TUI
```

O descarga el binario compilado desde [Releases](https://github.com/lui5gl/git-sync/releases) y luego instala el servicio:

```bash
# Linux
wget https://github.com/lui5gl/git-sync/releases/latest/download/git-sync
chmod +x git-sync
sudo mv git-sync /usr/local/bin/

# Inicializar e instalar el servicio systemd
sudo git-sync

# Verificar instalación
git-sync --version
```

Edita `/etc/git-sync/config.toml` y `/etc/git-sync/repositories.txt` con privilegios de administrador para ajustar la configuración y la lista de repos sincronizados. Luego reinicia el servicio con `sudo systemctl restart git-sync`.

### Compatibilidad con distribuciones antiguas (CentOS 7 y anteriores)

Si al ejecutar el binario precompilado aparece un error de carga relacionado con `GLIBC`, es porque el ejecutable fue enlazado
dinámicamente contra una versión más reciente de la biblioteca estándar de GNU. Las versiones antiguas de CentOS envían una
versión muy desactualizada de `glibc`, por lo que el binario no puede iniciarse. Para estos entornos hay **un artefacto
precompilado adicional** en la sección de Releases llamado `git-sync-linux-amd64-musl`, que está enlazado estáticamente con
`musl` y no depende de `glibc`. Si prefieres compilarlo tú mismo, ejecuta:

```bash
sudo dnf install musl-gcc # o yum install musl-gcc en CentOS antiguos
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
sudo cp target/x86_64-unknown-linux-musl/release/git-sync /usr/local/bin/
```

Al usar `musl` el binario resultante no depende de la versión de `glibc` del sistema y se ejecutará correctamente en CentOS
antiguos.

## Desinstalación

1. Detén y elimina el servicio:
   ```bash
   sudo git-sync uninstall-service
   ```

2. (Opcional) elimina el binario, la configuración y los logs:
   ```bash
   sudo rm /usr/local/bin/git-sync
   sudo rm -rf /etc/git-sync
   sudo rm -rf /var/log/git-sync
   ```

## Comandos disponibles

```bash
sudo git-sync              # Abre la interfaz TUI (instala el servicio si es necesario)
git-sync daemon            # Ejecuta el daemon de sincronización (lo usa systemd)
git-sync uninstall-service # Detiene y elimina el servicio systemd
git-sync --help            # Muestra ayuda
```

## Configuración

Al ejecutar por primera vez, se creará automáticamente:

```
/etc/git-sync/
├── config.toml        # Configuración del servicio
└── repositories.txt   # Lista de repositorios a sincronizar

/var/log/git-sync/
└── git-sync.log       # Archivo de logs con timestamp
```

### config.toml

```toml
# Tiempo de espera entre ciclos de sincronización (en segundos)
sync_interval = 60

# Detener el programa si hay algún error
stop_on_error = true

# Timeout para operaciones git (en segundos)
git_timeout = 300

# Número máximo de reintentos en caso de fallo temporal
max_retries = 0

# Mostrar output detallado
verbose = true

# Ejecutar en modo continuo (loop infinito)
continuous_mode = true
```

Edita `/etc/git-sync/repositories.txt` (requiere sudo) y añade las rutas absolutas de tus repositorios:

```
# Repositorios a sincronizar
/home/user/projects/repo1
/home/user/projects/repo2
/home/user/projects/repo3
```

### Gestor interactivo de repositorios (TUI)

Si prefieres no editar archivos a mano, ejecuta:

```bash
sudo git-sync
```

El gestor basado en `ratatui` permite:
- Navegar con ↑/↓
- Añadir (`a`), editar (`e` o Enter) y eliminar (`d`) rutas
- Guardar con Enter y salir con `q` o `Esc`

Los cambios se escriben directamente en `/etc/git-sync/repositories.txt`.

## Uso

### Como servicio systemd (recomendado)

Después de la primera ejecución de `sudo git-sync`, `systemd` arrancará el daemon en segundo plano:

```bash
sudo systemctl status git-sync
```

La unidad utiliza la cuenta del usuario que ejecutó la instalación y corre el binario directamente, leyendo la configuración desde `/etc/git-sync`. Los logs se guardan en `/var/log/git-sync/git-sync.log`.

Para aplicar cambios, edita los archivos de configuración y el servicio recargará la configuración en el siguiente ciclo.

### Ejecución manual (opcional)

Si deseas validar el comportamiento sin systemd, ejecuta directamente:
```bash
git-sync
```

El proceso se comportará exactamente igual que cuando lo arranca el servicio: recorre todos los repositorios, sincroniza y continúa en loop continuo (o único si `continuous_mode = false`).

### Logs

Los logs se guardan automáticamente en `/var/log/git-sync/git-sync.log`:

```
[2025-10-21 14:30:00] Git Sync - Repository synchronization daemon
[2025-10-21 14:30:00] Found 3 repository/repositories to check
[2025-10-21 14:30:01] Processing: /home/user/projects/repo1
[2025-10-21 14:30:01] ✅ Already up to date.
[2025-10-21 14:30:02] ✅ Cycle completed successfully.
```

## Estructura del código

```
src/
├── main.rs        # Loop principal y manejo de errores
├── config.rs      # Configuración y lectura de repositories.txt
├── git.rs         # Operaciones Git (fetch, pull, branches)
├── logger.rs      # Sistema de logging con timestamps
├── service.rs     # Instalación/desinstalación del servicio systemd
├── processor.rs   # Procesamiento de repositorios
└── tui.rs         # Gestor interactivo de repositorios (ratatui)
```

## Comportamiento ante errores

El programa se detiene inmediatamente si:
- ❌ Un repositorio no existe
- ❌ Una ruta no es un repositorio Git válido
- ❌ Falla el fetch de un repositorio
- ❌ Falla el pull de un repositorio

Esto es ideal para cron jobs, ya que evita ciclos infinitos de errores.

## Licencia

MIT
