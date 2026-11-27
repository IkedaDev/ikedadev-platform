import {
  Activity,
  ArrowRight,
  CalendarClock,
  DollarSign,
  FileText,
  Image as ImageIcon,
  Package,
  RefreshCw,
  Settings,
  Users,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { BffApiError, bffClient } from '../api/bffClient';
import { usePermissions } from '../auth/permissions';
import type { NavSection } from '../components/Sidebar';
import { useTheme } from '../context/ThemeProvider';
import type { AuthUser, BookingStatus, DashboardSummary, EnrichedBooking } from '../types/api';
import {
  Button,
  DataTable,
  StatCard,
  StatusBadge,
  formatDateTime,
  formatMoney,
  useToast,
  type DataTableColumn,
  type StatusTone,
} from '../ui';

interface DashboardViewProps {
  user: AuthUser;
  onNavigate?: (section: NavSection) => void;
}

const BOOKING_STATUS_LABEL: Record<BookingStatus, string> = {
  PENDING: 'Pendiente',
  SCHEDULED: 'Agendada',
  IN_PROGRESS: 'En curso',
  COMPLETED: 'Completada',
  CANCELLED: 'Cancelada',
  NO_SHOW: 'No asistió',
};

function bookingStatusTone(status: BookingStatus): StatusTone {
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

export function DashboardView({ user, onNavigate }: DashboardViewProps) {
  const { settings } = useTheme();
  const { canAccessSection } = usePermissions(user);
  const { toast } = useToast();
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
      const msg = err instanceof Error ? err.message : 'No pudimos cargar el resumen general.';
      setError(msg);
      if (!(err instanceof BffApiError && err.notified)) {
        toast.error(msg);
      }
    } finally {
      setLoading(false);
    }
  }, [toast]);

  useEffect(() => {
    void load();
  }, [load]);

  const defaultCurrency = data?.kpis.currency ?? settings.locale.currency ?? 'CLP';

  const columns = useMemo<DataTableColumn<EnrichedBooking>[]>(
    () => [
      {
        key: 'code',
        header: 'Código',
        sortable: true,
        sortValue: (b) => b.booking_code,
        render: (b) => <code className="text-xs font-semibold">{b.booking_code}</code>,
      },
      {
        key: 'starts_at',
        header: 'Fecha y hora',
        sortable: true,
        sortValue: (b) => b.starts_at,
        render: (b) => (
          <div className="stack-xs">
            <span className="font-semibold">{formatDateTime(b.starts_at)}</span>
            <span className="text-xs text-muted">
              {b.total_duration_minutes} min · hasta {formatDateTime(b.ends_at)}
            </span>
          </div>
        ),
      },
      {
        key: 'client',
        header: 'Cliente',
        sortable: true,
        sortValue: (b) => b.client?.full_name ?? '',
        render: (b) => (
          <div className="stack-xs">
            <span className="font-medium">{b.client?.full_name ?? 'Sin cliente'}</span>
            {b.client?.email && <span className="text-xs text-muted">{b.client.email}</span>}
          </div>
        ),
      },
      {
        key: 'items',
        header: 'Servicios',
        hideOnMobile: true,
        render: (b) => (
          <div className="chip-list">
            {b.items.map((item) => (
              <StatusBadge
                key={item.id}
                label={`${item.name_snapshot}${item.quantity > 1 ? ` ×${item.quantity}` : ''}`}
                tone="gray"
              />
            ))}
          </div>
        ),
      },
      {
        key: 'calendar',
        header: 'Calendario',
        hideOnMobile: true,
        render: (b) =>
          b.calendar_name ?? (b.calendar_id ? b.calendar_id.slice(0, 8) : '—'),
      },
      {
        key: 'total',
        header: 'Total',
        align: 'right',
        sortable: true,
        sortValue: (b) => b.total_price,
        render: (b) => (
          <span className="font-semibold">{formatMoney(b.total_price, b.currency)}</span>
        ),
      },
      {
        key: 'status',
        header: 'Estado',
        sortable: true,
        sortValue: (b) => b.status,
        render: (b) => (
          <StatusBadge
            label={BOOKING_STATUS_LABEL[b.status] ?? b.status}
            tone={bookingStatusTone(b.status)}
          />
        ),
      },
    ],
    [],
  );

  const showCrm =
    canAccessSection('clients', settings.features) && data?.modules?.crm !== false;
  const showCatalog =
    canAccessSection('catalog', settings.features) && data?.modules?.catalog !== false;
  const showBookings =
    canAccessSection('bookings', settings.features) && data?.modules?.bookings !== false;
  const hasAnyKpi = showCrm || showCatalog || showBookings;

  return (
    <div className="stack-lg">
      <div className="page-header">
        <div>
          <h1 className="page-title">Resumen general</h1>
          <p className="page-subtitle">
            Panorama actualizado de la operación diaria de tu negocio.
          </p>
        </div>
        <div className="page-actions">
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

      {hasAnyKpi && (
        <div className="kpi-grid">
          {showCrm && (
            <StatCard
              label="Clientes registrados"
              value={data?.kpis.totalClients ?? 0}
              icon={<Users size={18} />}
              helper="Activos en tu cartera"
              isLoading={loading}
            />
          )}
          {showCatalog && (
            <StatCard
              label="Catálogo activo"
              value={data?.kpis.activeCatalogItems ?? 0}
              icon={<Package size={18} />}
              helper="Servicios, productos y paquetes"
              isLoading={loading}
            />
          )}
          {showBookings && (
            <>
              <StatCard
                label="Próximas reservas"
                value={data?.kpis.upcomingBookingsCount ?? 0}
                icon={<CalendarClock size={18} />}
                helper="En los próximos 30 días"
                isLoading={loading}
              />
              <StatCard
                label="Ingresos agendados"
                value={formatMoney(data?.kpis.upcomingRevenue ?? 0, defaultCurrency)}
                icon={<DollarSign size={18} />}
                helper="Estimado de reservas activas"
                isLoading={loading}
              />
            </>
          )}
        </div>
      )}

      {onNavigate && (
        <div className="card card-compact">
          <div className="row-between row-wrap">
            <div>
              <h2 className="card-title">Acciones rápidas</h2>
              <p className="card-subtitle">Accede directamente a las herramientas de tu negocio.</p>
            </div>
            <div className="row-wrap">
              {canAccessSection('bookings', settings.features) && (
                <Button
                  variant="primary"
                  size="sm"
                  icon={<CalendarClock size={14} />}
                  onClick={() => onNavigate('bookings')}
                >
                  Agenda y Reservas
                </Button>
              )}
              {canAccessSection('clients', settings.features) && (
                <Button
                  variant="secondary"
                  size="sm"
                  icon={<Users size={14} />}
                  onClick={() => onNavigate('clients')}
                >
                  Clientes
                </Button>
              )}
              {canAccessSection('catalog', settings.features) && (
                <Button
                  variant="secondary"
                  size="sm"
                  icon={<Package size={14} />}
                  onClick={() => onNavigate('catalog')}
                >
                  Catálogo
                </Button>
              )}
              {canAccessSection('content', settings.features) && (
                <Button
                  variant="secondary"
                  size="sm"
                  icon={<FileText size={14} />}
                  onClick={() => onNavigate('content')}
                >
                  Contenido / Blog
                </Button>
              )}
              {canAccessSection('media', settings.features) && (
                <Button
                  variant="secondary"
                  size="sm"
                  icon={<ImageIcon size={14} />}
                  onClick={() => onNavigate('media')}
                >
                  Medios
                </Button>
              )}
              {canAccessSection('telemetry', settings.features) && (
                <Button
                  variant="secondary"
                  size="sm"
                  icon={<Activity size={14} />}
                  onClick={() => onNavigate('telemetry')}
                >
                  Actividad
                </Button>
              )}
              {canAccessSection('settings', settings.features) && (
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Settings size={14} />}
                  onClick={() => onNavigate('settings')}
                >
                  Ajustes
                </Button>
              )}
            </div>
          </div>
        </div>
      )}

      {showBookings && (
        <div className="stack-sm">
          <div className="row-between row-wrap">
            <h2 className="card-title">Próximas reservas</h2>
            <StatusBadge label="Próximos 30 días" tone="blue" />
          </div>

          <DataTable<EnrichedBooking>
            columns={columns}
            data={data?.upcomingBookings ?? []}
            rowKey={(row) => row.id}
            isLoading={loading}
            skeletonRows={4}
            emptyTitle="Sin reservas próximas"
            emptyDescription="Aún no hay citas agendadas para los próximos 30 días."
            emptyIcon={<CalendarClock size={22} />}
            emptyAction={
              onNavigate && canAccessSection('bookings', settings.features) ? (
                <Button
                  variant="primary"
                  size="sm"
                  icon={<ArrowRight size={14} />}
                  onClick={() => onNavigate('bookings')}
                >
                  Ir a Agenda y Reservas
                </Button>
              ) : undefined
            }
          />
        </div>
      )}
    </div>
  );
}
