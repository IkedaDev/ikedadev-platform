import {
  Activity,
  CalendarClock,
  LayoutDashboard,
  Package,
  RefreshCw,
  Settings,
  ShieldCheck,
  Users,
  X,
} from 'lucide-react';
import { useEffect } from 'react';
import { useTheme } from '../context/ThemeProvider';
import type { BffHealth, TenantFeatures } from '../types/api';
import { BrandLogo, Button, useBodyScrollLock } from '../ui';

export type NavSection =
  | 'dashboard'
  | 'clients'
  | 'catalog'
  | 'bookings'
  | 'telemetry'
  | 'identity'
  | 'settings';

export interface NavItemConfig {
  id: NavSection;
  label: string;
  icon: typeof LayoutDashboard;
  featureKey?: keyof TenantFeatures;
  section: 'main' | 'admin';
}

export const NAV_ITEMS: NavItemConfig[] = [
  { id: 'dashboard', label: 'Inicio', icon: LayoutDashboard, section: 'main' },
  { id: 'clients', label: 'Clientes', icon: Users, featureKey: 'crm', section: 'main' },
  { id: 'catalog', label: 'Catálogo', icon: Package, featureKey: 'catalog', section: 'main' },
  {
    id: 'bookings',
    label: 'Agenda y Reservas',
    icon: CalendarClock,
    featureKey: 'bookings',
    section: 'main',
  },
  {
    id: 'telemetry',
    label: 'Actividad',
    icon: Activity,
    featureKey: 'telemetry',
    section: 'main',
  },
  { id: 'identity', label: 'Equipo y Accesos', icon: ShieldCheck, section: 'admin' },
  { id: 'settings', label: 'Ajustes', icon: Settings, section: 'admin' },
];

export interface SidebarProps {
  active: NavSection;
  onSelect: (section: NavSection) => void;
  mobileOpen: boolean;
  onCloseMobile: () => void;
  health: BffHealth | null;
  healthLoading: boolean;
  onRefreshHealth: () => void;
}

export function Sidebar({
  active,
  onSelect,
  mobileOpen,
  onCloseMobile,
  health,
  healthLoading,
  onRefreshHealth,
}: SidebarProps) {
  const { settings, tenantMeta } = useTheme();
  const { branding, features } = settings;

  useBodyScrollLock(mobileOpen);

  useEffect(() => {
    if (!mobileOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onCloseMobile();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [mobileOpen, onCloseMobile]);

  const displayName = branding.displayName?.trim() || 'Kropy';
  const visibleItems = NAV_ITEMS.filter(
    (item) => !item.featureKey || features[item.featureKey] !== false,
  );
  const mainItems = visibleItems.filter((item) => item.section === 'main');
  const adminItems = visibleItems.filter((item) => item.section === 'admin');

  const statusClass = !health ? 'offline' : health.status === 'healthy' ? 'ok' : 'degraded';
  const statusLabel = !health
    ? 'Sin conexión'
    : health.status === 'healthy'
      ? 'Sistemas 100% operativos'
      : 'Servicio con intermitencias';

  const handleSelect = (section: NavSection) => {
    onSelect(section);
    onCloseMobile();
  };

  return (
    <>
      <div
        className={`sidebar-backdrop ${mobileOpen ? 'is-open' : ''}`}
        onClick={onCloseMobile}
        aria-hidden="true"
      />
      <aside className={`sidebar ${mobileOpen ? 'is-open' : ''}`} aria-label="Navegación principal">
        <div className="sidebar-header">
          <div className="sidebar-brand">
            <BrandLogo logoUrl={branding.logoUrl} name={displayName} className="brand-mark" />
            <div className="brand-meta">
              <div className="brand-title">{displayName}</div>
              <div className="brand-subtitle">{tenantMeta?.name || 'Portal de gestión'}</div>
            </div>
          </div>
          <Button
            variant="ghost"
            size="sm"
            className="sidebar-close-btn"
            icon={<X size={18} />}
            onClick={onCloseMobile}
            aria-label="Cerrar menú"
          />
        </div>

        <nav className="sidebar-nav">
          <div className="sidebar-section-label">Operación</div>
          {mainItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.id}
                type="button"
                className={`nav-item ${active === item.id ? 'active' : ''}`}
                onClick={() => handleSelect(item.id)}
              >
                <Icon size={18} />
                <span>{item.label}</span>
              </button>
            );
          })}

          <div className="sidebar-section-label">Administración</div>
          {adminItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.id}
                type="button"
                className={`nav-item ${active === item.id ? 'active' : ''}`}
                onClick={() => handleSelect(item.id)}
              >
                <Icon size={18} />
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>

        <div className="sidebar-footer">
          <div className="status-pill-footer">
            <span className="health-status-label">
              <span className={`health-dot ${statusClass}`} />
              <span>{statusLabel}</span>
            </span>
            <Button
              variant="ghost"
              size="sm"
              icon={<RefreshCw size={13} className={healthLoading ? 'btn-spinner' : ''} />}
              onClick={onRefreshHealth}
              disabled={healthLoading}
              title="Actualizar estado del servicio"
              aria-label="Actualizar estado del servicio"
            />
          </div>
        </div>
      </aside>
    </>
  );
}
