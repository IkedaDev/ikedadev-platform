import {
  Activity,
  BarChart3,
  ChevronLeft,
  ChevronRight,
  Fingerprint,
  RefreshCw,
  Sparkles,
  Users,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { bffClient } from '../api/bffClient';
import { StatusBadge, formatDateTime } from '../components/StatusBadge';
import type {
  PaginationMeta,
  TelemetryEvent,
  TelemetryEventKind,
  TelemetryMetricsSummary,
} from '../types/api';

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
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const now = new Date();
      const from = new Date(now.getTime() - RANGE_HOURS[range] * 60 * 60 * 1000).toISOString();
      const to = now.toISOString();
      const kind = kindFilter === 'ALL' ? undefined : kindFilter;

      const [summary, eventsPage] = await Promise.all([
        bffClient.getTelemetryMetrics({ from, to, kind }),
        bffClient.listTelemetryEvents({ page, limit: 20, kind }),
      ]);

      setMetrics(summary);
      setEvents(eventsPage.data ?? []);
      setPagination(eventsPage.pagination ?? null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error cargando actividad y telemetría');
    } finally {
      setLoading(false);
    }
  }, [range, kindFilter, page]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleKindChange = (next: KindFilter) => {
    setKindFilter(next);
    setPage(1);
  };

  const handleRangeChange = (next: DateRangeOption) => {
    setRange(next);
    setPage(1);
  };

  const breakdown = metrics?.by_event_name ?? [];
  const topEvent = breakdown[0] ?? null;
  const maxEventCount = breakdown.reduce((max, item) => Math.max(max, item.count), 1);

  return (
    <div>
      <div className="page-header">
        <div>
          <h1 className="page-title">Actividad y Telemetría</h1>
          <p className="page-subtitle">
            Eventos de tracking del portal y auditoría del BFF consolidados en tiempo real desde{' '}
            <code>telemetry-api</code>.
          </p>
        </div>

        <div className="btn-row">
          <div className="tabs">
            {(['24h', '7d', '30d'] as DateRangeOption[]).map((opt) => (
              <button
                key={opt}
                type="button"
                className={`tab ${range === opt ? 'active' : ''}`}
                onClick={() => handleRangeChange(opt)}
              >
                {RANGE_LABELS[opt]}
              </button>
            ))}
          </div>

          <button type="button" className="btn btn-secondary" onClick={load} disabled={loading}>
            <RefreshCw size={15} />
            Actualizar
          </button>
        </div>
      </div>

      {error && <div className="alert alert-error">{error}</div>}

      {/* KPI Cards */}
      <div className="kpi-grid">
        <div className="card kpi-card">
          <div className="kpi-label">
            <span>Total Eventos</span>
            <Activity size={17} />
          </div>
          <div className="kpi-value">{loading ? '—' : (metrics?.total_events ?? 0)}</div>
          <div className="kpi-hint">{RANGE_LABELS[range]}</div>
        </div>

        <div className="card kpi-card">
          <div className="kpi-label">
            <span>Clientes Únicos</span>
            <Users size={17} />
          </div>
          <div className="kpi-value">{loading ? '—' : (metrics?.unique_clients ?? 0)}</div>
          <div className="kpi-hint">Con client_id asociado</div>
        </div>

        <div className="card kpi-card">
          <div className="kpi-label">
            <span>Sesiones Únicas</span>
            <Fingerprint size={17} />
          </div>
          <div className="kpi-value">{loading ? '—' : (metrics?.unique_sessions ?? 0)}</div>
          <div className="kpi-hint">Sesiones web registradas</div>
        </div>

        <div className="card kpi-card">
          <div className="kpi-label">
            <span>Evento Principal</span>
            <Sparkles size={17} />
          </div>
          <div
            className="kpi-value"
            style={{ fontSize: 20, wordBreak: 'break-word', lineHeight: 1.25 }}
          >
            {loading ? '—' : (topEvent?.event_name ?? 'Sin eventos')}
          </div>
          <div className="kpi-hint">
            {topEvent ? `${topEvent.count} ocurrencias` : 'Aún no hay actividad registrada'}
          </div>
        </div>
      </div>

      {/* Breakdown by event_name */}
      <div className="card" style={{ marginBottom: 20 }}>
        <div className="section-header">
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <BarChart3 size={17} />
            <h2 className="section-title">Distribución por Evento</h2>
          </div>
          <StatusBadge label={`${breakdown.length} tipos de evento`} tone="blue" />
        </div>

        {loading ? (
          <div className="empty-state">Cargando distribución de eventos...</div>
        ) : breakdown.length === 0 ? (
          <div className="empty-state">
            No se registraron eventos en el rango seleccionado ({RANGE_LABELS[range]}).
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {breakdown.slice(0, 10).map((item) => {
              const pct = Math.max(4, Math.round((item.count / maxEventCount) * 100));
              return (
                <div
                  key={item.event_name}
                  style={{
                    display: 'grid',
                    gridTemplateColumns: 'minmax(160px, 220px) 1fr 64px',
                    alignItems: 'center',
                    gap: 12,
                  }}
                >
                  <code style={{ fontSize: 12, fontWeight: 600 }}>{item.event_name}</code>
                  <div
                    style={{
                      height: 10,
                      background: 'var(--surface-muted)',
                      borderRadius: 999,
                      overflow: 'hidden',
                    }}
                  >
                    <div
                      style={{
                        width: `${pct}%`,
                        height: '100%',
                        background: 'var(--primary)',
                        borderRadius: 999,
                      }}
                    />
                  </div>
                  <div style={{ textAlign: 'right', fontWeight: 600, fontSize: 13 }}>
                    {item.count}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Recent Activity Timeline */}
      <div className="card">
        <div className="section-header">
          <h2 className="section-title">Timeline de Actividad Reciente</h2>

          <div className="tabs">
            {(['ALL', 'TRACKING', 'AUDIT'] as KindFilter[]).map((k) => (
              <button
                key={k}
                type="button"
                className={`tab ${kindFilter === k ? 'active' : ''}`}
                onClick={() => handleKindChange(k)}
              >
                {k === 'ALL' ? 'Todos' : k}
              </button>
            ))}
          </div>
        </div>

        {loading ? (
          <div className="empty-state">Cargando eventos recientes...</div>
        ) : events.length === 0 ? (
          <div className="empty-state">
            No hay eventos registrados para el filtro seleccionado. Navega por el portal o crea un
            cliente, ítem o reserva para generar actividad.
          </div>
        ) : (
          <>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Fecha / Hora</th>
                    <th>Tipo</th>
                    <th>Evento</th>
                    <th>Origen</th>
                    <th>Recurso</th>
                    <th>Request ID</th>
                    <th>Detalles</th>
                  </tr>
                </thead>
                <tbody>
                  {events.map((ev) => (
                    <tr key={ev.id}>
                      <td style={{ whiteSpace: 'nowrap', fontSize: 12 }}>
                        {formatDateTime(ev.occurred_at)}
                      </td>
                      <td>
                        <StatusBadge
                          label={ev.kind}
                          tone={ev.kind === 'AUDIT' ? 'amber' : 'blue'}
                        />
                      </td>
                      <td>
                        <code style={{ fontSize: 12, fontWeight: 600 }}>{ev.event_name}</code>
                      </td>
                      <td>
                        <StatusBadge label={ev.source} tone="gray" />
                      </td>
                      <td style={{ fontSize: 12 }}>
                        {ev.resource_type ? (
                          <div>
                            <span style={{ fontWeight: 500 }}>{ev.resource_type}</span>
                            {ev.resource_id && (
                              <div style={{ color: 'var(--text-muted)', fontSize: 11 }}>
                                <code>{ev.resource_id.slice(0, 8)}...</code>
                              </div>
                            )}
                          </div>
                        ) : (
                          <span style={{ color: 'var(--text-muted)' }}>—</span>
                        )}
                      </td>
                      <td style={{ fontSize: 11 }}>
                        {ev.request_id ? (
                          <code title={ev.request_id}>{ev.request_id.slice(0, 8)}...</code>
                        ) : (
                          <span style={{ color: 'var(--text-muted)' }}>—</span>
                        )}
                      </td>
                      <td
                        style={{
                          fontSize: 12,
                          color: 'var(--text-muted)',
                          maxWidth: 280,
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                        title={JSON.stringify(ev.properties ?? {})}
                      >
                        {formatPropertiesPreview(ev.properties)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {pagination && pagination.total_pages > 1 && (
              <div className="pagination-bar">
                <span>
                  Página {pagination.page} de {pagination.total_pages} ({pagination.total_items}{' '}
                  eventos)
                </span>
                <div className="btn-row">
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    disabled={!pagination.has_prev_page || loading}
                    onClick={() => setPage((p) => Math.max(1, p - 1))}
                  >
                    <ChevronLeft size={14} />
                    Anterior
                  </button>
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    disabled={!pagination.has_next_page || loading}
                    onClick={() => setPage((p) => p + 1)}
                  >
                    Siguiente
                    <ChevronRight size={14} />
                  </button>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
