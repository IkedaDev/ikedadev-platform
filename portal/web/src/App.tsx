import { useCallback, useEffect, useRef, useState } from 'react';
import { bffClient } from './api/bffClient';
import { tracker } from './api/tracker';
import { AppLayout } from './components/AppLayout';
import { NAV_ITEMS, type NavSection } from './components/Sidebar';
import { useTheme } from './context/ThemeProvider';
import type { AuthUser, BffHealth } from './types/api';
import { Skeleton, useModal } from './ui';
import { BookingsView } from './views/BookingsView';
import { CatalogView } from './views/CatalogView';
import { ClientsView } from './views/ClientsView';
import { DashboardView } from './views/DashboardView';
import { IdentityView } from './views/IdentityView';
import { LoginView } from './views/LoginView';
import { SettingsView } from './views/SettingsView';
import { TelemetryView } from './views/TelemetryView';

export function App() {
  const { settings, reloadSettings } = useTheme();
  const { confirm } = useModal();
  const [section, setSection] = useState<NavSection>('dashboard');
  const [user, setUser] = useState<AuthUser | null>(null);
  const [authChecked, setAuthChecked] = useState(false);
  const [authError, setAuthError] = useState<string | null>(null);
  const [health, setHealth] = useState<BffHealth | null>(null);
  const [healthLoading, setHealthLoading] = useState(false);

  const settingsDirtyRef = useRef(false);

  const checkHealth = useCallback(async () => {
    setHealthLoading(true);
    try {
      const h = await bffClient.health();
      setHealth(h);
    } catch {
      setHealth(null);
    } finally {
      setHealthLoading(false);
    }
  }, []);

  const checkSession = useCallback(async () => {
    try {
      const res = await bffClient.getMe();
      if (res.authenticated && res.user) {
        setUser(res.user);
      } else {
        setUser(null);
      }
    } catch {
      setUser(null);
    } finally {
      setAuthChecked(true);
    }
  }, []);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const errParam = params.get('auth_error');
    if (errParam) {
      setAuthError(errParam);
      window.history.replaceState({}, '', window.location.pathname);
    }

    void checkSession();
    void checkHealth();
    const interval = setInterval(() => {
      void checkHealth();
    }, 30000);
    return () => clearInterval(interval);
  }, [checkSession, checkHealth]);

  useEffect(() => {
    if (user) {
      void reloadSettings();
    }
  }, [user, reloadSettings]);

  // Dynamically guard disabled feature routes -> fall back to dashboard
  useEffect(() => {
    const navItem = NAV_ITEMS.find((item) => item.id === section);
    if (navItem?.featureKey && settings.features[navItem.featureKey] === false) {
      setSection('dashboard');
    }
  }, [section, settings.features]);

  // Preserve telemetry page view tracking on navigation
  useEffect(() => {
    if (!user) return;
    tracker.trackPageView(section, {
      user_id: user.id,
      tenant_id: user.tenant_id,
    });
  }, [section, user]);

  const handleSelectSection = useCallback(
    async (next: NavSection) => {
      if (next === section) return;
      if (section === 'settings' && settingsDirtyRef.current) {
        const ok = await confirm({
          title: 'Cambios sin guardar',
          message:
            'Tienes cambios sin guardar en los ajustes del negocio. ¿Deseas salir y descartarlos?',
          variant: 'danger',
          confirmLabel: 'Descartar cambios',
          cancelLabel: 'Seguir editando',
        });
        if (!ok) return;
        settingsDirtyRef.current = false;
      }
      setSection(next);
    },
    [section, confirm],
  );

  const handleSettingsDirtyChange = useCallback((dirty: boolean) => {
    settingsDirtyRef.current = dirty;
  }, []);

  const handleLogout = async () => {
    try {
      const res = await bffClient.logout();
      setUser(null);
      if (res.logoutUrl) {
        window.location.href = res.logoutUrl;
        return;
      }
    } catch {
      setUser(null);
    }
  };

  if (!authChecked) {
    return (
      <div className="login-shell">
        <div className="login-card">
          <div className="stack-md">
            <Skeleton variant="line-lg" />
            <Skeleton variant="line" />
            <Skeleton variant="line-sm" />
          </div>
          <p className="text-sm text-muted">Cargando tu espacio de trabajo...</p>
        </div>
      </div>
    );
  }

  if (!user) {
    return <LoginView health={health} authError={authError} />;
  }

  return (
    <AppLayout
      active={section}
      onSelect={(next) => {
        void handleSelectSection(next);
      }}
      user={user}
      onLogout={handleLogout}
      health={health}
      healthLoading={healthLoading}
      onRefreshHealth={checkHealth}
    >
      {section === 'dashboard' && (
        <DashboardView
          onNavigate={(next) => {
            void handleSelectSection(next);
          }}
        />
      )}
      {section === 'clients' && settings.features.crm !== false && <ClientsView />}
      {section === 'catalog' && settings.features.catalog !== false && <CatalogView />}
      {section === 'bookings' && settings.features.bookings !== false && <BookingsView />}
      {section === 'telemetry' && settings.features.telemetry !== false && <TelemetryView />}
      {section === 'identity' && <IdentityView />}
      {section === 'settings' && <SettingsView onDirtyChange={handleSettingsDirtyChange} />}
    </AppLayout>
  );
}

export default App;
