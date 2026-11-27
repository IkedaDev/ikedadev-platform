---
description: Ejecuta la batería completa de validación de calidad y compilación en Rust, portal/bff y portal/web.
---

# Workflow `/validate` — Validación Integral de `ikedadev-platform`

Ejecuta todos los gates de calidad en orden desde la raíz del repositorio antes de dar por cerrada una tarea o abrir un PR:

// turbo-all

1. **Formato Rust (Workspace completo)**
```bash
cargo fmt --all -- --check
```

2. **Linting estricto Rust (cero warnings permitidos)**
```bash
cargo clippy --workspace --all-targets -- -D warnings
```

3. **Tests Rust (Workspace completo)**
```bash
cargo test --workspace
```

4. **Typecheck y Tests de Seguridad del BFF (`portal/bff`)**
```bash
cd portal/bff && npx tsc --noEmit && npm test
```

5. **Typecheck y Build de Producción del Frontend (`portal/web`)**
```bash
cd portal/web && npx tsc --noEmit && npm run build
```

> **Criterio de éxito:** Los 5 pasos deben finalizar con exit code `0` y sin warnings.
