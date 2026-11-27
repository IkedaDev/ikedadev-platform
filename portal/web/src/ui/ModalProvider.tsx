import { X } from 'lucide-react';
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
  clearModals: () => void;
  confirm: (options: ConfirmOptions) => Promise<boolean>;
  hasOpenModal: boolean;
}

const ModalContext = createContext<ModalContextValue | null>(null);

export function ModalProvider({ children }: { children: ReactNode }) {
  const [stack, setStack] = useState<ModalEntry[]>([]);
  const stackRef = useRef<ModalEntry[]>([]);
  const backdropMouseDownIdRef = useRef<string | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const topPanelRef = useRef<HTMLDivElement | null>(null);

  stackRef.current = stack;

  const closeModal = useCallback((id?: string) => {
    const current = stackRef.current;
    if (current.length === 0) return;

    const target = id ? current.find((m) => m.id === id) : current[current.length - 1];
    if (!target) return;

    const next = id ? current.filter((m) => m.id !== id) : current.slice(0, -1);
    stackRef.current = next;
    setStack(next);
    target.onDismiss?.();
  }, []);

  const clearModals = useCallback(() => {
    const current = stackRef.current;
    if (current.length === 0) return;
    stackRef.current = [];
    setStack([]);
    for (const entry of current) {
      entry.onDismiss?.();
    }
  }, []);

  const openModal = useCallback((options: OpenModalOptions): string => {
    const id = `modal_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    setStack((prev) => {
      const next = [...prev, { ...options, id }];
      stackRef.current = next;
      return next;
    });
    return id;
  }, []);

  const confirm = useCallback((options: ConfirmOptions): Promise<boolean> => {
    return new Promise<boolean>((resolve) => {
      const id = `confirm_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
      let settled = false;

      const finish = (result: boolean) => {
        if (settled) return;
        settled = true;
        setStack((prev) => {
          const next = prev.filter((m) => m.id !== id);
          stackRef.current = next;
          return next;
        });
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

      setStack((prev) => {
        const next = [...prev, entry];
        stackRef.current = next;
        return next;
      });
    });
  }, []);

  useBodyScrollLock(stack.length > 0);

  useEffect(() => {
    if (stack.length > 0) {
      if (!previousFocusRef.current && document.activeElement instanceof HTMLElement) {
        previousFocusRef.current = document.activeElement;
      }
      topPanelRef.current?.focus();
    } else if (previousFocusRef.current) {
      previousFocusRef.current.focus();
      previousFocusRef.current = null;
    }
  }, [stack.length]);

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
    () => ({ openModal, closeModal, clearModals, confirm, hasOpenModal: stack.length > 0 }),
    [openModal, closeModal, clearModals, confirm, stack.length],
  );

  return (
    <ModalContext.Provider value={value}>
      {children}
      {stack.map((modal, index) => (
        <div
          key={modal.id}
          className="modal-backdrop"
          style={{ zIndex: 200 + index * 10 }}
          onMouseDown={(e) => {
            backdropMouseDownIdRef.current = e.target === e.currentTarget ? modal.id : null;
          }}
          onClick={(e) => {
            const startedOnBackdrop = backdropMouseDownIdRef.current === modal.id;
            backdropMouseDownIdRef.current = null;
            if (startedOnBackdrop && e.target === e.currentTarget && modal.closeOnBackdrop !== false) {
              closeModal(modal.id);
            }
          }}
        >
          <div
            ref={index === stack.length - 1 ? topPanelRef : undefined}
            tabIndex={-1}
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
