import { X } from 'lucide-react';
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { Button } from './Button';
import { useBodyScrollLock } from './scrollLock';

export type ModalSize = 'sm' | 'md' | 'lg';

export interface OpenModalOptions {
  title: string;
  subtitle?: string;
  size?: ModalSize;
  content: ReactNode;
  footer?: ReactNode;
  closeOnBackdrop?: boolean;
}

export interface ConfirmOptions {
  title: string;
  message: string;
  variant?: 'danger' | 'primary';
  confirmLabel?: string;
  cancelLabel?: string;
}

interface ModalEntry extends OpenModalOptions {
  id: string;
  onDismiss?: () => void;
}

export interface ModalContextValue {
  openModal: (options: OpenModalOptions) => string;
  closeModal: (id?: string) => void;
  confirm: (options: ConfirmOptions) => Promise<boolean>;
  hasOpenModal: boolean;
}

const ModalContext = createContext<ModalContextValue | null>(null);

export function ModalProvider({ children }: { children: ReactNode }) {
  const [stack, setStack] = useState<ModalEntry[]>([]);

  const closeModal = useCallback((id?: string) => {
    setStack((prev) => {
      if (prev.length === 0) return prev;
      if (id) {
        const target = prev.find((m) => m.id === id);
        target?.onDismiss?.();
        return prev.filter((m) => m.id !== id);
      }
      const last = prev[prev.length - 1];
      last?.onDismiss?.();
      return prev.slice(0, -1);
    });
  }, []);

  const openModal = useCallback((options: OpenModalOptions): string => {
    const id = `modal_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    setStack((prev) => [...prev, { ...options, id }]);
    return id;
  }, []);

  const confirm = useCallback((options: ConfirmOptions): Promise<boolean> => {
    return new Promise<boolean>((resolve) => {
      const id = `confirm_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
      let settled = false;

      const finish = (result: boolean) => {
        if (settled) return;
        settled = true;
        setStack((prev) => prev.filter((m) => m.id !== id));
        resolve(result);
      };

      const entry: ModalEntry = {
        id,
        title: options.title,
        size: 'sm',
        closeOnBackdrop: true,
        onDismiss: () => {
          if (!settled) {
            settled = true;
            resolve(false);
          }
        },
        content: <p className="confirm-message">{options.message}</p>,
        footer: (
          <>
            <Button variant="secondary" onClick={() => finish(false)}>
              {options.cancelLabel ?? 'Cancelar'}
            </Button>
            <Button
              variant={options.variant === 'danger' ? 'danger' : 'primary'}
              onClick={() => finish(true)}
            >
              {options.confirmLabel ?? 'Confirmar'}
            </Button>
          </>
        ),
      };

      setStack((prev) => [...prev, entry]);
    });
  }, []);

  useBodyScrollLock(stack.length > 0);

  useEffect(() => {
    if (stack.length === 0) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        closeModal();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [stack.length, closeModal]);

  const value = useMemo<ModalContextValue>(
    () => ({ openModal, closeModal, confirm, hasOpenModal: stack.length > 0 }),
    [openModal, closeModal, confirm, stack.length],
  );

  return (
    <ModalContext.Provider value={value}>
      {children}
      {stack.map((modal, index) => (
        <div
          key={modal.id}
          className="modal-backdrop"
          style={{ zIndex: 200 + index * 10 }}
          onClick={() => {
            if (modal.closeOnBackdrop !== false) {
              closeModal(modal.id);
            }
          }}
        >
          <div
            className={`modal-panel modal-${modal.size ?? 'md'}`}
            role="dialog"
            aria-modal="true"
            aria-label={modal.title}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-header">
              <div>
                <h2 className="modal-title">{modal.title}</h2>
                {modal.subtitle && <p className="modal-subtitle">{modal.subtitle}</p>}
              </div>
              <Button
                variant="ghost"
                size="sm"
                icon={<X size={16} />}
                onClick={() => closeModal(modal.id)}
                aria-label="Cerrar modal"
              />
            </div>
            <div className="modal-body">{modal.content}</div>
            {modal.footer && <div className="modal-footer">{modal.footer}</div>}
          </div>
        </div>
      ))}
    </ModalContext.Provider>
  );
}

export function useModal(): ModalContextValue {
  const ctx = useContext(ModalContext);
  if (!ctx) {
    throw new Error('useModal must be used within a ModalProvider');
  }
  return ctx;
}
