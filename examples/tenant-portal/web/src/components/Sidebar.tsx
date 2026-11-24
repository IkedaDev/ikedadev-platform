import {
  CalendarClock,
  LayoutDashboard,
  LogOut,
  Package,
  RefreshCw,
  ShieldCheck,
  Users,
} from 'lucide-react';
import type { AuthUser, BffHealth } from '../types/api';

export type NavSection = 'dashboard' | 'clients' | 'catalog' | 'bookings' | 'identity';

interface SidebarProps {
  active: NavSection;
  onSelect: (section: NavSection) => void;
  user: AuthUser;
  onLogout: () => void;
  health: BffHealth | null;
  healthLoading: boolean;
  onRefreshHealth: () => void;
}

const NAV_ITEMS: Array<{ id: NavSection; label: string; icon: typeof LayoutDashboard }> = [
  { id: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
  { id: 'clients', label: 'Clientes (CRM)', icon: Users },
  { id: 'catalog', label: 'Catálogo', icon: Package },
  { id: 'bookings', label: 'Agenda y Reservas', icon: CalendarClock },
  { id: 'identity', label: 'Identidad y Acceso', icon: ShieldCheck },
];

export function Sidebar({
  active,
  onSelect,
  user,
  onLogout,
  health,
  healthLoading,
  onRefreshHealth,
}: SidebarProps) {
  const statusClass = !health ? 'offline' : health.status === 'healthy' ? 'ok' : 'degraded';
  const statusLabel = !health
    ? 'BFF Desconectado'
    : health.status === 'healthy'
      ? 'BFF Conectado'
      : 'Parcialmente activo';

  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <div className="brand-mark">IK</div>
        <div>
          <div className="brand-title">IkedaDev Portal</div>
          <div className="brand-subtitle">OIDC + JWT (Keycloak)</div>
        </div>
      </div>

      <nav className="nav-list">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          return (
            <button
              key={item.id}
              type="button"
              className={`nav-item ${active === item.id ? 'active' : ''}`}
              onClick={() => onSelect(item.id)}
            >
              <Icon size={17} />
              <span>{item.label}</span>
            </button>
          );
        })}
      </nav>

      <div className="sidebar-footer" style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
        {/* Authenticated user card */}
        <div className="health-box">
          <div style={{ fontWeight: 600, color: '#f8fafc', fontSize: 13 }}>{user.name}</div>
          <div style={{ color: '#94a3b8', fontSize: 11, marginBottom: 6, wordBreak: 'break-all' }}>
            {user.email}
          </div>
          {user.tenant_id && (
            <div style={{ color: '#64748b', fontSize: 10, marginBottom: 8 }}>
              Tenant: <code>{user.tenant_id.slice(0, 8)}...</code>
            </div>
          )}
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, marginBottom: 10 }}>
            {user.roles.slice(0, 3).map((role) => (
              <span
                key={role}
                style={{
                  fontSize: 10,
                  padding: '2px 6px',
                  borderRadius: 999,
                  background: 'rgba(59, 130, 246, 0.2)',
                  color: '#93c5fd',
                }}
              >
                {role}
              </span>
            ))}
          </div>
          <button
            type="button"
            className="btn btn-secondary btn-sm"
            onClick={onLogout}
            style={{
              width: '100%',
              background: '#0f172a',
              color: '#e2e8f0',
              borderColor: '#334155',
            }}
          >
            <LogOut size={13} />
            Cerrar sesión
          </button>
        </div>

        {/* Health status box */}
        <div className="health-box">
          <div className="health-header">
            <span>
              <span className={`health-dot ${statusClass}`} />
              {statusLabel}
            </span>
            <button
              type="button"
              onClick={onRefreshHealth}
              disabled={healthLoading}
              title="Verificar estado"
              style={{
                background: 'transparent',
                border: 'none',
                color: '#94a3b8',
                cursor: 'pointer',
                padding: 2,
              }}
            >
              <RefreshCw size={13} />
            </button>
          </div>
          {health ? (
            <div className="upstream-grid">
              {Object.entries(health.upstreams).map(([name, info]) => (
                <div key={name} className="upstream-item">
                  <span className={`health-dot ${info?.ok ? 'ok' : 'offline'}`} />
                  <span>{name}</span>
                </div>
              ))}
            </div>
          ) : (
            <div style={{ color: '#94a3b8', fontSize: 11 }}>
              Verifica que el BFF corra en el puerto configurado.
            </div>
          )}
        </div>
      </div>
    </aside>
  );
}
