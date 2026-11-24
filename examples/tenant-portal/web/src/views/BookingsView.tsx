import { AlertTriangle, CalendarPlus, CheckCircle2, Clock, RefreshCw, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { BffApiError, bffClient } from '../api/bffClient';
import { Modal } from '../components/Modal';
import { StatusBadge, formatDateTime, formatMoney } from '../components/StatusBadge';
import type {
  AvailabilityResult,
  Calendar,
  CatalogItem,
  Client,
  EnrichedBooking,
} from '../types/api';

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

export function BookingsView() {
  const [calendars, setCalendars] = useState<Calendar[]>([]);
  const [selectedCalendarId, setSelectedCalendarId] = useState<string>('');
  const [clients, setClients] = useState<Client[]>([]);
  const [catalogItems, setCatalogItems] = useState<CatalogItem[]>([]);
  const [bookings, setBookings] = useState<EnrichedBooking[]>([]);
  const [availability, setAvailability] = useState<AvailabilityResult | null>(null);

  // Availability query date
  const [availDate, setAvailDate] = useState<string>(() => {
    const tomorrow = new Date();
    tomorrow.setDate(tomorrow.getDate() + 1);
    return toDateInputValue(tomorrow);
  });
  const [slotDuration, setSlotDuration] = useState<number>(30);

  // New booking form state
  const [clientId, setClientId] = useState('');
  const [selectedItemIds, setSelectedItemIds] = useState<string[]>([]);
  const [startsAtLocal, setStartsAtLocal] = useState<string>(() => {
    const tomorrow = new Date();
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(10, 0, 0, 0);
    return toLocalDateTimeInputValue(tomorrow.toISOString());
  });
  const [notes, setNotes] = useState('');

  // Status feedback
  const [loading, setLoading] = useState(true);
  const [availLoading, setAvailLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflictAlert, setConflictAlert] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);

  // Create calendar modal
  const [calModalOpen, setCalModalOpen] = useState(false);
  const [calName, setCalName] = useState('');
  const [calTimezone, setCalTimezone] = useState('America/Santiago');
  const [calAllowOverlaps, setCalAllowOverlaps] = useState(false);
  const [savingCal, setSavingCal] = useState(false);

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
      setClientId((prev) => prev || clientsPage.data[0]?.id || '');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error cargando agenda');
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

  const toggleItemSelection = (id: string) => {
    setSelectedItemIds((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
  };

  const previewSummary = useMemo(() => {
    const selected = catalogItems.filter((i) => selectedItemIds.includes(i.id));
    const totalPrice = selected.reduce((sum, i) => sum + i.base_price, 0);
    const totalDuration = selected.reduce(
      (sum, i) => sum + (i.item_type === 'SERVICE' ? (i.duration_minutes ?? 0) : 0),
      0,
    );
    const currency = selected[0]?.currency ?? 'CLP';
    return { count: selected.length, totalPrice, totalDuration: totalDuration || 30, currency };
  }, [catalogItems, selectedItemIds]);

  const handleCreateBooking = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setConflictAlert(null);
    setSuccessMsg(null);

    if (!selectedCalendarId || !clientId || selectedItemIds.length === 0 || !startsAtLocal) {
      setError('Selecciona calendario, cliente, al menos un servicio/ítem y fecha/hora de inicio.');
      return;
    }

    setSubmitting(true);
    try {
      const created = await bffClient.createBooking({
        calendarId: selectedCalendarId,
        clientId,
        catalogItemIds: selectedItemIds,
        startsAt: new Date(startsAtLocal).toISOString(),
        notes: notes.trim() || undefined,
      });

      setSuccessMsg(
        `Reserva creada: ${created.title} (${formatMoney(created.total_price, created.currency)})`,
      );
      setNotes('');
      await Promise.all([loadBaseData(), loadAvailability()]);
    } catch (err) {
      if (err instanceof BffApiError && err.status === 409) {
        setConflictAlert(err.message);
      } else {
        setError(err instanceof Error ? err.message : 'Error creando reserva');
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleDeleteBooking = async (booking: EnrichedBooking) => {
    if (!confirm(`¿Cancelar/eliminar la reserva "${booking.title}"?`)) return;
    try {
      await bffClient.deleteBooking(booking.id);
      await Promise.all([loadBaseData(), loadAvailability()]);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error eliminando reserva');
    }
  };

  const handleCreateCalendar = async () => {
    if (!calName.trim()) return;
    setSavingCal(true);
    try {
      const created = await bffClient.createCalendar({
        name: calName.trim(),
        timezone: calTimezone.trim() || 'America/Santiago',
        allow_overlaps: calAllowOverlaps,
      });
      setCalName('');
      setCalModalOpen(false);
      await loadBaseData();
      setSelectedCalendarId(created.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error creando calendario');
    } finally {
      setSavingCal(false);
    }
  };

  const selectedCalendar = calendars.find((c) => c.id === selectedCalendarId);

  return (
    <div>
      <div className="page-header">
        <div>
          <h1 className="page-title">Agenda y Reservas</h1>
          <p className="page-subtitle">
            Flujo orquestado extremo a extremo: Disponibilidad + Cálculo de precio/duración +
            Prevención de solapamientos (409 Conflict).
          </p>
        </div>
        <div className="btn-row">
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => setCalModalOpen(true)}
          >
            <CalendarPlus size={15} />
            Nuevo Calendario
          </button>
          <button type="button" className="btn btn-secondary" onClick={loadBaseData}>
            <RefreshCw size={15} />
            Refrescar
          </button>
        </div>
      </div>

      {error && <div className="alert alert-error">{error}</div>}

      {conflictAlert && (
        <div className="alert alert-warning">
          <AlertTriangle size={18} style={{ flexShrink: 0, marginTop: 1 }} />
          <div>
            <strong>Solapamiento detectado (HTTP 409 Conflict)</strong>
            <div style={{ marginTop: 2 }}>{conflictAlert}</div>
          </div>
        </div>
      )}

      {successMsg && (
        <div className="alert alert-success">
          <CheckCircle2 size={18} style={{ flexShrink: 0, marginTop: 1 }} />
          <div>{successMsg}</div>
        </div>
      )}

      <div className="two-col" style={{ marginBottom: 24 }}>
        {/* Left: Calendar selector & visual availability */}
        <div className="card">
          <div className="section-header">
            <h2 className="section-title">Disponibilidad del Calendario</h2>
            {selectedCalendar && (
              <StatusBadge
                label={
                  selectedCalendar.allow_overlaps
                    ? 'Permite solapamientos'
                    : 'Sin solapamientos (estricto)'
                }
                tone={selectedCalendar.allow_overlaps ? 'amber' : 'blue'}
              />
            )}
          </div>

          <div className="form-grid" style={{ marginBottom: 16 }}>
            <div className="form-field full">
              <label className="form-label">Calendario</label>
              <select
                className="select"
                value={selectedCalendarId}
                onChange={(e) => setSelectedCalendarId(e.target.value)}
              >
                {calendars.length === 0 && <option value="">Sin calendarios disponibles</option>}
                {calendars.map((cal) => (
                  <option key={cal.id} value={cal.id}>
                    {cal.name} ({cal.timezone})
                  </option>
                ))}
              </select>
            </div>
            <div className="form-field">
              <label className="form-label">Fecha a consultar</label>
              <input
                type="date"
                className="input"
                value={availDate}
                onChange={(e) => setAvailDate(e.target.value)}
              />
            </div>
            <div className="form-field">
              <label className="form-label">Bloque (min)</label>
              <select
                className="select"
                value={slotDuration}
                onChange={(e) => setSlotDuration(Number(e.target.value))}
              >
                <option value={15}>15 min</option>
                <option value={30}>30 min</option>
                <option value={45}>45 min</option>
                <option value={60}>60 min</option>
              </select>
            </div>
          </div>

          {availLoading ? (
            <div className="empty-state">Consultando bloques disponibles...</div>
          ) : !availability ? (
            <div className="empty-state">Selecciona un calendario para ver disponibilidad.</div>
          ) : (
            <div>
              <div
                style={{
                  fontSize: 12,
                  fontWeight: 600,
                  color: 'var(--text-muted)',
                  marginBottom: 8,
                }}
              >
                Bloques disponibles ({availability.available_slots.length}) — haz clic para elegir
                hora de inicio:
              </div>
              {availability.available_slots.length === 0 ? (
                <div style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 12 }}>
                  No hay bloques libres en esta fecha.
                </div>
              ) : (
                <div className="slots-grid" style={{ marginBottom: 14 }}>
                  {availability.available_slots.slice(0, 24).map((slot) => {
                    const localVal = toLocalDateTimeInputValue(slot.starts_at);
                    const isSelected = startsAtLocal === localVal;
                    return (
                      <button
                        key={slot.starts_at}
                        type="button"
                        className={`slot-chip ${isSelected ? 'selected' : ''}`}
                        onClick={() => setStartsAtLocal(localVal)}
                      >
                        <Clock size={11} style={{ marginRight: 4, verticalAlign: 'middle' }} />
                        {formatTimeOnly(slot.starts_at)} - {formatTimeOnly(slot.ends_at)}
                      </button>
                    );
                  })}
                </div>
              )}

              {availability.busy_slots.length > 0 && (
                <div>
                  <div
                    style={{
                      fontSize: 12,
                      fontWeight: 600,
                      color: 'var(--danger)',
                      marginBottom: 6,
                    }}
                  >
                    Bloques ocupados ({availability.busy_slots.length}):
                  </div>
                  <div className="slots-grid">
                    {availability.busy_slots.map((slot) => (
                      <span key={slot.starts_at} className="slot-chip busy">
                        {formatTimeOnly(slot.starts_at)} - {formatTimeOnly(slot.ends_at)}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Right: New Booking form */}
        <form className="card" onSubmit={handleCreateBooking}>
          <div className="section-header">
            <h2 className="section-title">Nueva Reserva</h2>
            <StatusBadge label="POST /api/bookings" tone="blue" />
          </div>

          <div className="form-grid">
            <div className="form-field full">
              <label className="form-label">Cliente *</label>
              <select
                className="select"
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
              >
                <option value="">Selecciona un cliente...</option>
                {clients.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.full_name} {c.tax_id ? `(${c.tax_id})` : ''}
                  </option>
                ))}
              </select>
            </div>

            <div className="form-field full">
              <label className="form-label">Servicios / Ítems del Catálogo *</label>
              <div
                style={{
                  border: '1px solid var(--border)',
                  borderRadius: 8,
                  padding: 10,
                  maxHeight: 160,
                  overflowY: 'auto',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 8,
                }}
              >
                {catalogItems.length === 0 ? (
                  <span style={{ fontSize: 13, color: 'var(--text-muted)' }}>
                    No hay ítems activos en el catálogo.
                  </span>
                ) : (
                  catalogItems.map((item) => (
                    <label key={item.id} className="checkbox-row">
                      <input
                        type="checkbox"
                        checked={selectedItemIds.includes(item.id)}
                        onChange={() => toggleItemSelection(item.id)}
                      />
                      <span style={{ flex: 1 }}>
                        <strong>{item.name}</strong>{' '}
                        <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>
                          ({item.item_type}
                          {item.item_type === 'SERVICE' ? ` · ${item.duration_minutes}m` : ''})
                        </span>
                      </span>
                      <span style={{ fontWeight: 600, fontSize: 12 }}>
                        {formatMoney(item.base_price, item.currency)}
                      </span>
                    </label>
                  ))
                )}
              </div>
            </div>

            <div className="form-field full">
              <label className="form-label">Fecha y Hora de Inicio *</label>
              <input
                type="datetime-local"
                className="input"
                value={startsAtLocal}
                onChange={(e) => setStartsAtLocal(e.target.value)}
              />
            </div>

            <div className="form-field full">
              <label className="form-label">Notas</label>
              <input
                className="input"
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                placeholder="Ej: Cliente solicita confirmación previa"
              />
            </div>

            {previewSummary.count > 0 && (
              <div className="form-field full">
                <div className="summary-box">
                  <div>
                    <strong>{previewSummary.count} ítem(s)</strong> · Duración estimada:{' '}
                    <strong>{previewSummary.totalDuration} min</strong>
                  </div>
                  <div style={{ fontWeight: 700, fontSize: 15 }}>
                    {formatMoney(previewSummary.totalPrice, previewSummary.currency)}
                  </div>
                </div>
              </div>
            )}

            <div className="form-field full">
              <button
                type="submit"
                className="btn btn-primary"
                disabled={submitting || selectedItemIds.length === 0 || !selectedCalendarId}
              >
                {submitting ? 'Agendando vía BFF...' : 'Confirmar Reserva'}
              </button>
            </div>
          </div>
        </form>
      </div>

      {/* Enriched bookings table */}
      <div className="card">
        <div className="section-header">
          <h2 className="section-title">Reservas Enriquecidas ({bookings.length})</h2>
          <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>
            Resueltas en lote por GET /api/bookings
          </span>
        </div>

        {loading ? (
          <div className="empty-state">Cargando reservas...</div>
        ) : bookings.length === 0 ? (
          <div className="empty-state">No hay reservas en el rango consultado.</div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Horario</th>
                  <th>Cliente</th>
                  <th>Servicios / Ítems</th>
                  <th>Total</th>
                  <th>Notas</th>
                  <th style={{ width: 70 }}>Acción</th>
                </tr>
              </thead>
              <tbody>
                {bookings.map((b) => (
                  <tr key={b.id}>
                    <td>
                      <div style={{ fontWeight: 600 }}>{formatDateTime(b.starts_at)}</div>
                      <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                        hasta {formatTimeOnly(b.ends_at)}
                      </div>
                    </td>
                    <td>
                      <div style={{ fontWeight: 500 }}>{b.client_name ?? 'Sin cliente'}</div>
                      {b.calendar_name && (
                        <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                          {b.calendar_name}
                        </div>
                      )}
                    </td>
                    <td>
                      <div className="btn-row">
                        {b.items.length > 0 ? (
                          b.items.map((item) => (
                            <StatusBadge key={item.id} label={item.name} tone="blue" />
                          ))
                        ) : (
                          <span>{b.title}</span>
                        )}
                      </div>
                    </td>
                    <td style={{ fontWeight: 600 }}>{formatMoney(b.total_price, b.currency)}</td>
                    <td style={{ color: 'var(--text-muted)', maxWidth: 220 }}>{b.notes ?? '—'}</td>
                    <td>
                      <button
                        type="button"
                        className="btn btn-danger btn-sm"
                        onClick={() => handleDeleteBooking(b)}
                        title="Eliminar reserva"
                      >
                        <Trash2 size={13} />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Create Calendar Modal */}
      <Modal
        open={calModalOpen}
        title="Nuevo Calendario"
        onClose={() => setCalModalOpen(false)}
        footer={
          <>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => setCalModalOpen(false)}
            >
              Cancelar
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleCreateCalendar}
              disabled={savingCal}
            >
              {savingCal ? 'Creando...' : 'Crear Calendario'}
            </button>
          </>
        }
      >
        <div className="form-grid">
          <div className="form-field full">
            <label className="form-label">Nombre *</label>
            <input
              className="input"
              value={calName}
              onChange={(e) => setCalName(e.target.value)}
              placeholder="Ej: Box Atención 1"
            />
          </div>
          <div className="form-field full">
            <label className="form-label">Zona Horaria (IANA)</label>
            <input
              className="input"
              value={calTimezone}
              onChange={(e) => setCalTimezone(e.target.value)}
              placeholder="America/Santiago"
            />
          </div>
          <div className="form-field full">
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={calAllowOverlaps}
                onChange={(e) => setCalAllowOverlaps(e.target.checked)}
              />
              Permitir reservas simultáneas (desmarca para validar 409 Conflict)
            </label>
          </div>
        </div>
      </Modal>
    </div>
  );
}
