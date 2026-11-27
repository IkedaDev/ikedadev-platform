import { useState, type ReactNode } from 'react';
import type { AuthUser, BffHealth } from '../types/api';
import { Sidebar, type NavSection } from './Sidebar';
import { Topbar } from './Topbar';

export interface AppLayoutProps {
  active: NavSection;
  onSelect: (section: NavSection) => void;
  user: AuthUser;
  onLogout: () => void;
  health: BffHealth | null;
  healthLoading: boolean;
  onRefreshHealth: () => void;
  children: ReactNode;
}

export function AppLayout({
  active,
  onSelect,
  user,
  onLogout,
  health,
  healthLoading,
  onRefreshHealth,
  children,
}: AppLayoutProps) {
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  return (
    <div className="app-shell">
      <Sidebar
        active={active}
        user={user}
        onSelect={onSelect}
        mobileOpen={mobileMenuOpen}
        onCloseMobile={() => setMobileMenuOpen(false)}
        health={health}
        healthLoading={healthLoading}
        onRefreshHealth={onRefreshHealth}
      />
      <div className="app-main">
        <Topbar
          active={active}
          user={user}
          onSelect={onSelect}
          onLogout={onLogout}
          onOpenMobileMenu={() => setMobileMenuOpen(true)}
        />
        <main className="page-container">{children}</main>
      </div>
    </div>
  );
}
