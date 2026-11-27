# Reglas del Design System del Portal (`portal/web`)

## 1. Identidad Visual Kropy y Tokens
- Toda la paleta, tipografía, radios, sombras y escalas z-index viven en `portal/web/src/styles/tokens.css` (Light en `:root`, Dark en `html.dark`) y los estilos globales/utilidades en `portal/web/src/styles/ui.css`.
- **Cero estilos inline (`style={{...}}`):** usa o crea clases semánticas en `ui.css` (única excepción técnica: `zIndex` dinámico del stack de modales en `ModalProvider.tsx`).
- Evita clases indefinidas o duplicadas; reutiliza utilidades estándar (`.stack-*`, `.row`, `.row-sm`, `.row-xs`, `.row-between`, `.row-wrap`, `.row-end`, `.input-action-row`, `.form-grid-2`, `.visually-hidden`, `.min-w-0`).

## 2. Reutilización Obligatoria de Primitivas (`portal/web/src/ui/`)
- No dupliques tablas, modales, drawers, toasts ni campos de formulario. Usa siempre:
  - `DataTable` (búsqueda, ordenamiento, paginación, skeleton, empty state y tarjetas móviles).
  - `DrawerProvider` (`useDrawer`) con `hasUnsavedChanges` en formularios de edición.
  - `ModalProvider` (`useModal`, `confirm`) y `useMediaPicker` (`portal/web/src/components/shared/MediaPickerModal.tsx`).
  - `ToastProvider` (`useToast`) conectado al notificador global de `bffClient.ts`.
  - `Button` (con `isLoading` en acciones asíncronas), `Input`, `Select`, `Textarea`, `StatCard`, `StatusBadge`, `EmptyState`, `Skeleton`, `BrandLogo`.

## 3. Contraste, Temas y Responsive (`375px`)
- Toda vista y componente debe verse con contraste WCAG AA en **Light Mode y Dark Mode** (verificar texto dentro de `<button>` como `.media-card-selectable`, badges de error `--color-danger`, inputs deshabilitados y TipTap `.tiptap-shell.is-disabled`).
- **Responsive móvil (`375px`):**
  - Sin scroll horizontal roto: barras de filtros y acciones de fila usan `.row-wrap`, `.topbar-title` trunca con `ellipsis`, `.input-action-row` apila botón bajo input en `<= 640px` y `.modal-backdrop`/`.modal-body` reducen padding.
- **UX en español y anti doble-submit:**
  - Mensajes humanos claros en español (sin códigos técnicos crudos ni backticks literales).
  - Toda mutación bloquea envíos duplicados (`saving` / `busyRowId` + `isLoading`).
  - Al hacer logout o recibir `401`, `clearClientSessionState()` cierra drawers/modals, limpia toasts, resetea `ThemeProvider` y borra `localStorage`/`sessionStorage`.
