# GitLab CD/CI - Repository Sync Daemon

Un daemon automatizado para mantener múltiples repositorios Git sincronizados con sus remotos.

## Características

- ✅ Sincronización automática de múltiples repositorios
- ✅ Loop continuo con espera de 60 segundos entre ciclos
- ✅ Detección automática de rama por defecto (main/master)
- ✅ Logging completo con timestamps en archivo `.log`
- ✅ **Se detiene ante cualquier error** (ideal para cron jobs)
- ✅ Configuración simple en `~/.gitlab-cd-ci/`

## Instalación

```bash
cargo build --release
sudo cp target/release/gitlab-cd-ci /usr/local/bin/
```

## Configuración

Al ejecutar por primera vez, se creará automáticamente:

```
~/.gitlab-cd-ci/
├── repos.txt    # Lista de repositorios
└── .log         # Archivo de logs
```

Edita `~/.gitlab-cd-ci/repos.txt` y añade las rutas absolutas de tus repositorios:

```
# Repositorios a sincronizar
/home/user/projects/repo1
/home/user/projects/repo2
/home/user/projects/repo3
```

## Uso

### Modo daemon (continuo)
```bash
gitlab-cd-ci
```

El programa ejecutará un loop infinito:
1. Revisa todos los repositorios
2. Hace fetch y pull si hay cambios
3. Espera 60 segundos
4. Repite

**Si hay algún error, el programa se detiene** (exit code 1).

### Con cron
```bash
# Cada 5 minutos
*/5 * * * * /usr/local/bin/gitlab-cd-ci >> /tmp/gitlab-cd-ci-cron.log 2>&1
```

Si un ciclo falla, el programa se detiene y cron no lo volverá a ejecutar hasta el siguiente intervalo.

### Logs

Los logs se guardan automáticamente en `~/.gitlab-cd-ci/.log`:

```
[2025-10-21 14:30:00] GitLab CD/CI - Starting repository sync daemon
[2025-10-21 14:30:00] Found 3 repository/repositories to check
[2025-10-21 14:30:01] Processing: /home/user/projects/repo1
[2025-10-21 14:30:01] ✅ Already up to date.
[2025-10-21 14:30:02] ✅ Cycle completed successfully. Waiting 60 seconds...
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
