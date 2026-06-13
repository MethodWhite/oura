# Oura — Loop Engine

**Oura** (Ouroboros) es un servidor MCP para iteración inteligente automatizada. La serpiente que refina código a través de ciclos infinitos de mejora.

## Características

- **Loop Engine**: iteraciones automáticas con detección de convergencia
- **Feedback multi-fuente**: tests, lint, typecheck, custom
- **Sub-agentes**: Security Auditor, Refactor Engine, Anti-deletion Guard, Code Optimizer
- **Integración GitHub**: PRs, workflows, actions, auto-commit, multi-repo
- **Plugin system**: hooks extensibles para eventos del loop
- **Synapsis bridge**: persistencia en Synapsis memory + task orchestration
- **Config**: TOML + env vars (`OURA_*`)

## Instalación

```bash
cargo install --path .
```

O descarga el binario de [releases](https://github.com/MethodWhite/oura/releases).

## Configuración

```bash
# Iniciar config por defecto
oura --init

# O usar env vars
export OURA_GITHUB_TOKEN=ghp_xxx
export OURA_GITHUB_OWNER=MethodWhite
export OURA_GITHUB_REPO=my-project
export OURA_MAX_ITERATIONS=50
```

### Config file (`~/.config/oura/config.toml`)

```toml
[loop_engine]
max_iterations = 20
convergence_threshold = 90.0
feedback_sources = ["test", "lint", "typecheck"]

[github]
enabled = true
default_owner = "MethodWhite"
default_repo = "my-project"
auto_commit = true
auto_pr = true

[[github.repos]]
owner = "MethodWhite"
repo = "my-project"
branch = "develop"
base_branch = "main"
auto_sync = true

[synapsis]
enabled = true
endpoint = "http://localhost:7438"
```

## Uso con MCP

Añade a `opencode.json`:

```json
{
  "mcpServers": {
    "oura": {
      "type": "stdio",
      "command": "/path/to/oura"
    }
  }
}
```

### Herramientas MCP (12)

| Tool | Descripción |
|------|-------------|
| `oura_start_loop` | Inicia loop de iteración |
| `oura_iterate` | Ejecuta un paso manual |
| `oura_loop_status` | Estado del loop actual |
| `oura_loop_stop` | Detiene el loop |
| `oura_results` | Resultados acumulados |
| `oura_configure` | Actualiza configuración |
| `oura_plugin_load` | Carga un plugin |
| `oura_plugin_list` | Lista plugins cargados |
| `oura_analyze_security` | Auditoría de seguridad |
| `oura_analyze_code` | Análisis de clean code |
| `oura_check_integrity` | Verifica integridad de símbolos |
| `oura_guard_destructive` | Protege contra operaciones destructivas |

## Licencia

BSL 1.1 — ver [LICENSE](LICENSE).

---

Hecho con 🐍 por [MethodWhite](https://github.com/MethodWhite).
