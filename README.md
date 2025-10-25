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

O descarga el binario compilado desde [Releases](https://github.com/lui5gl/git-sync/releases):

```bash
# Linux
wget https://github.com/lui5gl/git-sync/releases/latest/download/git-sync
chmod +x git-sync
sudo mv git-sync /usr/local/bin/

# Verificar instalación
git-sync --version
```

## Desinstalación

```bash
# Eliminar el binario
sudo rm /usr/local/bin/git-sync

# Eliminar configuración y logs (opcional)
rm -rf ~/.config/git-sync
```

## Configuración

Al ejecutar por primera vez, se creará automáticamente:

```
~/.config/git-sync/
├── config.toml        # Configuración del programa
├── repositories.txt   # Lista de repositorios
└── .log               # Archivo de logs
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

Edita `~/.config/git-sync/repositories.txt` y añade las rutas absolutas de tus repositorios:

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

### Modo quiet (silencioso)
Ideal para ejecutar en segundo plano con mínimo consumo de recursos:
```bash
# Ejecutar en background con salida mínima
git-sync --quiet &

# O con nohup para mantenerlo corriendo después de cerrar sesión
nohup git-sync --quiet > /dev/null 2>&1 &
```

El modo quiet (`-q` o `--quiet`) desactiva toda la salida verbose, manteniendo solo los errores críticos.

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
├── config.rs      # Configuración y lectura de repositories.txt
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
