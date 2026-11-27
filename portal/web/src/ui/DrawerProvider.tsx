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
import { useModal } from './ModalProvider';
import { useBodyScrollLock } from './scrollLock';

export type DrawerSize = 'sm' | 'md' | 'lg';

export interface OpenDrawerOptions {
  title: string;
  subtitle?: string;
  size?: DrawerSize;
  content: ReactNode;
  footer?: ReactNode;
  hasUnsavedChanges?: () => boolean;
}

export interface DrawerContextValue {
  openDrawer: (options: OpenDrawerOptions) => void;
  closeDrawer: (force?: boolean) => void;
  isOpen: boolean;
}

const DrawerContext = createContext<DrawerContextValue | null>(null);

export function DrawerProvider({ children }: { children: ReactNode }) {
  const [drawer, setDrawer] = useState<OpenDrawerOptions | null>(null);
  const { confirm, hasOpenModal } = useModal();
  const backdropMouseDownRef = useRef(false);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);

  useBodyScrollLock(drawer !== null);

  useEffect(() => {
    if (drawer) {
      if (!previousFocusRef.current && document.activeElement instanceof HTMLElement) {
        previousFocusRef.current = document.activeElement;
      }
      panelRef.current?.focus();
    } else if (previousFocusRef.current) {
      previousFocusRef.current.focus();
      previousFocusRef.current = null;
    }
  }, [drawer]);

  const requestClose = useCallback(
    async (force = false) => {
      if (!drawer) return;
      if (!force && drawer.hasUnsavedChanges?.()) {
        const confirmed = await confirm({
          title: 'Cambios sin guardar',
          message: 'Tienes cambios sin guardar en este formulario. ¿Deseas descartarlos y cerrar?',
          variant: 'danger',
          confirmLabel: 'Descartar cambios',
          cancelLabel: 'Seguir editando',
        });
        if (!confirmed) return;
      }
      setDrawer(null);
    },
    [drawer, confirm],
  );

  const openDrawer = useCallback((options: OpenDrawerOptions) => {
    setDrawer(options);
  }, []);

  const closeDrawer = useCallback(
    (force = false) => {
      void requestClose(force);
    },
    [requestClose],
  );

  useEffect(() => {
    if (!drawer) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !hasOpenModal) {
        void requestClose(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [drawer, hasOpenModal, requestClose]);

  const value = useMemo<DrawerContextValue>(
    () => ({ openDrawer, closeDrawer, isOpen: drawer !== null }),
    [openDrawer, closeDrawer, drawer],
  );

  return (
    <DrawerContext.Provider value={value}>
      {children}
      {drawer && (
        <div
          className="drawer-backdrop"
          onMouseDown={(e) => {
            backdropMouseDownRef.current = e.target === e.currentTarget;
          }}
          onClick={(e) => {
            const startedOnBackdrop = backdropMouseDownRef.current;
            backdropMouseDownRef.current = false;
            if (startedOnBackdrop && e.target === e.currentTarget) {
              void requestClose(false);
            }
          }}
        >
          <div
            ref={panelRef}
            tabIndex={-1}
            className={`drawer-panel drawer-${drawer.size ?? 'md'}`}
            role="dialog"
            aria-modal="true"
            aria-label={drawer.title}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="drawer-header">
              <div>
                <h2 className="drawer-title">{drawer.title}</h2>
                {drawer.subtitle && <p className="drawer-subtitle">{drawer.subtitle}</p>}
              </div>
              <Button
                variant="ghost"
                size="sm"
                icon={<X size={17} />}
                onClick={() => {
                  void requestClose(false);
                }}
                aria-label="Cerrar panel"
              />
            </div>
            <div className="drawer-body">{drawer.content}</div>
            {drawer.footer && <div className="drawer-footer">{drawer.footer}</div>}
          </div>
        </div>
      )}
    </DrawerContext.Provider>
  );
}

export function useDrawer(): DrawerContextValue {
  const ctx = useContext(DrawerContext);
  if (!ctx) {
    throw new Error('useDrawer must be used within a DrawerProvider');
  }
  return ctx;
}
