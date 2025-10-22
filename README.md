# Git Sync - Repository Sync Daemon

Un daemon automatizado para mantener múltiples repositorios Git sincronizados con sus remotos.

## Características

- ✅ Sincronización automática de múltiples repositorios
- ✅ Configuración flexible con `config.toml`
- ✅ Loop continuo o ejecución única
- ✅ Detección automática de rama por defecto (main/master)
- ✅ Logging completo con timestamps en archivo `.log`
- ✅ **Se detiene ante cualquier error** (ideal para cron jobs)
- ✅ Configuración en `~/.config/git-sync/`
- ✅ Recarga de configuración en caliente

## Instalación

```bash
cargo build --release
sudo cp target/release/git-sync /usr/local/bin/
```

## Configuración

Al ejecutar por primera vez, se creará automáticamente:

```
~/.config/git-sync/
├── config.toml  # Configuración del programa
├── repos.txt    # Lista de repositorios
└── .log         # Archivo de logs
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

Edita `~/.config/git-sync/repos.txt` y añade las rutas absolutas de tus repositorios:

```
# Repositorios a sincronizar
/home/user/projects/repo1
/home/user/projects/repo2
/home/user/projects/repo3
```

## Uso

### Modo daemon (continuo)
```bash
git-sync
```

El programa ejecutará un loop infinito:
1. Revisa todos los repositorios
2. Hace fetch y pull si hay cambios
3. Espera el tiempo configurado en `sync_interval`
4. Repite

**Si hay algún error y `stop_on_error=true`, el programa se detiene** (exit code 1).

### Modo único (sin loop)
Edita `~/.config/git-sync/config.toml` y establece:
```toml
continuous_mode = false
```

Luego ejecuta:
```bash
git-sync
```

### Con cron
```bash
# Cada 5 minutos
*/5 * * * * /usr/local/bin/git-sync >> /tmp/git-sync-cron.log 2>&1
```

Si un ciclo falla, el programa se detiene y cron no lo volverá a ejecutar hasta el siguiente intervalo.

### Logs

Los logs se guardan automáticamente en `~/.config/git-sync/.log`:

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
├── config.rs      # Configuración y lectura de repos.txt
├── git.rs         # Operaciones Git (fetch, pull, branches)
├── logger.rs      # Sistema de logging con timestamps
└── processor.rs   # Procesamiento de repositorios
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
