import { Building2, ChevronDown, LogOut, Menu, Moon, Settings, Sun } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { usePermissions } from '../auth/permissions';
import { useTheme } from '../context/ThemeProvider';
import type { AuthUser } from '../types/api';
import { Button, StatusBadge } from '../ui';
import { NAV_ITEMS, type NavSection } from './Sidebar';

export interface TopbarProps {
  active: NavSection;
  user: AuthUser;
  onSelect: (section: NavSection) => void;
  onLogout: () => void;
  onOpenMobileMenu: () => void;
}

function getUserInitials(name: string, email: string): string {
  const source = name.trim() || email.trim();
  const parts = source.split(/\s+/).filter(Boolean);
  if (parts.length >= 2) {
    return `${parts[0][0]}${parts[1][0]}`.toUpperCase();
  }
  return source.slice(0, 2).toUpperCase();
}

export function Topbar({
  active,
  user,
  onSelect,
  onLogout,
  onOpenMobileMenu,
}: TopbarProps) {
  const { settings, tenantMeta, resolvedTheme, toggleTheme } = useTheme();
  const { isAdmin, can } = usePermissions(user);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  const activeItem = NAV_ITEMS.find((item) => item.id === active);
  const tenantLabel = tenantMeta?.name || settings.branding.displayName || 'Kropy';
  const roleLabel = isAdmin ? 'Administrador' : 'Colaborador';
  const canOpenSettings = can('settings:read');

  useEffect(() => {
    if (!menuOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setMenuOpen(false);
    };
    document.addEventListener('mousedown', handleClickOutside);
    window.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      window.removeEventListener('keydown', handleEscape);
    };
  }, [menuOpen]);

  return (
    <header className="topbar">
      <div className="topbar-left">
        <Button
          variant="ghost"
          size="sm"
          className="topbar-menu-btn"
          icon={<Menu size={20} />}
          onClick={onOpenMobileMenu}
          aria-label="Abrir navegación"
        />
        <div className="topbar-title-group">
          <span className="topbar-title">{activeItem?.label ?? 'Inicio'}</span>
        </div>
      </div>

      <div className="topbar-right">
        <span className="tenant-pill" title={tenantLabel}>
          <Building2 size={14} />
          <span className="truncate">{tenantLabel}</span>
        </span>

        <Button
          variant="ghost"
          size="sm"
          icon={resolvedTheme === 'dark' ? <Sun size={17} /> : <Moon size={17} />}
          onClick={toggleTheme}
          title={resolvedTheme === 'dark' ? 'Cambiar a modo claro' : 'Cambiar a modo oscuro'}
          aria-label={resolvedTheme === 'dark' ? 'Cambiar a modo claro' : 'Cambiar a modo oscuro'}
        />

        <div className="user-menu-wrap" ref={menuRef}>
          <button
            type="button"
            className="user-menu-trigger"
            onClick={() => setMenuOpen((prev) => !prev)}
            aria-expanded={menuOpen}
            aria-haspopup="menu"
          >
            <span className="user-avatar">{getUserInitials(user.name, user.email)}</span>
            <span className="user-trigger-name">{user.name}</span>
            <ChevronDown size={14} className="text-muted" />
          </button>

          {menuOpen && (
            <div className="user-menu-dropdown" role="menu">
              <div className="user-menu-header">
                <span className="user-menu-name">{user.name}</span>
                <span className="user-menu-email">{user.email}</span>
              </div>

              <div className="chip-list">
                <StatusBadge label={roleLabel} tone="emerald" />
              </div>

              <div className="stack-xs">
                {canOpenSettings && (
                  <Button
                    variant="ghost"
                    size="sm"
                    block
                    icon={<Settings size={14} />}
                    onClick={() => {
                      setMenuOpen(false);
                      onSelect('settings');
                    }}
                  >
                    Ajustes
                  </Button>
                )}
                <Button
                  variant="secondary"
                  size="sm"
                  block
                  icon={<LogOut size={14} />}
                  onClick={() => {
                    setMenuOpen(false);
                    onLogout();
                  }}
                >
                  Cerrar sesión
                </Button>
              </div>
            </div>
          )}
        </div>
      </div>
    </header>
  );
}
