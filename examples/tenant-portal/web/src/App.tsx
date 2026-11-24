import { useCallback, useEffect, useState } from 'react';
import { bffClient } from './api/bffClient';
import { Sidebar, type NavSection } from './components/Sidebar';
import type { BffHealth } from './types/api';
import { BookingsView } from './views/BookingsView';
import { CatalogView } from './views/CatalogView';
import { ClientsView } from './views/ClientsView';
import { DashboardView } from './views/DashboardView';
import { IdentityView } from './views/IdentityView';

export function App() {
  const [section, setSection] = useState<NavSection>('dashboard');
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

  useEffect(() => {
    void checkHealth();
    const interval = setInterval(() => {
      void checkHealth();
    }, 30000);
    return () => clearInterval(interval);
  }, [checkHealth]);

  return (
    <div className="app-shell">
      <Sidebar
        active={section}
        onSelect={setSection}
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
