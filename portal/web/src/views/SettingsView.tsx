import {
  Check,
  CheckCircle2,
  Globe,
  Image as ImageIcon,
  Info,
  Layers,
  Link2,
  Palette,
  RotateCcw,
  Save,
  Sparkles,
} from 'lucide-react';
import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { BffApiError } from '../api/bffClient';
import { KROPY_DEFAULT_BRANDING, useTheme } from '../context/ThemeProvider';
import type {
  AuthUser,
  TenantBranding,
  TenantFeatures,
  TenantLocale,
  ThemeMode,
} from '../types/api';
import { BrandLogo, Button, Input, Select, StatusBadge, formatMoney, useToast } from '../ui';
import { useMediaPicker } from '../components/shared/MediaPickerModal';
import { sanitizeLogoUrl } from '../utils/url';

interface SettingsViewProps {
  user: AuthUser;
  onDirtyChange?: (dirty: boolean) => void;
}

const CURATED_PALETTES: Array<{
  name: string;
  primaryColor: string;
  accentColor: string;
}> = [
  { name: 'Kropy Esmeralda', primaryColor: '#059669', accentColor: '#0f172a' },
  { name: 'Azul Corporativo', primaryColor: '#2563eb', accentColor: '#0f172a' },
  { name: 'Índigo', primaryColor: '#4f46e5', accentColor: '#1e1b4b' },
  { name: 'Terracota', primaryColor: '#ea580c', accentColor: '#1c1917' },
  { name: 'Violeta', primaryColor: '#7c3aed', accentColor: '#1e1b4b' },
  { name: 'Grafito', primaryColor: '#334155', accentColor: '#0f172a' },
];

const CURRENCY_OPTIONS = [
  { value: 'CLP', label: 'CLP — Peso Chileno' },
  { value: 'USD', label: 'USD — Dólar Estadounidense' },
  { value: 'EUR', label: 'EUR — Euro' },
  { value: 'MXN', label: 'MXN — Peso Mexicano' },
  { value: 'ARS', label: 'ARS — Peso Argentino' },
  { value: 'COP', label: 'COP — Peso Colombiano' },
  { value: 'PEN', label: 'PEN — Sol Peruano' },
];

const DEFAULT_TIMEZONES = [
  'America/Santiago',
  'America/Mexico_City',
  'America/Bogota',
  'America/Argentina/Buenos_Aires',
  'America/Lima',
  'America/New_York',
  'Europe/Madrid',
  'UTC',
];

const FEATURE_MODULES: Array<{
  key: keyof TenantFeatures;
  label: string;
  description: string;
}> = [
  {
    key: 'crm',
    label: 'Clientes',
    description: 'Directorio de clientes, fichas y campos personalizados.',
  },
  {
    key: 'catalog',
    label: 'Catálogo',
    description: 'Categorías, servicios agendables, productos y paquetes.',
  },
  {
    key: 'events',
    label: 'Calendarios',
    description: 'Salas, profesionales y disponibilidad de horarios.',
  },
  {
    key: 'bookings',
    label: 'Agenda y Reservas',
    description: 'Gestión de citas y control de horarios disponibles.',
  },
  {
    key: 'telemetry',
    label: 'Actividad',
    description: 'Historial de acciones y cambios del equipo.',
  },
  {
    key: 'content',
    label: 'Contenido / Blog',
    description: 'Editor visual de artículos, categorías, biblioteca de medios y Deploy Hooks SSG.',
  },
];

const HEX_REGEX = /^#[0-9a-fA-F]{6}$/;

export function SettingsView({ user, onDirtyChange }: SettingsViewProps) {
  const {
    settings,
    tenantMeta,
    canEditSettings,
    isLoading,
    updateSettings,
    applyPreview,
    resetToKropy,
  } = useTheme();
  const { toast } = useToast();
  const { openMediaPicker } = useMediaPicker();

  const [branding, setBranding] = useState<TenantBranding>({ ...settings.branding });
  const [locale, setLocale] = useState<TenantLocale>({ ...settings.locale });
  const [features, setFeatures] = useState<TenantFeatures>({ ...settings.features });
  const [saving, setSaving] = useState(false);
  const [copiedLink, setCopiedLink] = useState(false);

  const syncedOnceRef = useRef(false);

  useEffect(() => {
    if (!isLoading && !syncedOnceRef.current) {
      syncedOnceRef.current = true;
      setBranding({ ...settings.branding });
      setLocale({ ...settings.locale });
      setFeatures({ ...settings.features });
      onDirtyChange?.(false);
    }
  }, [isLoading, settings, onDirtyChange]);

  useEffect(() => {
    return () => {
      applyPreview(null);
      onDirtyChange?.(false);
    };
  }, [applyPreview, onDirtyChange]);

  const timezoneOptions = useMemo(() => {
    if (locale.timezone && !DEFAULT_TIMEZONES.includes(locale.timezone)) {
      return [locale.timezone, ...DEFAULT_TIMEZONES];
    }
    return DEFAULT_TIMEZONES;
  }, [locale.timezone]);

  const updateBrandingField = <K extends keyof TenantBranding>(
    key: K,
    value: TenantBranding[K],
  ) => {
    const next = { ...branding, [key]: value };
    setBranding(next);
    applyPreview(next);
    onDirtyChange?.(true);
  };

  const applyPalette = (primaryColor: string, accentColor: string) => {
    if (!canEditSettings) return;
    const next = { ...branding, primaryColor, accentColor };
    setBranding(next);
    applyPreview(next);
    onDirtyChange?.(true);
  };

  const handleResetKropy = () => {
    const defaults = { ...KROPY_DEFAULT_BRANDING };
    setBranding(defaults);
    resetToKropy();
    onDirtyChange?.(true);
    toast.info('Vista previa restaurada al estilo Kropy. Guarda para aplicar los cambios.');
  };

  const handleToggleFeature = (key: keyof TenantFeatures) => {
    if (!canEditSettings) return;
    setFeatures((prev) => ({
      ...prev,
      [key]: !prev[key],
    }));
    onDirtyChange?.(true);
  };

  const handleCopyAccessLink = async () => {
    const slug = tenantMeta?.slug;
    const url = slug
      ? `${window.location.origin}/?tenant=${encodeURIComponent(slug)}`
      : window.location.origin;
    try {
      await navigator.clipboard.writeText(url);
      setCopiedLink(true);
      toast.success('Enlace de acceso copiado al portapapeles.');
      setTimeout(() => setCopiedLink(false), 2500);
    } catch {
      toast.error('No se pudo copiar el enlace.');
    }
  };

  const handleSave = async (e: FormEvent) => {
    e.preventDefault();
    if (!canEditSettings) return;

    if (!HEX_REGEX.test(branding.primaryColor) || !HEX_REGEX.test(branding.accentColor)) {
      toast.error('Los colores deben tener un código hexadecimal válido (ej: #059669).');
      return;
    }

    const trimmedLogo = branding.logoUrl.trim();
    const sanitizedLogo = trimmedLogo ? sanitizeLogoUrl(trimmedLogo) : '';
    if (trimmedLogo && !sanitizedLogo) {
      toast.error('La URL del logotipo debe usar https:// o una ruta relativa válida.');
      return;
    }

    setSaving(true);
    try {
      const updated = await updateSettings({
        branding: {
          displayName: branding.displayName.trim() || 'Kropy',
          logoUrl: sanitizedLogo ?? '',
          primaryColor: branding.primaryColor,
          accentColor: branding.accentColor,
          themeMode: branding.themeMode,
        },
        locale: {
          currency: locale.currency,
          timezone: locale.timezone,
          language: locale.language,
        },
        features,
      });

      setBranding({ ...updated.branding });
      setLocale({ ...updated.locale });
      setFeatures({ ...updated.features });
      onDirtyChange?.(false);
      toast.success('Ajustes del negocio guardados correctamente.');
    } catch (err) {
      if (!(err instanceof BffApiError && err.notified)) {
        toast.error(err instanceof Error ? err.message : 'Error guardando los ajustes.');
      }
    } finally {
      setSaving(false);
    }
  };

  const displayName = branding.displayName.trim() || 'Kropy';
  const enabledCount = Object.values(features).filter(Boolean).length;
  const safePrimarySwatch = HEX_REGEX.test(branding.primaryColor)
    ? branding.primaryColor
    : KROPY_DEFAULT_BRANDING.primaryColor;
  const safeAccentSwatch = HEX_REGEX.test(branding.accentColor)
    ? branding.accentColor
    : KROPY_DEFAULT_BRANDING.accentColor;

  return (
    <form className="stack-lg" onSubmit={handleSave}>
      <div className="page-header">
        <div>
          <h1 className="page-title">Ajustes del Negocio</h1>
          <p className="page-subtitle">
            Personaliza la apariencia, moneda, zona horaria y módulos activos de tu portal.
          </p>
        </div>
        <div className="page-actions">
          <Button
            variant="secondary"
            type="button"
            icon={copiedLink ? <Check size={15} /> : <Link2 size={15} />}
            onClick={handleCopyAccessLink}
          >
            {copiedLink ? 'Enlace copiado' : 'Copiar enlace de acceso'}
          </Button>
          <Button
            variant="secondary"
            type="button"
            icon={<RotateCcw size={15} />}
            onClick={handleResetKropy}
            disabled={!canEditSettings || saving}
          >
            Restaurar estilo Kropy
          </Button>
          <Button
            variant="primary"
            type="submit"
            icon={<Save size={15} />}
            isLoading={saving}
            disabled={!canEditSettings}
          >
            Guardar cambios
          </Button>
        </div>
      </div>

      {!canEditSettings && (
        <div className="alert-banner alert-banner-info" role="status">
          <Info size={17} />
          <div className="alert-banner-body">
            Estás viendo los ajustes en modo solo lectura. Solo un Administrador puede guardar
            cambios.
          </div>
        </div>
      )}

      <div className="settings-layout">
        {/* Left Column: Form Sections */}
        <div className="stack-lg">
          {/* Branding */}
          <div className="card">
            <div className="card-header">
              <div className="row-sm">
                <Palette size={18} />
                <div>
                  <h2 className="card-title">Identidad y Marca</h2>
                  <p className="card-subtitle">
                    Previsualiza los colores y el tema de tu negocio en tiempo real.
                  </p>
                </div>
              </div>
              {tenantMeta?.name && <StatusBadge label={tenantMeta.name} tone="gray" />}
            </div>

            <div className="form-grid">
              <div className="form-grid-2">
                <Input
                  label="Nombre visible del negocio"
                  value={branding.displayName}
                  onChange={(e) => updateBrandingField('displayName', e.target.value)}
                  placeholder="Kropy"
                  disabled={!canEditSettings}
                />
                <Select
                  label="Apariencia"
                  value={branding.themeMode}
                  onChange={(e) =>
                    updateBrandingField('themeMode', e.target.value as ThemeMode)
                  }
                  disabled={!canEditSettings}
                >
                  <option value="light">Claro</option>
                  <option value="dark">Oscuro</option>
                  <option value="system">Automático (según tu dispositivo)</option>
                </Select>
              </div>

              <div className="input-action-row">
                <Input
                  label="URL del logotipo"
                  type="url"
                  value={branding.logoUrl}
                  onChange={(e) => updateBrandingField('logoUrl', e.target.value)}
                  placeholder="https://tunegocio.com/logo.png"
                  hint="Si lo dejas vacío o la imagen no carga, mostraremos tus iniciales."
                  error={
                    branding.logoUrl.trim() && !sanitizeLogoUrl(branding.logoUrl)
                      ? 'Usa una URL https:// o una ruta relativa válida (/logo.png)'
                      : undefined
                  }
                  disabled={!canEditSettings}
                />
                {canEditSettings && (
                  <Button
                    type="button"
                    size="md"
                    variant="secondary"
                    className="input-action-btn"
                    icon={<ImageIcon size={15} />}
                    onClick={() =>
                      openMediaPicker({
                        context: 'branding',
                        title: 'Seleccionar logotipo del negocio',
                        allowUpload: user.roles.some(
                          (r) => r === 'tenant:admin' || r === 'platform:admin',
                        ),
                        onSelect: (url) => updateBrandingField('logoUrl', url),
                      })
                    }
                  >
                    Galería
                  </Button>
                )}
              </div>

              <div className="field">
                <span className="field-label">Paletas sugeridas</span>
                <div className="palette-grid">
                  {CURATED_PALETTES.map((palette) => {
                    const isActive =
                      branding.primaryColor.toLowerCase() ===
                        palette.primaryColor.toLowerCase() &&
                      branding.accentColor.toLowerCase() === palette.accentColor.toLowerCase();
                    return (
                      <button
                        key={palette.name}
                        type="button"
                        className={`palette-btn ${isActive ? 'is-active' : ''}`}
                        onClick={() => applyPalette(palette.primaryColor, palette.accentColor)}
                        disabled={!canEditSettings}
                      >
                        <span className="truncate">{palette.name}</span>
                        <span className="palette-dots" aria-hidden="true">
                          <svg width="14" height="14" viewBox="0 0 14 14">
                            <circle cx="7" cy="7" r="6" fill={palette.primaryColor} />
                          </svg>
                          <svg width="14" height="14" viewBox="0 0 14 14">
                            <circle cx="7" cy="7" r="6" fill={palette.accentColor} />
                          </svg>
                        </span>
                      </button>
                    );
                  })}
                </div>
              </div>

              <div className="form-grid-2">
                <div className="field">
                  <label className="field-label" htmlFor="settings-primary-hex">
                    Color principal personalizado
                  </label>
                  <div className="color-picker-row">
                    <input
                      type="color"
                      className="color-swatch-input"
                      value={safePrimarySwatch}
                      onChange={(e) => updateBrandingField('primaryColor', e.target.value)}
                      aria-label="Selector de color principal"
                      disabled={!canEditSettings}
                    />
                    <Input
                      id="settings-primary-hex"
                      value={branding.primaryColor}
                      onChange={(e) => updateBrandingField('primaryColor', e.target.value)}
                      placeholder="#059669"
                      maxLength={7}
                      error={
                        branding.primaryColor && !HEX_REGEX.test(branding.primaryColor)
                          ? 'Usa formato #RRGGBB'
                          : undefined
                      }
                      disabled={!canEditSettings}
                    />
                  </div>
                </div>

                <div className="field">
                  <label className="field-label" htmlFor="settings-accent-hex">
                    Color secundario personalizado
                  </label>
                  <div className="color-picker-row">
                    <input
                      type="color"
                      className="color-swatch-input"
                      value={safeAccentSwatch}
                      onChange={(e) => updateBrandingField('accentColor', e.target.value)}
                      aria-label="Selector de color secundario"
                      disabled={!canEditSettings}
                    />
                    <Input
                      id="settings-accent-hex"
                      value={branding.accentColor}
                      onChange={(e) => updateBrandingField('accentColor', e.target.value)}
                      placeholder="#0f172a"
                      maxLength={7}
                      error={
                        branding.accentColor && !HEX_REGEX.test(branding.accentColor)
                          ? 'Usa formato #RRGGBB'
                          : undefined
                      }
                      disabled={!canEditSettings}
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Regional / Locale */}
          <div className="card">
            <div className="card-header">
              <div className="row-sm">
                <Globe size={18} />
                <div>
                  <h2 className="card-title">Región e Idioma</h2>
                  <p className="card-subtitle">
                    Moneda por defecto, zona horaria e idioma del portal.
                  </p>
                </div>
              </div>
            </div>

            <div className="grid-3">
              <Select
                label="Moneda"
                value={locale.currency}
                onChange={(e) => {
                  setLocale((prev) => ({ ...prev, currency: e.target.value }));
                  onDirtyChange?.(true);
                }}
                disabled={!canEditSettings}
              >
                {CURRENCY_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </Select>

              <Select
                label="Zona horaria"
                value={locale.timezone}
                onChange={(e) => {
                  setLocale((prev) => ({ ...prev, timezone: e.target.value }));
                  onDirtyChange?.(true);
                }}
                disabled={!canEditSettings}
              >
                {timezoneOptions.map((tz) => (
                  <option key={tz} value={tz}>
                    {tz}
                  </option>
                ))}
              </Select>

              <Select
                label="Idioma"
                value={locale.language === 'en' ? 'es' : locale.language}
                onChange={(e) => {
                  setLocale((prev) => ({ ...prev, language: e.target.value }));
                  onDirtyChange?.(true);
                }}
                disabled={!canEditSettings}
              >
                <option value="es">Español</option>
                <option value="en" disabled>
                  English (Próximamente)
                </option>
              </Select>
            </div>
          </div>

          {/* Platform Modules */}
          <div className="card">
            <div className="card-header">
              <div className="row-sm">
                <Layers size={18} />
                <div>
                  <h2 className="card-title">Módulos activos</h2>
                  <p className="card-subtitle">
                    Elige qué secciones mostrar en el menú lateral para tu equipo.
                  </p>
                </div>
              </div>
              <StatusBadge
                label={`${enabledCount} de ${FEATURE_MODULES.length} activos`}
                tone="emerald"
              />
            </div>

            <div className="module-toggle-grid">
              {FEATURE_MODULES.map((mod) => {
                const enabled = features[mod.key] !== false;
                return (
                  <label
                    key={mod.key}
                    className={`module-toggle-card ${enabled ? 'is-enabled' : ''} ${!canEditSettings ? 'is-disabled' : ''}`}
                  >
                    <div className="stack-xs">
                      <span className="font-semibold text-sm">{mod.label}</span>
                      <span className="text-xs text-muted">{mod.description}</span>
                    </div>
                    <input
                      type="checkbox"
                      className="module-toggle-checkbox"
                      checked={enabled}
                      onChange={() => handleToggleFeature(mod.key)}
                      disabled={!canEditSettings}
                      aria-label={`Activar módulo ${mod.label}`}
                    />
                  </label>
                );
              })}
            </div>
          </div>
        </div>

        {/* Right Column: Live Preview Card */}
        <div className="card preview-card">
          <div className="card-header">
            <div className="row-sm">
              <Sparkles size={17} />
              <h2 className="card-title">Vista previa en vivo</h2>
            </div>
            <StatusBadge label="Tiempo real" tone="emerald" />
          </div>

          <div className="preview-mockup">
            <div className="preview-mockup-header">
              <div className="sidebar-brand">
                <BrandLogo logoUrl={branding.logoUrl} name={displayName} className="brand-mark" />
                <div className="brand-meta">
                  <div className="brand-title">{displayName}</div>
                  <div className="brand-subtitle">{tenantMeta?.name ?? 'Tu negocio'}</div>
                </div>
              </div>
              <StatusBadge
                label={
                  branding.themeMode === 'dark'
                    ? 'Oscuro'
                    : branding.themeMode === 'system'
                      ? 'Auto'
                      : 'Claro'
                }
                tone="blue"
              />
            </div>

            <div className="preview-mockup-body">
              <div className="detail-list">
                <div className="detail-row">
                  <span className="text-muted">Precio de ejemplo</span>
                  <strong>{formatMoney(45000, locale.currency)}</strong>
                </div>
                <div className="detail-row">
                  <span className="text-muted">Zona horaria</span>
                  <span className="text-xs font-medium">{locale.timezone}</span>
                </div>
                <div className="detail-row">
                  <span className="text-muted">Módulos activos</span>
                  <span>{enabledCount} módulos</span>
                </div>
              </div>

              <div className="row-wrap">
                <Button variant="primary" size="sm" type="button" icon={<CheckCircle2 size={14} />}>
                  Acción principal
                </Button>
                <Button variant="secondary" size="sm" type="button">
                  Acción secundaria
                </Button>
              </div>
            </div>
          </div>

          <p className="text-xs text-muted">
            Al guardar, la marca y los colores seleccionados se aplicarán automáticamente para
            todos los miembros de tu equipo.
          </p>
        </div>
      </div>
    </form>
  );
}
