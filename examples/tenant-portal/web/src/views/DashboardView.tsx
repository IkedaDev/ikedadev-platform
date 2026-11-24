import { CalendarClock, DollarSign, Package, RefreshCw, Users } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { bffClient } from '../api/bffClient';
import { StatusBadge, formatDateTime, formatMoney } from '../components/StatusBadge';
import type { BookingStatus, DashboardSummary } from '../types/api';

function bookingStatusTone(status: BookingStatus): 'green' | 'amber' | 'red' | 'blue' | 'gray' {
  switch (status) {
    case 'COMPLETED':
      return 'green';
    case 'SCHEDULED':
    case 'IN_PROGRESS':
      return 'blue';
    case 'PENDING':
      return 'amber';
    case 'CANCELLED':
    case 'NO_SHOW':
      return 'red';
  }
}

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
            Resumen consolidado en paralelo desde crm-api, catalog-api, events-api y bookings-api
            vía BFF.
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
          <div className="kpi-hint">Total calculado por bookings-api (reservas activas)</div>
        </div>
      </div>

      <div className="card">
        <div className="section-header">
          <h2 className="section-title">Próximas Reservas</h2>
          <StatusBadge label="Orquestación BFF (Bookings + Events + CRM + Catalog)" tone="blue" />
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
                  <th>Código</th>
                  <th>Fecha / Hora</th>
                  <th>Cliente</th>
                  <th>Ítems Congelados</th>
                  <th>Calendario</th>
                  <th>Total</th>
                  <th>Estado</th>
                </tr>
              </thead>
              <tbody>
                {data.upcomingBookings.map((booking) => (
                  <tr key={booking.id}>
                    <td>
                      <code style={{ fontSize: 12, fontWeight: 600 }}>{booking.booking_code}</code>
                    </td>
                    <td>
                      <div style={{ fontWeight: 600 }}>{formatDateTime(booking.starts_at)}</div>
                      <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                        {booking.total_duration_minutes} min · hasta{' '}
                        {formatDateTime(booking.ends_at)}
                      </div>
                    </td>
                    <td>
                      <div style={{ fontWeight: 500 }}>
                        {booking.client?.full_name ?? 'Sin cliente'}
                      </div>
                      {booking.client?.email && (
                        <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                          {booking.client.email}
                        </div>
                      )}
                    </td>
                    <td>
                      <div className="btn-row">
                        {booking.items.map((item) => (
                          <StatusBadge
                            key={item.id}
                            label={`${item.name_snapshot}${item.quantity > 1 ? ` ×${item.quantity}` : ''}`}
                            tone="gray"
                          />
                        ))}
                      </div>
                    </td>
                    <td>
                      {booking.calendar_name ??
                        (booking.calendar_id ? booking.calendar_id.slice(0, 8) : '—')}
                    </td>
                    <td style={{ fontWeight: 600 }}>
                      {formatMoney(booking.total_price, booking.currency)}
                    </td>
                    <td>
                      <StatusBadge label={booking.status} tone={bookingStatusTone(booking.status)} />
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
