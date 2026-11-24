import { CalendarClock, DollarSign, Package, RefreshCw, Users } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { bffClient } from '../api/bffClient';
import { StatusBadge, formatDateTime, formatMoney } from '../components/StatusBadge';
import type { DashboardSummary } from '../types/api';

export function DashboardView() {
  const [data, setData] = useState<DashboardSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const summary = await bffClient.getDashboard();
      setData(summary);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error cargando dashboard');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div>
      <div className="page-header">
        <div>
          <h1 className="page-title">Dashboard General</h1>
          <p className="page-subtitle">
            Resumen consolidado en paralelo desde crm-api, catalog-api y events-api vía BFF.
          </p>
        </div>
        <button type="button" className="btn btn-secondary" onClick={load} disabled={loading}>
          <RefreshCw size={15} />
          Actualizar
        </button>
      </div>

      {error && <div className="alert alert-error">{error}</div>}

      <div className="kpi-grid">
        <div className="card kpi-card">
          <div className="kpi-label">
            <span>Total Clientes</span>
            <Users size={17} />
          </div>
          <div className="kpi-value">{loading ? '—' : (data?.kpis.totalClients ?? 0)}</div>
          <div className="kpi-hint">Activos en crm-api</div>
        </div>

        <div className="card kpi-card">
          <div className="kpi-label">
            <span>Ítems en Catálogo</span>
            <Package size={17} />
          </div>
          <div className="kpi-value">{loading ? '—' : (data?.kpis.activeCatalogItems ?? 0)}</div>
          <div className="kpi-hint">Servicios, productos y bundles activos</div>
        </div>

        <div className="card kpi-card">
          <div className="kpi-label">
            <span>Próximas Reservas</span>
            <CalendarClock size={17} />
          </div>
          <div className="kpi-value">{loading ? '—' : (data?.kpis.upcomingBookingsCount ?? 0)}</div>
          <div className="kpi-hint">En los próximos 30 días</div>
        </div>

        <div className="card kpi-card">
          <div className="kpi-label">
            <span>Ingreso Agendado</span>
            <DollarSign size={17} />
          </div>
          <div className="kpi-value">
            {loading
              ? '—'
              : formatMoney(data?.kpis.upcomingRevenue ?? 0, data?.kpis.currency ?? 'CLP')}
          </div>
          <div className="kpi-hint">Suma calculada desde metadata de reservas</div>
        </div>
      </div>

      <div className="card">
        <div className="section-header">
          <h2 className="section-title">Próximas Citas Enriquecidas</h2>
          <StatusBadge label="Orquestación BFF (Events + CRM + Catalog)" tone="blue" />
        </div>

        {loading ? (
          <div className="empty-state">Cargando resumen...</div>
        ) : !data || data.upcomingBookings.length === 0 ? (
          <div className="empty-state">
            No hay reservas próximas. Ejecuta <code>npm run seed</code> en el BFF o crea una en{' '}
            <strong>Agenda y Reservas</strong>.
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Fecha / Hora</th>
                  <th>Cliente</th>
                  <th>Servicios / Ítems</th>
                  <th>Calendario</th>
                  <th>Total</th>
                  <th>Estado</th>
                </tr>
              </thead>
              <tbody>
                {data.upcomingBookings.map((booking) => (
                  <tr key={booking.id}>
                    <td>
                      <div style={{ fontWeight: 600 }}>{formatDateTime(booking.starts_at)}</div>
                      <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                        hasta {formatDateTime(booking.ends_at)}
                      </div>
                    </td>
                    <td>
                      <div style={{ fontWeight: 500 }}>{booking.client_name ?? 'Sin cliente'}</div>
                      {booking.client_email && (
                        <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                          {booking.client_email}
                        </div>
                      )}
                    </td>
                    <td>
                      {booking.items.length > 0 ? (
                        <div className="btn-row">
                          {booking.items.map((item) => (
                            <StatusBadge key={item.id} label={item.name} tone="gray" />
                          ))}
                        </div>
                      ) : (
                        <span style={{ color: 'var(--text-muted)' }}>{booking.title}</span>
                      )}
                    </td>
                    <td>{booking.calendar_name ?? booking.calendar_id.slice(0, 8)}</td>
                    <td style={{ fontWeight: 600 }}>
                      {formatMoney(booking.total_price, booking.currency)}
                    </td>
                    <td>
                      <StatusBadge
                        label={booking.status}
                        tone={
                          booking.status === 'CONFIRMED'
                            ? 'green'
                            : booking.status === 'TENTATIVE'
                              ? 'amber'
                              : 'red'
                        }
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
