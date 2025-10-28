# Git Sync - Daemon de Sincronización de Repositorios

Git Sync es un daemon ligero que automatiza la sincronización de múltiples repositorios Git con sus remotos, sin depender de pipelines externos ni de infraestructura adicional.

## Motivación

Git Sync responde a la necesidad de mantener repositorios alineados cuando no es viable utilizar GitLab CI/CD:

- Instalaciones de GitLab en versiones antiguas que no soportan funcionalidades modernas de CI/CD
- Restricciones de infraestructura o políticas que impiden configurar pipelines
- Ambientes cerrados en los que no se autoriza el uso de GitLab CI/CD

La herramienta se ejecuta en el entorno local del usuario, opera de manera independiente y mantiene los repositorios actualizados mediante ciclos de sincronización regulares.

## Características principales

- Sincronización automatizada de múltiples repositorios en un mismo ciclo
- Instalación directa como servicio `systemd`
- Configuración centralizada en `/etc/git-sync/config.toml`
- Funcionamiento en modo continuo o ejecución única
- Detección automática de la rama principal (`main` o `master`)
- Registro detallado en `/var/log/git-sync/git-sync.log`
- Interrupción inmediata ante errores (ideal para tareas programadas)
- Recarga dinámica de la configuración sin reinicios manuales

## Instalación

Compile desde el código fuente:

```bash
cargo build --release
sudo cp target/release/git-sync /usr/local/bin/
sudo git-sync      # Primera ejecución: instala el servicio y abre la TUI
```

También puede descargar el binario precompilado desde la sección de [releases](https://github.com/lui5gl/git-sync/releases):

```bash
# Linux
wget https://github.com/lui5gl/git-sync/releases/latest/download/git-sync
chmod +x git-sync
sudo mv git-sync /usr/local/bin/

# Inicialización e instalación del servicio systemd
sudo git-sync

# Verificación de la instalación
git-sync --version
```

Edite `/etc/git-sync/config.toml` y `/etc/git-sync/repositories.txt` con privilegios elevados para ajustar parámetros y definir los repositorios a sincronizar. Reinicie el servicio con `sudo systemctl restart git-sync` después de realizar cambios.

### Distribuciones antiguas (CentOS 7 y anteriores)

Si el binario precompilado falla por dependencias de `GLIBC`, utilice el artefacto `git-sync-linux-amd64-musl.tar.gz` disponible en la sección de releases. Dentro del archivo comprimido encontrará un ejecutable enlazado estáticamente con `musl`, generado automáticamente por GitHub Actions y apto para sistemas con versiones heredadas de `glibc`.

```bash
wget https://github.com/lui5gl/git-sync/releases/latest/download/git-sync-linux-amd64-musl.tar.gz
tar -xzf git-sync-linux-amd64-musl.tar.gz
chmod +x git-sync-linux-amd64-musl
sudo mv git-sync-linux-amd64-musl /usr/local/bin/git-sync
```

Gracias al enlazado estático con `musl` no es necesario recompilar localmente para CentOS 7 o derivadas: la acción de GitHub publica el binario listo para usar en cada release.

> **Nota:** al publicar manualmente una versión desde la pestaña **Releases**, GitHub Actions reconstruye automáticamente el binario y adjunta los artefactos `git-sync-linux-amd64.tar.gz` y `git-sync-linux-amd64-musl.tar.gz`. Si necesita reintentar el proceso, use el botón **Run workflow** del flujo "Build and Release" e introduzca el tag correspondiente.

## Desinstalación

1. Detenga y elimine el servicio:
   ```bash
   sudo git-sync uninstall-service
   ```

2. (Opcional) retire el binario, la configuración y los registros:
   ```bash
   sudo rm /usr/local/bin/git-sync
   sudo rm -rf /etc/git-sync
   sudo rm -rf /var/log/git-sync
   ```

## Comandos

```bash
sudo git-sync              # Abre la interfaz TUI (e instala el servicio si aún no existe)
git-sync daemon            # Ejecuta el daemon de sincronización (lo invoca systemd)
git-sync uninstall-service # Detiene y elimina el servicio systemd
git-sync --help            # Muestra ayuda
```

## Configuración

La primera ejecución crea de manera automática:

```
/etc/git-sync/
├── config.toml        # Configuración del servicio
└── repositories.txt   # Lista de repositorios a sincronizar

/var/log/git-sync/
└── git-sync.log       # Archivo de registro con marca de tiempo
```

### `config.toml`

```toml
# Intervalo entre ciclos de sincronización (segundos)
sync_interval = 60

# Finalizar el programa ante cualquier error
stop_on_error = true

# Tiempo máximo para operaciones Git (segundos)
git_timeout = 300

# Número máximo de reintentos ante fallos temporales
max_retries = 0

# Activar salida detallada
verbose = true

# Ejecutar en modo continuo
continuous_mode = true
```

Defina las rutas absolutas de los repositorios en `/etc/git-sync/repositories.txt`:

```
# Repositorios a sincronizar
/home/user/projects/repo1
/home/user/projects/repo2
/home/user/projects/repo3
```

### Gestor interactivo de repositorios (TUI)

El comando `sudo git-sync` abre la interfaz basada en `ratatui`, que permite:

- Desplazamiento con ↑/↓
- Añadir (`a`), modificar (`e` o Enter) y eliminar (`d`) rutas
- Guardar con Enter y salir con `q` o `Esc`

Las modificaciones se escriben directamente en `/etc/git-sync/repositories.txt`.

## Uso

### Servicio `systemd` (recomendado)

Tras la primera ejecución de `sudo git-sync`, el servicio queda instalado y en operación:

```bash
sudo systemctl status git-sync
```

La unidad utiliza la cuenta que ejecutó la instalación, lee la configuración desde `/etc/git-sync` y registra la actividad en `/var/log/git-sync/git-sync.log`. La configuración se recarga automáticamente en el siguiente ciclo después de cualquier actualización.

### Ejecución manual

Para validar el comportamiento sin `systemd`, ejecute:

```bash
git-sync
```

El proceso recorre los repositorios, sincroniza cada uno y continúa en modo continuo (o finaliza tras un ciclo si `continuous_mode = false`).

### Registros

Los registros se conservan en `/var/log/git-sync/git-sync.log` con un formato legible:

```
[2025-10-21 14:30:00] Git Sync - Daemon de sincronización de repositorios
[2025-10-21 14:30:00] Se analizarán 3 repositorios
[2025-10-21 14:30:01] Procesando repositorio: /home/user/projects/repo1
[2025-10-21 14:30:01] El repositorio ya está actualizado.
[2025-10-21 14:30:02] Ciclo completado correctamente.
```

## Arquitectura del código

```
src/
├── main.rs        # Punto de entrada y gestión de errores
├── config.rs      # Lectura de configuración y lista de repositorios
├── git.rs         # Operaciones Git (fetch, pull, ramas)
├── logger.rs      # Sistema de registro con marcas de tiempo
├── service.rs     # Instalación y eliminación del servicio systemd
├── processor.rs   # Orquestación del ciclo de sincronización
└── tui.rs         # Interfaz interactiva basada en ratatui
```

## Manejo de errores

El daemon detiene la ejecución ante cualquiera de los siguientes eventos:

- Ruta inexistente
- Directorio sin repositorio Git válido
- Error en la operación `git fetch`
- Error en la operación `git pull`

Este comportamiento evita bucles fallidos en ejecuciones programadas.

## Licencia

MIT
