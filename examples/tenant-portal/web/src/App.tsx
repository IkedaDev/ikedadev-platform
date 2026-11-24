import { useCallback, useEffect, useState } from 'react';
import { bffClient } from './api/bffClient';
import { Sidebar, type NavSection } from './components/Sidebar';
import type { AuthUser, BffHealth } from './types/api';
import { BookingsView } from './views/BookingsView';
import { CatalogView } from './views/CatalogView';
import { ClientsView } from './views/ClientsView';
import { DashboardView } from './views/DashboardView';
import { IdentityView } from './views/IdentityView';
import { LoginView } from './views/LoginView';

export function App() {
  const [section, setSection] = useState<NavSection>('dashboard');
  const [user, setUser] = useState<AuthUser | null>(null);
  const [authChecked, setAuthChecked] = useState(false);
  const [authError, setAuthError] = useState<string | null>(null);
  const [health, setHealth] = useState<BffHealth | null>(null);
  const [healthLoading, setHealthLoading] = useState(false);

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
      <div
        style={{
          minHeight: '100vh',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: 'var(--text-muted)',
          fontSize: 14,
        }}
      >
        Verificando sesión OIDC con el BFF...
      </div>
    );
  }

  if (!user) {
    return <LoginView health={health} authError={authError} />;
  }

  return (
    <div className="app-shell">
      <Sidebar
        active={section}
        onSelect={setSection}
        user={user}
        onLogout={handleLogout}
        health={health}
        healthLoading={healthLoading}
        onRefreshHealth={checkHealth}
      />
      <main className="main-content">
        {section === 'dashboard' && <DashboardView />}
        {section === 'clients' && <ClientsView />}
        {section === 'catalog' && <CatalogView />}
        {section === 'bookings' && <BookingsView />}
        {section === 'identity' && <IdentityView />}
      </main>
    </div>
  );
}

export default App;
