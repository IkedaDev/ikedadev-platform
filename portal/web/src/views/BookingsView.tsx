import {
  AlertTriangle,
  CalendarClock,
  CalendarPlus,
  Check,
  Clock,
  Plus,
  RefreshCw,
  Trash2,
  UserX,
  XCircle,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { BffApiError, bffClient } from '../api/bffClient';
import { usePermissions } from '../auth/permissions';
import { useTheme } from '../context/ThemeProvider';
import type {
  AuthUser,
  AvailabilityResult,
  BookingStatus,
  Calendar,
  CatalogItem,
  Client,
  EnrichedBooking,
} from '../types/api';
import {
  Button,
  DataTable,
  Input,
  Select,
  Skeleton,
  StatusBadge,
  formatDateTime,
  formatMoney,
  useDrawer,
  useModal,
  useToast,
  type DataTableColumn,
  type StatusTone,
} from '../ui';

type StatusFilter = 'ALL' | BookingStatus;
type DateQuickFilter = 'TODAY' | 'NEXT_7_DAYS' | 'ALL';

const BOOKING_STATUS_LABEL: Record<BookingStatus, string> = {
  PENDING: 'Pendiente',
  SCHEDULED: 'Agendada',
  IN_PROGRESS: 'En curso',
  COMPLETED: 'Completada',
  CANCELLED: 'Cancelada',
  NO_SHOW: 'No asistió',
};

const ITEM_TYPE_SHORT: Record<string, string> = {
  SERVICE: 'Servicio',
  PRODUCT: 'Producto',
  BUNDLE: 'Paquete',
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

function isTerminalStatus(status: BookingStatus): boolean {
  return status === 'COMPLETED' || status === 'CANCELLED' || status === 'NO_SHOW';
}

function toDateInputValue(date: Date): string {
  const yyyy = date.getFullYear();
  const mm = String(date.getMonth() + 1).padStart(2, '0');
  const dd = String(date.getDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}

function toLocalDateTimeInputValue(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  const hh = String(d.getHours()).padStart(2, '0');
  const min = String(d.getMinutes()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}T${hh}:${min}`;
}

function formatTimeOnly(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return new Intl.DateTimeFormat('es-CL', { timeStyle: 'short' }).format(d);
}

interface BookingDrawerFormProps {
  calendars: Calendar[];
  clients: Client[];
  catalogItems: CatalogItem[];
  defaultCalendarId: string;
  defaultStartsAtLocal: string;
  defaultCurrency: string;
  onDirtyChange: (dirty: boolean) => void;
  onSaved: () => void;
  onCancel: () => void;
}

function BookingDrawerForm({
  calendars,
  clients,
  catalogItems,
  defaultCalendarId,
  defaultStartsAtLocal,
  defaultCurrency,
  onDirtyChange,
  onSaved,
  onCancel,
}: BookingDrawerFormProps) {
  const { toast } = useToast();
  const [calendarId, setCalendarId] = useState(defaultCalendarId || calendars[0]?.id || '');
  const [clientId, setClientId] = useState(clients[0]?.id ?? '');
  const [selectedItemIds, setSelectedItemIds] = useState<string[]>([]);
  const [startsAtLocal, setStartsAtLocal] = useState(defaultStartsAtLocal);
  const [notes, setNotes] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [conflictAlert, setConflictAlert] = useState<string | null>(null);

  const toggleItem = (id: string) => {
    setSelectedItemIds((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
    onDirtyChange(true);
  };

  const previewSummary = useMemo(() => {
    const selected = catalogItems.filter((i) => selectedItemIds.includes(i.id));
    const totalPrice = selected.reduce((sum, i) => sum + i.base_price, 0);
    const totalDuration = selected.reduce(
      (sum, i) => sum + (i.item_type === 'SERVICE' ? (i.duration_minutes ?? 0) : 0),
      0,
    );
    const currency = selected[0]?.currency ?? defaultCurrency ?? 'CLP';
    return { count: selected.length, totalPrice, totalDuration: totalDuration || 30, currency };
  }, [catalogItems, selectedItemIds, defaultCurrency]);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setFormError(null);
    setConflictAlert(null);

    if (!calendarId || !clientId || selectedItemIds.length === 0 || !startsAtLocal) {
      setFormError('Selecciona calendario, cliente, al menos un servicio y la fecha/hora.');
      return;
    }

    setSubmitting(true);
    try {
      const created = await bffClient.createBooking({
        calendarId,
        clientId,
        catalogItemIds: selectedItemIds,
        startsAt: new Date(startsAtLocal).toISOString(),
        notes: notes.trim() || undefined,
      });

      toast.success(
        `Reserva ${created.booking_code} creada (${formatMoney(created.total_price, created.currency)}).`,
      );
      onDirtyChange(false);
      onSaved();
    } catch (err) {
      if (err instanceof BffApiError && err.status === 409) {
        setConflictAlert('Horario no disponible: ya existe una reserva en ese horario.');
      } else {
        const msg = err instanceof Error ? err.message : 'No pudimos crear la reserva.';
        setFormError(msg);
        if (!(err instanceof BffApiError && err.notified)) {
          toast.error(msg);
        }
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form className="form-grid" onSubmit={handleSubmit}>
      {formError && (
        <div className="alert-banner alert-banner-danger" role="alert">
          <div className="alert-banner-body">{formError}</div>
        </div>
      )}

      {conflictAlert && (
        <div className="alert-banner alert-banner-warning" role="alert">
          <AlertTriangle size={17} />
          <div className="alert-banner-body stack-xs">
            <strong>Horario no disponible</strong>
            <span>{conflictAlert}</span>
          </div>
        </div>
      )}

      <Select
        label="Calendario"
        required
        value={calendarId}
        onChange={(e) => {
          setCalendarId(e.target.value);
          onDirtyChange(true);
        }}
      >
        <option value="">Selecciona un calendario...</option>
        {calendars.map((cal) => (
          <option key={cal.id} value={cal.id}>
            {cal.name}
          </option>
        ))}
      </Select>

      <Select
        label="Cliente"
        required
        value={clientId}
        onChange={(e) => {
          setClientId(e.target.value);
          onDirtyChange(true);
        }}
      >
        <option value="">Selecciona un cliente...</option>
        {clients.map((c) => (
          <option key={c.id} value={c.id}>
            {c.full_name} {c.tax_id ? `(${c.tax_id})` : ''}
          </option>
        ))}
      </Select>

      <div className="field">
        <span className="field-label">
          Servicios o productos <span className="field-required">*</span>
        </span>
        <div className="card card-compact stack-xs">
          {catalogItems.length === 0 ? (
            <span className="text-sm text-muted">No hay servicios activos en el catálogo.</span>
          ) : (
            catalogItems.map((item) => (
              <label key={item.id} className="row-between checkbox-row">
                <span className="row-sm">
                  <input
                    type="checkbox"
                    checked={selectedItemIds.includes(item.id)}
                    onChange={() => toggleItem(item.id)}
                  />
                  <span>
                    <strong>{item.name}</strong>{' '}
                    <span className="text-xs text-muted">
                      ({ITEM_TYPE_SHORT[item.item_type] ?? item.item_type}
                      {item.item_type === 'SERVICE' ? ` · ${item.duration_minutes} min` : ''})
                    </span>
                  </span>
                </span>
                <span className="text-xs font-semibold">
                  {formatMoney(item.base_price, item.currency)}
                </span>
              </label>
            ))
          )}
        </div>
      </div>

      <Input
        label="Fecha y hora de inicio"
        required
        type="datetime-local"
        value={startsAtLocal}
        onChange={(e) => {
          setStartsAtLocal(e.target.value);
          onDirtyChange(true);
        }}
      />

      <Input
        label="Notas"
        value={notes}
        onChange={(e) => {
          setNotes(e.target.value);
          onDirtyChange(true);
        }}
        placeholder="Ej: Cliente solicita confirmación previa"
      />

      {previewSummary.count > 0 && (
        <div className="alert-banner alert-banner-info">
          <div className="alert-banner-body row-between">
            <span>
              <strong>{previewSummary.count} servicio(s)</strong> · Duración estimada:{' '}
              <strong>{previewSummary.totalDuration} min</strong>
            </span>
            <strong className="font-bold">
              {formatMoney(previewSummary.totalPrice, previewSummary.currency)}
            </strong>
          </div>
        </div>
      )}

      <div className="row-end">
        <Button variant="secondary" type="button" onClick={onCancel} disabled={submitting}>
          Cancelar
        </Button>
        <Button
          variant="primary"
          type="submit"
          isLoading={submitting}
          disabled={selectedItemIds.length === 0 || !calendarId}
        >
          Confirmar reserva
        </Button>
      </div>
    </form>
  );
}

interface CalendarDrawerFormProps {
  defaultTimezone: string;
  onDirtyChange: (dirty: boolean) => void;
  onSaved: (created: Calendar) => void;
  onCancel: () => void;
}

function CalendarDrawerForm({
  defaultTimezone,
  onDirtyChange,
  onSaved,
  onCancel,
}: CalendarDrawerFormProps) {
  const { toast } = useToast();
  const [name, setName] = useState('');
  const [timezone, setTimezone] = useState(defaultTimezone || 'America/Santiago');
  const [preventOverlaps, setPreventOverlaps] = useState(true);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setFormError(null);
    if (!name.trim()) {
      setFormError('El nombre del calendario es obligatorio.');
      return;
    }

    setSaving(true);
    try {
      const created = await bffClient.createCalendar({
        name: name.trim(),
        timezone: timezone.trim() || 'America/Santiago',
        allow_overlaps: !preventOverlaps,
      });
      toast.success(`Calendario "${created.name}" creado.`);
      onDirtyChange(false);
      onSaved(created);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Error creando calendario';
      setFormError(msg);
      if (!(err instanceof BffApiError && err.notified)) {
        toast.error(msg);
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <form className="form-grid" onSubmit={handleSubmit}>
      {formError && (
        <div className="alert-banner alert-banner-danger" role="alert">
          <div className="alert-banner-body">{formError}</div>
        </div>
      )}

      <Input
        label="Nombre"
        required
        value={name}
        onChange={(e) => {
          setName(e.target.value);
          onDirtyChange(true);
        }}
        placeholder="Ej: Box Atención 1 o Sala Principal"
      />

      <Input
        label="Zona horaria"
        value={timezone}
        onChange={(e) => {
          setTimezone(e.target.value);
          onDirtyChange(true);
        }}
        placeholder="America/Santiago"
      />

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={preventOverlaps}
          onChange={(e) => {
            setPreventOverlaps(e.target.checked);
            onDirtyChange(true);
          }}
        />
        <span>Evitar reservas simultáneas en el mismo horario</span>
      </label>

      <div className="row-end">
        <Button variant="secondary" type="button" onClick={onCancel} disabled={saving}>
          Cancelar
        </Button>
        <Button variant="primary" type="submit" isLoading={saving}>
          Crear calendario
        </Button>
      </div>
    </form>
  );
}

export interface BookingsViewProps {
  user: AuthUser;
}

export function BookingsView({ user }: BookingsViewProps) {
  const { settings } = useTheme();
  const { can } = usePermissions(user);
  const canWriteBookings = can('bookings:write');
  const canWriteEvents = can('events:write');
  const canCreateCalendar = settings.features.events !== false && canWriteEvents;
  const canCreateBooking = canWriteBookings && canWriteEvents;
  const { openDrawer, closeDrawer } = useDrawer();
  const { confirm } = useModal();
  const { toast } = useToast();

  const [calendars, setCalendars] = useState<Calendar[]>([]);
  const [selectedCalendarId, setSelectedCalendarId] = useState<string>('');
  const [clients, setClients] = useState<Client[]>([]);
  const [catalogItems, setCatalogItems] = useState<CatalogItem[]>([]);
  const [bookings, setBookings] = useState<EnrichedBooking[]>([]);
  const [availability, setAvailability] = useState<AvailabilityResult | null>(null);

  const [availDate, setAvailDate] = useState<string>(() => {
    const tomorrow = new Date();
    tomorrow.setDate(tomorrow.getDate() + 1);
    return toDateInputValue(tomorrow);
  });
  const [slotDuration, setSlotDuration] = useState<number>(30);
  const [dateFilter, setDateFilter] = useState<DateQuickFilter>('ALL');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('ALL');
  const [searchTerm, setSearchTerm] = useState('');

  const [loading, setLoading] = useState(true);
  const [availLoading, setAvailLoading] = useState(false);
  const [busyBookingId, setBusyBookingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const dirtyRef = useRef(false);

  const loadBaseData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const now = new Date();
      const rangeStart = new Date(now.getTime() - 24 * 60 * 60 * 1000).toISOString();
      const rangeEnd = new Date(now.getTime() + 30 * 24 * 60 * 60 * 1000).toISOString();

      const [cals, clientsPage, itemsPage, bookingsList] = await Promise.all([
        bffClient.listCalendars(),
        bffClient.searchClients({ pagination: { page: 1, limit: 100 } }),
        bffClient.searchItems({
          filter: { and: [{ field: 'is_active', operator: 'eq', value: true }] },
          pagination: { page: 1, limit: 100 },
        }),
        bffClient.listBookings({ start: rangeStart, end: rangeEnd }),
      ]);

      setCalendars(cals);
      setClients(clientsPage.data);
      setCatalogItems(itemsPage.data);
      setBookings(bookingsList);
      setSelectedCalendarId((prev) => prev || cals[0]?.id || '');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'No pudimos cargar la agenda.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadBaseData();
  }, [loadBaseData]);

  const loadAvailability = useCallback(async () => {
    if (!selectedCalendarId || !availDate) return;
    setAvailLoading(true);
    try {
      const start = new Date(`${availDate}T08:00:00`);
      const end = new Date(`${availDate}T20:00:00`);
      const res = await bffClient.getAvailability(selectedCalendarId, {
        starts_at: start.toISOString(),
        ends_at: end.toISOString(),
        slot_duration_minutes: slotDuration,
      });
      setAvailability(res);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error consultando disponibilidad');
    } finally {
      setAvailLoading(false);
    }
  }, [selectedCalendarId, availDate, slotDuration]);

  useEffect(() => {
    void loadAvailability();
  }, [loadAvailability]);

  const openBookingDrawer = useCallback(
    (prefillStartsAt?: string) => {
      const defaultStart =
        prefillStartsAt ??
        (() => {
          const tomorrow = new Date();
          tomorrow.setDate(tomorrow.getDate() + 1);
          tomorrow.setHours(10, 0, 0, 0);
          return toLocalDateTimeInputValue(tomorrow.toISOString());
        })();

      dirtyRef.current = false;
      openDrawer({
        title: 'Nueva reserva',
        subtitle: 'Selecciona cliente, servicios y horario disponible.',
        size: 'md',
        hasUnsavedChanges: () => dirtyRef.current,
        content: (
          <BookingDrawerForm
            calendars={calendars}
            clients={clients}
            catalogItems={catalogItems}
            defaultCalendarId={selectedCalendarId}
            defaultStartsAtLocal={defaultStart}
            defaultCurrency={settings.locale.currency}
            onDirtyChange={(dirty) => {
              dirtyRef.current = dirty;
            }}
            onCancel={() => closeDrawer(false)}
            onSaved={() => {
              closeDrawer(true);
              void Promise.all([loadBaseData(), loadAvailability()]);
            }}
          />
        ),
      });
    },
    [
      openDrawer,
      closeDrawer,
      calendars,
      clients,
      catalogItems,
      selectedCalendarId,
      settings.locale.currency,
      loadBaseData,
      loadAvailability,
    ],
  );

  const openCalendarDrawer = useCallback(() => {
    dirtyRef.current = false;
    openDrawer({
      title: 'Nuevo calendario',
      subtitle: 'Agrega una sala, box o profesional con agenda propia.',
      size: 'sm',
      hasUnsavedChanges: () => dirtyRef.current,
      content: (
        <CalendarDrawerForm
          defaultTimezone={settings.locale.timezone}
          onDirtyChange={(dirty) => {
            dirtyRef.current = dirty;
          }}
          onCancel={() => closeDrawer(false)}
          onSaved={(created) => {
            closeDrawer(true);
            setSelectedCalendarId(created.id);
            void loadBaseData();
          }}
        />
      ),
    });
  }, [openDrawer, closeDrawer, settings.locale.timezone, loadBaseData]);

  const handleTransitionStatus = useCallback(
    async (booking: EnrichedBooking, targetStatus: BookingStatus) => {
      if (busyBookingId === booking.id) return;
      if (targetStatus === 'CANCELLED') {
        const ok = await confirm({
          title: `Cancelar reserva ${booking.booking_code}`,
          message:
            '¿Deseas cancelar esta reserva? El horario quedará disponible nuevamente para otros clientes.',
          variant: 'danger',
          confirmLabel: 'Cancelar reserva',
        });
        if (!ok) return;
      }

      setBusyBookingId(booking.id);
      try {
        const updated = await bffClient.transitionBookingStatus(booking.id, {
          status: targetStatus,
          cancellationReason:
            targetStatus === 'CANCELLED' ? 'Cancelada desde el portal' : undefined,
        });
        toast.success(
          `Reserva ${updated.booking_code} marcada como ${BOOKING_STATUS_LABEL[updated.status] ?? updated.status}.`,
        );
        await Promise.all([loadBaseData(), loadAvailability()]);
      } catch (err) {
        if (!(err instanceof BffApiError && err.notified)) {
          toast.error(
            err instanceof Error ? err.message : 'Error actualizando estado de la reserva',
          );
        }
      } finally {
        setBusyBookingId((prev) => (prev === booking.id ? null : prev));
      }
    },
    [busyBookingId, confirm, toast, loadBaseData, loadAvailability],
  );

  const handleDeleteBooking = useCallback(
    async (booking: EnrichedBooking) => {
      if (busyBookingId === booking.id) return;
      const ok = await confirm({
        title: `Eliminar reserva ${booking.booking_code}`,
        message: '¿Seguro que deseas eliminar esta reserva del listado?',
        variant: 'danger',
        confirmLabel: 'Eliminar',
      });
      if (!ok) return;

      setBusyBookingId(booking.id);
      try {
        await bffClient.deleteBooking(booking.id);
        toast.success(`Reserva ${booking.booking_code} eliminada.`);
        await Promise.all([loadBaseData(), loadAvailability()]);
      } catch (err) {
        if (!(err instanceof BffApiError && err.notified)) {
          toast.error(err instanceof Error ? err.message : 'Error eliminando reserva');
        }
      } finally {
        setBusyBookingId((prev) => (prev === booking.id ? null : prev));
      }
    },
    [busyBookingId, confirm, toast, loadBaseData, loadAvailability],
  );

  const filteredBookings = useMemo(() => {
    const q = searchTerm.trim().toLowerCase();
    const now = new Date();
    const todayKey = toDateInputValue(now);
    const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const sevenDaysEnd = new Date(now.getTime() + 7 * 24 * 60 * 60 * 1000);

    return bookings.filter((b) => {
      if (statusFilter !== 'ALL' && b.status !== statusFilter) return false;

      if (dateFilter === 'TODAY') {
        const bDate = toDateInputValue(new Date(b.starts_at));
        if (bDate !== todayKey) return false;
      } else if (dateFilter === 'NEXT_7_DAYS') {
        const d = new Date(b.starts_at);
        if (d < todayStart || d > sevenDaysEnd) return false;
      }

      if (!q) return true;
      return (
        b.booking_code.toLowerCase().includes(q) ||
        (b.client?.full_name.toLowerCase().includes(q) ?? false) ||
        (b.calendar_name?.toLowerCase().includes(q) ?? false)
      );
    });
  }, [bookings, statusFilter, dateFilter, searchTerm]);

  const selectedCalendar = calendars.find((c) => c.id === selectedCalendarId);

  const columns = useMemo<DataTableColumn<EnrichedBooking>[]>(
    () => [
      {
        key: 'code',
        header: 'Código y estado',
        sortable: true,
        sortValue: (b) => b.booking_code,
        render: (b) => (
          <div className="stack-xs">
            <code className="text-xs font-bold">{b.booking_code}</code>
            <div>
              <StatusBadge
                label={BOOKING_STATUS_LABEL[b.status] ?? b.status}
                tone={bookingStatusTone(b.status)}
              />
            </div>
          </div>
        ),
      },
      {
        key: 'starts_at',
        header: 'Horario',
        sortable: true,
        sortValue: (b) => b.starts_at,
        render: (b) => (
          <div className="stack-xs">
            <span className="font-semibold">{formatDateTime(b.starts_at)}</span>
            <span className="text-xs text-muted">
              hasta {formatTimeOnly(b.ends_at)} ({b.total_duration_minutes} min)
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
            {b.calendar_name && <span className="text-xs text-muted">{b.calendar_name}</span>}
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
                label={`${item.name_snapshot} ×${item.quantity} (${formatMoney(item.line_total, b.currency)})`}
                tone="gray"
              />
            ))}
          </div>
        ),
      },
      {
        key: 'total',
        header: 'Total',
        align: 'right',
        sortable: true,
        sortValue: (b) => b.total_price,
        render: (b) => (
          <div className="stack-xs">
            <span className="font-bold">{formatMoney(b.total_price, b.currency)}</span>
            <span className="text-xs text-muted">
              Neto: {formatMoney(b.subtotal, b.currency)} · Imp:{' '}
              {formatMoney(b.tax_total, b.currency)}
            </span>
          </div>
        ),
      },
      {
        key: 'notes',
        header: 'Notas',
        hideOnMobile: true,
        render: (b) => (
          <span className="text-xs text-muted cell-truncate">
            {b.cancellation_reason
              ? `Cancelada: ${b.cancellation_reason}`
              : (b.notes ?? '—')}
          </span>
        ),
      },
      ...(canWriteBookings
        ? [
            {
              key: 'actions',
              header: 'Acciones',
              align: 'right' as const,
              render: (b: EnrichedBooking) => {
                const isBusy = busyBookingId === b.id;
                return (
                  <div className="row-end row-wrap">
                    {!isTerminalStatus(b.status) && (
                      <>
                        <Button
                          variant="secondary"
                          size="sm"
                          icon={<Check size={13} />}
                          onClick={() => handleTransitionStatus(b, 'COMPLETED')}
                          disabled={isBusy}
                          title="Marcar como completada"
                        >
                          Completar
                        </Button>
                        <Button
                          variant="secondary"
                          size="sm"
                          icon={<UserX size={13} />}
                          onClick={() => handleTransitionStatus(b, 'NO_SHOW')}
                          disabled={isBusy}
                          title="Marcar que no asistió"
                        >
                          No asistió
                        </Button>
                        <Button
                          variant="danger"
                          size="sm"
                          icon={<XCircle size={13} />}
                          onClick={() => handleTransitionStatus(b, 'CANCELLED')}
                          disabled={isBusy}
                          title="Cancelar reserva"
                        >
                          Cancelar
                        </Button>
                      </>
                    )}
                    <Button
                      variant="ghost"
                      size="sm"
                      icon={<Trash2 size={13} />}
                      onClick={() => handleDeleteBooking(b)}
                      disabled={isBusy}
                      isLoading={isBusy}
                      aria-label={`Eliminar reserva ${b.booking_code}`}
                      title="Eliminar reserva"
                    />
                  </div>
                );
              },
            },
          ]
        : []),
    ],
    [busyBookingId, canWriteBookings, handleTransitionStatus, handleDeleteBooking],
  );

  return (
    <div className="stack-lg">
      <div className="page-header">
        <div>
          <h1 className="page-title">Agenda y Reservas</h1>
          <p className="page-subtitle">
            Consulta horarios disponibles, agenda citas y gestiona el estado de cada atención.
          </p>
        </div>
        <div className="page-actions">
          {canCreateCalendar && (
            <Button
              variant="secondary"
              icon={<CalendarPlus size={15} />}
              onClick={openCalendarDrawer}
            >
              Nuevo calendario
            </Button>
          )}
          <Button
            variant="secondary"
            icon={<RefreshCw size={15} className={loading ? 'btn-spinner' : ''} />}
            onClick={loadBaseData}
            disabled={loading}
          >
            Actualizar
          </Button>
          {canCreateBooking && (
            <Button
              variant="primary"
              icon={<Plus size={15} />}
              onClick={() => openBookingDrawer()}
            >
              Nueva reserva
            </Button>
          )}
        </div>
      </div>

      {error && (
        <div className="alert-banner alert-banner-danger" role="alert">
          <div className="alert-banner-body">{error}</div>
        </div>
      )}

      {/* Availability card */}
      <div className="card">
        <div className="card-header">
          <div>
            <h2 className="card-title">Disponibilidad del calendario</h2>
            <p className="card-subtitle">
              Haz clic en un bloque libre para agendar directamente en ese horario.
            </p>
          </div>
          {selectedCalendar && (
            <StatusBadge
              label={
                selectedCalendar.allow_overlaps
                  ? 'Permite reservas simultáneas'
                  : 'Sin reservas simultáneas'
              }
              tone={selectedCalendar.allow_overlaps ? 'amber' : 'blue'}
            />
          )}
        </div>

        <div className="grid-3">
          <Select
            label="Calendario"
            value={selectedCalendarId}
            onChange={(e) => setSelectedCalendarId(e.target.value)}
          >
            {calendars.length === 0 && <option value="">Sin calendarios disponibles</option>}
            {calendars.map((cal) => (
              <option key={cal.id} value={cal.id}>
                {cal.name}
              </option>
            ))}
          </Select>

          <Input
            label="Fecha a consultar"
            type="date"
            value={availDate}
            onChange={(e) => setAvailDate(e.target.value)}
          />

          <Select
            label="Duración de bloque"
            value={String(slotDuration)}
            onChange={(e) => setSlotDuration(Number(e.target.value))}
          >
            <option value="15">15 minutos</option>
            <option value="30">30 minutos</option>
            <option value="45">45 minutos</option>
            <option value="60">60 minutos</option>
          </Select>
        </div>

        {availLoading ? (
          <div className="stack-sm">
            <Skeleton variant="line" />
            <Skeleton variant="line-sm" />
          </div>
        ) : !availability ? (
          <p className="text-sm text-muted">
            Selecciona un calendario para consultar disponibilidad.
          </p>
        ) : (
          <div className="stack-md">
            <div className="stack-xs">
              <span className="text-xs font-semibold text-muted">
                Horarios disponibles ({availability.available_slots.length})
              </span>
              {availability.available_slots.length === 0 ? (
                <span className="text-sm text-muted">No hay bloques libres en esta fecha.</span>
              ) : (
                <div className="chip-list">
                  {availability.available_slots.slice(0, 24).map((slot, idx) => {
                    const localVal = toLocalDateTimeInputValue(slot.starts_at);
                    return (
                      <button
                        key={`${slot.starts_at}_${slot.ends_at}_${idx}`}
                        type="button"
                        className="pill-btn row-sm"
                        disabled={!canCreateBooking}
                        onClick={() => {
                          if (canCreateBooking) openBookingDrawer(localVal);
                        }}
                      >
                        <Clock size={12} />
                        <span>
                          {formatTimeOnly(slot.starts_at)} - {formatTimeOnly(slot.ends_at)}
                        </span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>

            {availability.busy_slots.length > 0 && (
              <div className="stack-xs">
                <span className="text-xs font-semibold text-muted">
                  Horarios ocupados ({availability.busy_slots.length})
                </span>
                <div className="chip-list">
                  {availability.busy_slots.map((slot, idx) => (
                    <StatusBadge
                      key={`${slot.starts_at}_${slot.ends_at}_${idx}`}
                      label={`${formatTimeOnly(slot.starts_at)} - ${formatTimeOnly(slot.ends_at)}`}
                      tone="red"
                    />
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Bookings table */}
      <div className="stack-sm">
        <div className="row-between row-wrap">
          <h2 className="card-title">Reservas ({filteredBookings.length})</h2>
          <div className="tabs">
            {(
              [
                { id: 'TODAY', label: 'Hoy' },
                { id: 'NEXT_7_DAYS', label: 'Próximos 7 días' },
                { id: 'ALL', label: 'Todas' },
              ] as Array<{ id: DateQuickFilter; label: string }>
            ).map((opt) => (
              <button
                key={opt.id}
                type="button"
                className={`tab-btn ${dateFilter === opt.id ? 'active' : ''}`}
                onClick={() => setDateFilter(opt.id)}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </div>

        <DataTable<EnrichedBooking>
          columns={columns}
          data={filteredBookings}
          rowKey={(b) => b.id}
          isLoading={loading}
          searchValue={searchTerm}
          onSearchChange={setSearchTerm}
          searchPlaceholder="Buscar por código, cliente o calendario..."
          toolbar={
            <div className="filter-pills">
              {(
                ['ALL', 'SCHEDULED', 'PENDING', 'COMPLETED', 'CANCELLED', 'NO_SHOW'] as StatusFilter[]
              ).map((st) => (
                <button
                  key={st}
                  type="button"
                  className={`pill-btn ${statusFilter === st ? 'active' : ''}`}
                  onClick={() => setStatusFilter(st)}
                >
                  {st === 'ALL' ? 'Todos los estados' : BOOKING_STATUS_LABEL[st]}
                </button>
              ))}
            </div>
          }
          emptyTitle="Sin reservas registradas"
          emptyDescription="No hay reservas para el rango o filtro seleccionado."
          emptyIcon={<CalendarClock size={22} />}
          emptyAction={
            canCreateBooking ? (
              <Button
                variant="primary"
                size="sm"
                icon={<Plus size={14} />}
                onClick={() => openBookingDrawer()}
              >
                Nueva reserva
              </Button>
            ) : undefined
          }
        />
      </div>
    </div>
  );
}
