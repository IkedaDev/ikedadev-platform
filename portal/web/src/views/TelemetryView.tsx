import {
  Activity,
  BarChart3,
  Fingerprint,
  RefreshCw,
  Sparkles,
  Users,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { bffClient } from '../api/bffClient';
import type {
  PaginationMeta,
  TelemetryEvent,
  TelemetryEventKind,
  TelemetryMetricsSummary,
} from '../types/api';
import {
  Button,
  DataTable,
  EmptyState,
  Skeleton,
  StatCard,
  StatusBadge,
  formatDateTime,
  type DataTableColumn,
} from '../ui';

type KindFilter = 'ALL' | TelemetryEventKind;
type DateRangeOption = '24h' | '7d' | '30d';

const RANGE_HOURS: Record<DateRangeOption, number> = {
  '24h': 24,
  '7d': 7 * 24,
  '30d': 30 * 24,
};

const RANGE_LABELS: Record<DateRangeOption, string> = {
  '24h': 'Últimas 24h',
  '7d': 'Últimos 7 días',
  '30d': 'Últimos 30 días',
};

const KIND_TAB_LABELS: Record<KindFilter, string> = {
  ALL: 'Todos',
  TRACKING: 'Navegación',
  AUDIT: 'Cambios del equipo',
};

const HUMAN_EVENT_NAMES: Record<string, string> = {
  'page.viewed': 'Vista visitada',
  'client.created': 'Cliente creado',
  'client.updated': 'Cliente actualizado',
  'client.deleted': 'Cliente eliminado',
  'booking.created': 'Reserva creada',
  'booking.updated': 'Reserva actualizada',
  'booking.cancelled': 'Reserva cancelada',
  'booking.completed': 'Reserva completada',
  'booking.deleted': 'Reserva eliminada',
  'catalog.created': 'Ítem creado',
  'catalog.updated': 'Ítem actualizado',
  'catalog.deleted': 'Ítem eliminado',
  'catalog.item_created': 'Ítem creado',
  'catalog.item_updated': 'Ítem actualizado',
  'catalog.item_deleted': 'Ítem eliminado',
  'catalog.category_created': 'Categoría creada',
  'catalog.category_updated': 'Categoría actualizada',
  'catalog.category_deleted': 'Categoría eliminada',
  'calendar.created': 'Calendario creado',
  'settings.updated': 'Ajustes actualizados',
  'user.created': 'Miembro agregado',
  'user.deleted': 'Miembro eliminado',
  'api_key.created': 'Clave de integración creada',
  'api_key.revoked': 'Clave de integración revocada',
  'content.article.published': 'Artículo publicado',
  'content.article.scheduled': 'Artículo programado',
  'content.article.archived': 'Artículo archivado',
  'content.article.status_changed': 'Estado de artículo actualizado',
  'content.article.deleted': 'Artículo eliminado',
  'media.asset.uploaded': 'Archivo multimedia subido',
  'media.asset.purged': 'Archivo multimedia eliminado definitivamente',
};

function formatEventName(name: string): string {
  return (
    HUMAN_EVENT_NAMES[name] ??
    name
      .replace(/[._-]+/g, ' ')
      .replace(/^\w/, (c) => c.toUpperCase())
  );
}

function formatSourceLabel(source: string): string {
  const s = source.toLowerCase();
  if (s.includes('web') || s.includes('portal')) return 'Portal web';
  if (s.includes('bff') || s.includes('api') || s.includes('server')) return 'Sistema';
  return source;
}

function formatPropertiesPreview(properties: Record<string, unknown>): string {
  const entries = Object.entries(properties ?? {});
  if (entries.length === 0) return '—';
  return entries
    .slice(0, 3)
    .map(([k, v]) => `${k}: ${typeof v === 'object' && v !== null ? JSON.stringify(v) : String(v)}`)
    .join(' · ');
}

export function TelemetryView() {
  const [range, setRange] = useState<DateRangeOption>('7d');
  const [kindFilter, setKindFilter] = useState<KindFilter>('ALL');
  const [page, setPage] = useState(1);

  const [metrics, setMetrics] = useState<TelemetryMetricsSummary | null>(null);
  const [events, setEvents] = useState<TelemetryEvent[]>([]);
  const [pagination, setPagination] = useState<PaginationMeta | null>(null);
  const [metricsLoading, setMetricsLoading] = useState(true);
  const [eventsLoading, setEventsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadMetrics = useCallback(async () => {
    setMetricsLoading(true);
    setError(null);
    try {
      const now = new Date();
      const from = new Date(now.getTime() - RANGE_HOURS[range] * 60 * 60 * 1000).toISOString();
      const to = now.toISOString();
      const kind = kindFilter === 'ALL' ? undefined : kindFilter;

      const summary = await bffClient.getTelemetryMetrics({ from, to, kind });
      setMetrics(summary);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'No pudimos cargar las métricas de actividad.');
    } finally {
      setMetricsLoading(false);
    }
  }, [range, kindFilter]);

  const loadEvents = useCallback(async () => {
    setEventsLoading(true);
    setError(null);
    try {
      const kind = kindFilter === 'ALL' ? undefined : kindFilter;
      const eventsPage = await bffClient.listTelemetryEvents({ page, limit: 20, kind });
      setEvents(eventsPage.data ?? []);
      setPagination(eventsPage.pagination ?? null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'No pudimos cargar el historial de actividad.');
    } finally {
      setEventsLoading(false);
    }
  }, [kindFilter, page]);

  useEffect(() => {
    void loadMetrics();
  }, [loadMetrics]);

  useEffect(() => {
    void loadEvents();
  }, [loadEvents]);

  const load = useCallback(() => {
    void Promise.all([loadMetrics(), loadEvents()]);
  }, [loadMetrics, loadEvents]);

  const loading = metricsLoading || eventsLoading;

  const handleKindChange = (next: KindFilter) => {
    setKindFilter(next);
    setPage(1);
  };

  const handleRangeChange = (next: DateRangeOption) => {
    setRange(next);
  };

  const breakdown = metrics?.by_event_name ?? [];
  const topEvent = breakdown[0] ?? null;
  const maxEventCount = breakdown.reduce((max, item) => Math.max(max, item.count), 1);

  const columns = useMemo<DataTableColumn<TelemetryEvent>[]>(
    () => [
      {
        key: 'occurred_at',
        header: 'Fecha y hora',
        sortable: true,
        sortValue: (ev) => ev.occurred_at,
        render: (ev) => <span className="text-xs">{formatDateTime(ev.occurred_at)}</span>,
      },
      {
        key: 'kind',
        header: 'Categoría',
        sortable: true,
        sortValue: (ev) => ev.kind,
        render: (ev) => (
          <StatusBadge
            label={ev.kind === 'AUDIT' ? 'Cambio del equipo' : 'Navegación'}
            tone={ev.kind === 'AUDIT' ? 'amber' : 'blue'}
          />
        ),
      },
      {
        key: 'event_name',
        header: 'Acción',
        sortable: true,
        sortValue: (ev) => formatEventName(ev.event_name),
        render: (ev) => <span className="font-semibold text-sm">{formatEventName(ev.event_name)}</span>,
      },
      {
        key: 'source',
        header: 'Origen',
        hideOnMobile: true,
        render: (ev) => <StatusBadge label={formatSourceLabel(ev.source)} tone="gray" />,
      },
      {
        key: 'resource',
        header: 'Elemento',
        hideOnMobile: true,
        render: (ev) =>
          ev.resource_type ? (
            <span className="text-xs font-medium">{ev.resource_type}</span>
          ) : (
            <span className="text-muted">—</span>
          ),
      },
      {
        key: 'details',
        header: 'Detalles',
        hideOnMobile: true,
        render: (ev) => (
          <span
            className="text-xs text-muted cell-truncate"
            title={formatPropertiesPreview(ev.properties)}
          >
            {formatPropertiesPreview(ev.properties)}
          </span>
        ),
      },
    ],
    [],
  );

  return (
    <div className="stack-lg">
      <div className="page-header">
        <div>
          <h1 className="page-title">Historial de Actividad</h1>
          <p className="page-subtitle">
            Revisa la navegación del portal y los cambios realizados por tu equipo en tiempo real.
          </p>
        </div>

        <div className="page-actions">
          <div className="tabs">
            {(['24h', '7d', '30d'] as DateRangeOption[]).map((opt) => (
              <button
                key={opt}
                type="button"
                className={`tab-btn ${range === opt ? 'active' : ''}`}
                onClick={() => handleRangeChange(opt)}
              >
                {RANGE_LABELS[opt]}
              </button>
            ))}
          </div>

          <Button
            variant="secondary"
            icon={<RefreshCw size={15} className={loading ? 'btn-spinner' : ''} />}
            onClick={load}
            disabled={loading}
          >
            Actualizar
          </Button>
        </div>
      </div>

      {error && (
        <div className="alert-banner alert-banner-danger" role="alert">
          <div className="alert-banner-body">{error}</div>
        </div>
      )}

      {/* KPI Cards */}
      <div className="kpi-grid">
        <StatCard
          label="Total de acciones"
          value={metrics?.total_events ?? 0}
          icon={<Activity size={18} />}
          helper={RANGE_LABELS[range]}
          isLoading={metricsLoading}
        />
        <StatCard
          label="Clientes involucrados"
          value={metrics?.unique_clients ?? 0}
          icon={<Users size={18} />}
          helper="Con actividad registrada"
          isLoading={metricsLoading}
        />
        <StatCard
          label="Sesiones activas"
          value={metrics?.unique_sessions ?? 0}
          icon={<Fingerprint size={18} />}
          helper="Sesiones registradas en el período"
          isLoading={metricsLoading}
        />
        <StatCard
          label="Acción más frecuente"
          value={
            <span className="text-sm font-bold">
              {topEvent ? formatEventName(topEvent.event_name) : 'Sin actividad'}
            </span>
          }
          icon={<Sparkles size={18} />}
          helper={
            topEvent ? `${topEvent.count} veces registrada` : 'Aún no hay actividad registrada'
          }
          isLoading={metricsLoading}
        />
      </div>

      {/* Breakdown by event_name */}
      <div className="card">
        <div className="card-header">
          <div className="row-sm">
            <BarChart3 size={18} />
            <h2 className="card-title">Acciones más frecuentes</h2>
          </div>
          <StatusBadge label={`${breakdown.length} tipos`} tone="blue" />
        </div>

        {metricsLoading ? (
          <div className="stack-sm">
            <Skeleton variant="line" />
            <Skeleton variant="line" />
            <Skeleton variant="line-sm" />
          </div>
        ) : breakdown.length === 0 ? (
          <EmptyState
            title="Sin distribución disponible"
            description={`No se registró actividad en el período seleccionado (${RANGE_LABELS[range]}).`}
            icon={<BarChart3 size={22} />}
          />
        ) : (
          <div className="bar-list">
            {breakdown.slice(0, 10).map((item) => {
              const pct = Math.max(4, Math.round((item.count / maxEventCount) * 100));
              const label = formatEventName(item.event_name);
              return (
                <div key={item.event_name} className="bar-row">
                  <div className="bar-row-header">
                    <span className="text-xs font-semibold">{label}</span>
                    <span className="text-xs font-semibold">{item.count}</span>
                  </div>
                  <svg
                    viewBox="0 0 100 8"
                    preserveAspectRatio="none"
                    className="bar-track"
                    role="img"
                    aria-label={`${label}: ${item.count}`}
                  >
                    <rect
                      x="0"
                      y="0"
                      width={pct}
                      height="8"
                      rx="4"
                      fill="var(--color-primary)"
                    />
                  </svg>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Recent Activity Timeline */}
      <div className="stack-sm">
        <div className="row-between row-wrap">
          <h2 className="card-title">Registro de actividad reciente</h2>
        </div>

        <DataTable<TelemetryEvent>
          columns={columns}
          data={events}
          rowKey={(ev) => ev.id}
          isLoading={eventsLoading}
          toolbar={
            <div className="tabs">
              {(['ALL', 'TRACKING', 'AUDIT'] as KindFilter[]).map((k) => (
                <button
                  key={k}
                  type="button"
                  className={`tab-btn ${kindFilter === k ? 'active' : ''}`}
                  onClick={() => handleKindChange(k)}
                >
                  {KIND_TAB_LABELS[k]}
                </button>
              ))}
            </div>
          }
          page={pagination?.page ?? page}
          totalPages={pagination?.total_pages ?? 1}
          total={pagination?.total_items}
          onPageChange={setPage}
          emptyTitle="Sin actividad registrada"
          emptyDescription="Navega por el portal o realiza cambios en clientes, catálogo o reservas para ver el historial."
          emptyIcon={<Activity size={22} />}
        />
      </div>
    </div>
  );
}
