import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { bffClient } from '../api/bffClient';
import type {
  TenantBranding,
  TenantSettings,
  TenantSettingsUpdate,
  ThemeMode,
} from '../types/api';

export const KROPY_DEFAULT_BRANDING: TenantBranding = {
  displayName: 'Kropy',
  logoUrl: '',
  primaryColor: '#059669',
  accentColor: '#0f172a',
  themeMode: 'light',
};

export const KROPY_DEFAULT_SETTINGS: TenantSettings = {
  branding: { ...KROPY_DEFAULT_BRANDING },
  locale: {
    currency: 'CLP',
    timezone: 'America/Santiago',
    language: 'es',
  },
  features: {
    crm: true,
    catalog: true,
    events: true,
    bookings: true,
    telemetry: true,
  },
};

export interface TenantMeta {
  tenantId: string;
  slug: string;
  name: string;
}

export interface ThemeContextValue {
  settings: TenantSettings;
  tenantMeta: TenantMeta | null;
  canEditSettings: boolean;
  resolvedTheme: 'light' | 'dark';
  isLoading: boolean;
  toggleTheme: () => void;
  updateSettings: (patch: TenantSettingsUpdate) => Promise<TenantSettings>;
  resetToKropy: () => void;
  applyPreview: (partialBranding: Partial<TenantBranding> | null) => void;
  reloadSettings: () => Promise<void>;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const LOCAL_THEME_KEY = 'kropy_theme_override';
const HEX_REGEX = /^#[0-9a-fA-F]{6}$/;

function isValidHex(value: string | undefined): value is string {
  return typeof value === 'string' && HEX_REGEX.test(value);
}

function hexToRgb(hex: string): { r: number; g: number; b: number } {
  const clean = hex.replace('#', '');
  return {
    r: parseInt(clean.slice(0, 2), 16),
    g: parseInt(clean.slice(2, 4), 16),
    b: parseInt(clean.slice(4, 6), 16),
  };
}

function darkenHex(hex: string, factor = 0.85): string {
  const { r, g, b } = hexToRgb(hex);
  const clamp = (n: number) => Math.max(0, Math.min(255, Math.round(n * factor)));
  const toHex = (n: number) => n.toString(16).padStart(2, '0');
  return `#${toHex(clamp(r))}${toHex(clamp(g))}${toHex(clamp(b))}`;
}

function getRelativeLuminance(hex: string): number {
  const { r, g, b } = hexToRgb(hex);
  const toLinear = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * toLinear(r) + 0.7152 * toLinear(g) + 0.0722 * toLinear(b);
}

function getContrastColor(hex: string): '#ffffff' | '#020617' {
  return getRelativeLuminance(hex) > 0.42 ? '#020617' : '#ffffff';
}

function resolveSlugFromLocation(): string | null {
  if (typeof window === 'undefined') return null;

  const params = new URLSearchParams(window.location.search);
  const queryTenant = params.get('tenant')?.trim().toLowerCase();
  if (queryTenant) return queryTenant;

  const host = window.location.hostname.toLowerCase();
  if (
    host === 'localhost' ||
    host === '127.0.0.1' ||
    /^\d{1,3}(\.\d{1,3}){3}$/.test(host)
  ) {
    return null;
  }

  const parts = host.split('.');
  if (parts.length >= 3) {
    const candidate = parts[0];
    if (candidate && !['www', 'app', 'portal', 'platform', 'platform-qa'].includes(candidate)) {
      return candidate;
    }
  }

  return null;
}

function getSystemDark(): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function mergeSettings(raw?: Partial<TenantSettings>): TenantSettings {
  return {
    branding: {
      ...KROPY_DEFAULT_SETTINGS.branding,
      ...(raw?.branding ?? {}),
    },
    locale: {
      ...KROPY_DEFAULT_SETTINGS.locale,
      ...(raw?.locale ?? {}),
    },
    features: {
      ...KROPY_DEFAULT_SETTINGS.features,
      ...(raw?.features ?? {}),
    },
  };
}

function applyCssVariables(branding: TenantBranding, resolvedTheme: 'light' | 'dark'): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;

  if (resolvedTheme === 'dark') {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }

  const primary = isValidHex(branding.primaryColor)
    ? branding.primaryColor
    : KROPY_DEFAULT_BRANDING.primaryColor;
  const accent = isValidHex(branding.accentColor)
    ? branding.accentColor
    : KROPY_DEFAULT_BRANDING.accentColor;

  const isDefaultPrimary = primary.toLowerCase() === KROPY_DEFAULT_BRANDING.primaryColor;
  const isDefaultAccent = accent.toLowerCase() === KROPY_DEFAULT_BRANDING.accentColor;

  if (isDefaultPrimary) {
    root.style.removeProperty('--color-primary');
    root.style.removeProperty('--color-primary-hover');
    root.style.removeProperty('--color-primary-soft');
    root.style.removeProperty('--color-primary-contrast');
  } else {
    const { r, g, b } = hexToRgb(primary);
    root.style.setProperty('--color-primary', primary);
    root.style.setProperty('--color-primary-hover', darkenHex(primary, 0.85));
    root.style.setProperty(
      '--color-primary-soft',
      resolvedTheme === 'dark'
        ? `rgba(${r}, ${g}, ${b}, 0.16)`
        : `rgba(${r}, ${g}, ${b}, 0.10)`,
    );
    root.style.setProperty('--color-primary-contrast', getContrastColor(primary));
  }

  if (isDefaultAccent) {
    root.style.removeProperty('--color-accent');
  } else {
    root.style.setProperty('--color-accent', accent);
  }

  const titleName = branding.displayName?.trim() || 'Kropy';
  document.title = `${titleName} — SaaS Portal`;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<TenantSettings>(() => mergeSettings());
  const [previewBranding, setPreviewBranding] = useState<Partial<TenantBranding> | null>(null);
  const [tenantMeta, setTenantMeta] = useState<TenantMeta | null>(null);
  const [canEditSettings, setCanEditSettings] = useState<boolean>(false);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [localThemeOverride, setLocalThemeOverride] = useState<'light' | 'dark' | null>(() => {
    try {
      const saved = window.localStorage.getItem(LOCAL_THEME_KEY);
      return saved === 'light' || saved === 'dark' ? saved : null;
    } catch {
      return null;
    }
  });
  const [systemDark, setSystemDark] = useState<boolean>(() => getSystemDark());

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, []);

  const reloadSettings = useCallback(async () => {
    setIsLoading(true);
    try {
      const res = await bffClient.getSettings({ silent: true });
      const merged = mergeSettings(res.settings);
      setSettings(merged);
      setTenantMeta({
        tenantId: res.tenantId,
        slug: res.slug,
        name: res.name,
      });
      setCanEditSettings(res.canEdit ?? true);
    } catch {
      // Not authenticated or settings unavailable: try public branding by ?tenant= / subdomain
      setTenantMeta(null);
      setCanEditSettings(false);
      const slug = resolveSlugFromLocation();
      if (slug) {
        try {
          const pub = await bffClient.getPublicBranding(slug);
          setSettings((prev) =>
            mergeSettings({
              ...prev,
              branding: { ...KROPY_DEFAULT_BRANDING, ...pub.branding },
            }),
          );
        } catch {
          setSettings(mergeSettings());
        }
      }
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void reloadSettings();
  }, [reloadSettings]);

  const effectiveBranding = useMemo<TenantBranding>(
    () => ({
      ...settings.branding,
      ...(previewBranding ?? {}),
    }),
    [settings.branding, previewBranding],
  );

  const resolvedTheme = useMemo<'light' | 'dark'>(() => {
    if (previewBranding?.themeMode) {
      const mode: ThemeMode = previewBranding.themeMode;
      if (mode === 'system') return systemDark ? 'dark' : 'light';
      return mode;
    }
    if (localThemeOverride) return localThemeOverride;
    const mode: ThemeMode = settings.branding.themeMode ?? 'light';
    if (mode === 'system') return systemDark ? 'dark' : 'light';
    return mode;
  }, [previewBranding?.themeMode, localThemeOverride, settings.branding.themeMode, systemDark]);

  useEffect(() => {
    applyCssVariables(effectiveBranding, resolvedTheme);
  }, [effectiveBranding, resolvedTheme]);

  const toggleTheme = useCallback(() => {
    const next: 'light' | 'dark' = resolvedTheme === 'dark' ? 'light' : 'dark';
    setLocalThemeOverride(next);
    try {
      window.localStorage.setItem(LOCAL_THEME_KEY, next);
    } catch {
      // ignore storage errors
    }
  }, [resolvedTheme]);

  const applyPreview = useCallback((partialBranding: Partial<TenantBranding> | null) => {
    setPreviewBranding(partialBranding);
  }, []);

  const resetToKropy = useCallback(() => {
    setPreviewBranding({ ...KROPY_DEFAULT_BRANDING });
    setLocalThemeOverride(null);
    try {
      window.localStorage.removeItem(LOCAL_THEME_KEY);
    } catch {
      // ignore
    }
  }, []);

  const updateSettings = useCallback(
    async (patch: TenantSettingsUpdate): Promise<TenantSettings> => {
      const res = await bffClient.updateSettings(patch);
      const merged = mergeSettings(res.settings);
      setSettings(merged);
      setPreviewBranding(null);
      if (patch.branding?.themeMode) {
        setLocalThemeOverride(null);
        try {
          window.localStorage.removeItem(LOCAL_THEME_KEY);
        } catch {
          // ignore
        }
      }
      setTenantMeta({
        tenantId: res.tenantId,
        slug: res.slug,
        name: res.name,
      });
      setCanEditSettings(res.canEdit ?? true);
      return merged;
    },
    [],
  );

  const value = useMemo<ThemeContextValue>(
    () => ({
      settings: {
        ...settings,
        branding: effectiveBranding,
      },
      tenantMeta,
      canEditSettings,
      resolvedTheme,
      isLoading,
      toggleTheme,
      updateSettings,
      resetToKropy,
      applyPreview,
      reloadSettings,
    }),
    [
      settings,
      effectiveBranding,
      tenantMeta,
      canEditSettings,
      resolvedTheme,
      isLoading,
      toggleTheme,
      updateSettings,
      resetToKropy,
      applyPreview,
      reloadSettings,
    ],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return ctx;
}
