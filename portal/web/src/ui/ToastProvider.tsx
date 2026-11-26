import {
  AlertTriangle,
  CheckCircle2,
  Info,
  X,
  XCircle,
} from 'lucide-react';
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { registerGlobalErrorNotifier, type BffApiError } from '../api/bffClient';

export type ToastVariant = 'success' | 'error' | 'warning' | 'info';

export interface ToastOptions {
  title?: string;
  duration?: number;
}

interface ToastItem {
  id: string;
  variant: ToastVariant;
  title?: string;
  message: string;
}

export interface ToastApi {
  success: (message: string, options?: ToastOptions) => void;
  error: (message: string, options?: ToastOptions) => void;
  warning: (message: string, options?: ToastOptions) => void;
  info: (message: string, options?: ToastOptions) => void;
}

export interface ToastContextValue {
  toast: ToastApi;
  dismiss: (id: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

const DEFAULT_DURATION_MS = 3500;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const timersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: string) => {
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
    setItems((prev) => prev.filter((item) => item.id !== id));
  }, []);

  const push = useCallback(
    (variant: ToastVariant, message: string, options?: ToastOptions) => {
      const id = `toast_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
      const duration = options?.duration ?? DEFAULT_DURATION_MS;

      setItems((prev) => [...prev.slice(-4), { id, variant, title: options?.title, message }]);

      if (duration > 0) {
        const timer = setTimeout(() => {
          dismiss(id);
        }, duration);
        timersRef.current.set(id, timer);
      }
    },
    [dismiss],
  );

  const toast = useMemo<ToastApi>(
    () => ({
      success: (message, options) => push('success', message, options),
      error: (message, options) => push('error', message, options),
      warning: (message, options) => push('warning', message, options),
      info: (message, options) => push('info', message, options),
    }),
    [push],
  );

  useEffect(() => {
    const unregister = registerGlobalErrorNotifier((err: BffApiError) => {
      if (err.status === 0) {
        toast.error(err.message, { title: 'Sin conexión con el servidor', duration: 5000 });
      } else if (err.status === 403) {
        toast.warning(err.message || 'No tienes permisos para realizar esta acción.', {
          title: 'Permisos insuficientes',
          duration: 4500,
        });
      } else if (err.status === 409) {
        toast.warning(err.message || 'Conflicto de estado o registro duplicado.', {
          title: 'Conflicto detectado',
          duration: 4500,
        });
      } else if (err.status === 422) {
        toast.error(err.message || 'Revisa los datos ingresados e intenta nuevamente.', {
          title: 'Datos inválidos',
          duration: 4500,
        });
      }
    });

    return () => {
      unregister();
    };
  }, [toast]);

  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      for (const timer of timers.values()) {
        clearTimeout(timer);
      }
      timers.clear();
    };
  }, []);

  const value = useMemo<ToastContextValue>(() => ({ toast, dismiss }), [toast, dismiss]);

  return (
    <ToastContext.Provider value={value}>
      {children}
      <div className="toast-stack" role="region" aria-label="Notificaciones" aria-live="polite">
        {items.map((item) => (
          <div key={item.id} className={`toast-item toast-${item.variant}`} role="status">
            <span className="toast-icon">
              {item.variant === 'success' && <CheckCircle2 size={18} />}
              {item.variant === 'error' && <XCircle size={18} />}
              {item.variant === 'warning' && <AlertTriangle size={18} />}
              {item.variant === 'info' && <Info size={18} />}
            </span>
            <div className="toast-content">
              {item.title && <div className="toast-title">{item.title}</div>}
              <div className="toast-desc">{item.message}</div>
            </div>
            <button
              type="button"
              className="toast-close"
              onClick={() => dismiss(item.id)}
              aria-label="Cerrar notificación"
            >
              <X size={15} />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error('useToast must be used within a ToastProvider');
  }
  return ctx;
}
