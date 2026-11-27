import { Activity, Eye, Pencil, Plus, Trash2, Users } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { BffApiError, bffClient } from '../api/bffClient';
import { usePermissions } from '../auth/permissions';
import type { AuthUser, Client, Criteria, PaginationMeta, TelemetryEvent } from '../types/api';
import {
  Button,
  DataTable,
  EmptyState,
  Input,
  Skeleton,
  StatusBadge,
  Textarea,
  formatDateTime,
  useDrawer,
  useModal,
  useToast,
  type DataTableColumn,
} from '../ui';

interface CustomFieldRow {
  id: string;
  key: string;
  value: string;
}

interface ClientFormState {
  full_name: string;
  email: string;
  phone: string;
  tax_id: string;
  address: string;
  notes: string;
  customFields: CustomFieldRow[];
}

const EVENT_LABELS: Record<string, string> = {
  'page.viewed': 'Vista visitada',
  'client.created': 'Cliente registrado',
  'client.updated': 'Ficha actualizada',
  'client.deleted': 'Cliente eliminado',
  'booking.created': 'Reserva creada',
  'booking.updated': 'Reserva actualizada',
  'booking.cancelled': 'Reserva cancelada',
  'booking.completed': 'Reserva completada',
  'booking.deleted': 'Reserva eliminada',
};

function humanEventLabel(name: string): string {
  return (
    EVENT_LABELS[name] ??
    name
      .replace(/[._-]+/g, ' ')
      .replace(/^\w/, (c) => c.toUpperCase())
  );
}

function metadataToRows(metadata?: Record<string, unknown>): CustomFieldRow[] {
  if (!metadata) return [];
  return Object.entries(metadata).map(([key, value], idx) => ({
    id: `cf_${idx}_${key}`,
    key,
    value: typeof value === 'object' && value !== null ? JSON.stringify(value) : String(value),
  }));
}

function rowsToMetadata(rows: CustomFieldRow[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const row of rows) {
    const k = row.key.trim();
    if (!k) continue;
    result[k] = row.value.trim();
  }
  return result;
}

interface ClientFormProps {
  initialClient: Client | null;
  onDirtyChange: (dirty: boolean) => void;
  onSaved: () => void;
  onCancel: () => void;
}

function ClientDrawerForm({ initialClient, onDirtyChange, onSaved, onCancel }: ClientFormProps) {
  const { toast } = useToast();
  const [form, setForm] = useState<ClientFormState>(() =>
    initialClient
      ? {
          full_name: initialClient.full_name,
          email: initialClient.email ?? '',
          phone: initialClient.phone ?? '',
          tax_id: initialClient.tax_id ?? '',
          address: initialClient.address ?? '',
          notes: initialClient.notes ?? '',
          customFields: metadataToRows(initialClient.metadata),
        }
      : {
          full_name: '',
          email: '',
          phone: '',
          tax_id: '',
          address: '',
          notes: '',
          customFields: [],
        },
  );
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const updateField = <K extends keyof ClientFormState>(key: K, value: ClientFormState[K]) => {
    setForm((prev) => ({ ...prev, [key]: value }));
    onDirtyChange(true);
  };

  const addCustomField = () => {
    setForm((prev) => ({
      ...prev,
      customFields: [
        ...prev.customFields,
        { id: `cf_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`, key: '', value: '' },
      ],
    }));
    onDirtyChange(true);
  };

  const updateCustomField = (id: string, field: 'key' | 'value', nextVal: string) => {
    setForm((prev) => ({
      ...prev,
      customFields: prev.customFields.map((row) =>
        row.id === id ? { ...row, [field]: nextVal } : row,
      ),
    }));
    onDirtyChange(true);
  };

  const removeCustomField = (id: string) => {
    setForm((prev) => ({
      ...prev,
      customFields: prev.customFields.filter((row) => row.id !== id),
    }));
    onDirtyChange(true);
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setFormError(null);

    if (!form.full_name.trim()) {
      setFormError('El nombre completo es obligatorio.');
      return;
    }

    setSaving(true);
    try {
      const payload = {
        full_name: form.full_name.trim(),
        email: form.email.trim() || null,
        phone: form.phone.trim() || null,
        tax_id: form.tax_id.trim() || null,
        address: form.address.trim() || null,
        notes: form.notes.trim() || null,
        metadata: rowsToMetadata(form.customFields),
      };

      if (initialClient) {
        await bffClient.updateClient(initialClient.id, payload);
        toast.success(`Cliente "${payload.full_name}" actualizado.`);
      } else {
        await bffClient.createClient(payload);
        toast.success(`Cliente "${payload.full_name}" creado.`);
      }

      onDirtyChange(false);
      onSaved();
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Error guardando cliente';
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
        label="Nombre completo"
        required
        value={form.full_name}
        onChange={(e) => updateField('full_name', e.target.value)}
        placeholder="Ej: Camila Rojas Valdés"
      />

      <div className="form-grid-2">
        <Input
          label="RUT / Documento"
          value={form.tax_id}
          onChange={(e) => updateField('tax_id', e.target.value)}
          placeholder="Ej: 18.456.789-2"
        />
        <Input
          label="Correo electrónico"
          type="email"
          value={form.email}
          onChange={(e) => updateField('email', e.target.value)}
          placeholder="cliente@ejemplo.cl"
        />
      </div>

      <div className="form-grid-2">
        <Input
          label="Teléfono"
          value={form.phone}
          onChange={(e) => updateField('phone', e.target.value)}
          placeholder="+56 9 8123 4567"
        />
        <Input
          label="Dirección"
          value={form.address}
          onChange={(e) => updateField('address', e.target.value)}
          placeholder="Av. Providencia 1234"
        />
      </div>

      <Textarea
        label="Notas internas"
        value={form.notes}
        onChange={(e) => updateField('notes', e.target.value)}
        placeholder="Preferencias, observaciones de atención..."
      />

      <div className="field">
        <div className="row-between">
          <span className="field-label">Campos personalizados</span>
          <Button
            variant="ghost"
            size="sm"
            type="button"
            icon={<Plus size={14} />}
            onClick={addCustomField}
          >
            Agregar campo
          </Button>
        </div>

        <div className="kv-editor">
          {form.customFields.length === 0 ? (
            <span className="text-xs text-muted">
              Agrega etiquetas o atributos adicionales como Segmento, Canal o Convenio.
            </span>
          ) : (
            form.customFields.map((row) => (
              <div key={row.id} className="kv-row">
                <Input
                  value={row.key}
                  onChange={(e) => updateCustomField(row.id, 'key', e.target.value)}
                  placeholder="Campo (ej: Segmento)"
                  aria-label="Nombre del campo"
                />
                <Input
                  value={row.value}
                  onChange={(e) => updateCustomField(row.id, 'value', e.target.value)}
                  placeholder="Valor (ej: VIP)"
                  aria-label="Valor del campo"
                />
                <Button
                  variant="ghost"
                  size="sm"
                  type="button"
                  icon={<Trash2 size={14} />}
                  onClick={() => removeCustomField(row.id)}
                  aria-label="Quitar campo"
                />
              </div>
            ))
          )}
        </div>
      </div>

      <div className="row-end">
        <Button variant="secondary" type="button" onClick={onCancel} disabled={saving}>
          Cancelar
        </Button>
        <Button variant="primary" type="submit" isLoading={saving}>
          {initialClient ? 'Guardar cambios' : 'Crear cliente'}
        </Button>
      </div>
    </form>
  );
}

interface Client360DrawerProps {
  client: Client;
  onEdit?: () => void;
}

function Client360Drawer({ client, onEdit }: Client360DrawerProps) {
  const [timeline, setTimeline] = useState<TelemetryEvent[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    setLoading(true);
    bffClient
      .getClientTimeline(client.id, { page: 1, limit: 20 })
      .then((res) => {
        if (active) setTimeline(res.data ?? []);
      })
      .catch(() => {
        if (active) setTimeline([]);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [client.id]);

  const customEntries = Object.entries(client.metadata ?? {});

  return (
    <div className="stack-lg">
      <div className="card card-compact">
        <div className="row-between">
          <div className="stack-xs">
            <span className="font-bold">{client.full_name}</span>
            <StatusBadge
              label={client.is_active ? 'Activo' : 'Inactivo'}
              tone={client.is_active ? 'green' : 'gray'}
            />
          </div>
          {onEdit && (
            <Button variant="secondary" size="sm" icon={<Pencil size={13} />} onClick={onEdit}>
              Editar ficha
            </Button>
          )}
        </div>

        <div className="detail-list">
          <div className="detail-row">
            <span className="text-muted">RUT / Documento</span>
            <span>{client.tax_id ?? '—'}</span>
          </div>
          <div className="detail-row">
            <span className="text-muted">Correo</span>
            <span>{client.email ?? '—'}</span>
          </div>
          <div className="detail-row">
            <span className="text-muted">Teléfono</span>
            <span>{client.phone ?? '—'}</span>
          </div>
          <div className="detail-row">
            <span className="text-muted">Dirección</span>
            <span>{client.address ?? '—'}</span>
          </div>
          <div className="detail-row">
            <span className="text-muted">Registrado</span>
            <span className="text-xs">{formatDateTime(client.created_at)}</span>
          </div>
        </div>

        {client.notes && (
          <div className="stack-xs">
            <span className="text-xs font-semibold text-muted">Notas internas</span>
            <p className="text-sm">{client.notes}</p>
          </div>
        )}

        {customEntries.length > 0 && (
          <div className="stack-xs">
            <span className="text-xs font-semibold text-muted">Campos personalizados</span>
            <div className="chip-list">
              {customEntries.map(([k, v]) => (
                <StatusBadge key={k} label={`${k}: ${String(v)}`} tone="gray" />
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="stack-sm">
        <div className="row-between">
          <h3 className="card-title">Historial de actividad</h3>
          <StatusBadge label={`${timeline.length} registros`} tone="blue" />
        </div>

        {loading ? (
          <div className="stack-sm">
            <Skeleton variant="line" />
            <Skeleton variant="line" />
            <Skeleton variant="line-sm" />
          </div>
        ) : timeline.length === 0 ? (
          <EmptyState
            title="Sin actividad registrada"
            description="Las reservas e interacciones de este cliente aparecerán aquí."
            icon={<Activity size={20} />}
          />
        ) : (
          <div className="timeline-list">
            {timeline.map((ev) => (
              <div key={ev.id} className="timeline-item">
                <span className="timeline-dot" />
                <div className="row-between">
                  <span className="text-sm font-semibold">{humanEventLabel(ev.event_name)}</span>
                  <StatusBadge
                    label={ev.kind === 'AUDIT' ? 'Cambio' : 'Interacción'}
                    tone={ev.kind === 'AUDIT' ? 'amber' : 'blue'}
                  />
                </div>
                <span className="text-xs text-muted">{formatDateTime(ev.occurred_at)}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export interface ClientsViewProps {
  user: AuthUser;
}

export function ClientsView({ user }: ClientsViewProps) {
  const { can } = usePermissions(user);
  const canWrite = can('crm:write');
  const { openDrawer, closeDrawer } = useDrawer();
  const { confirm } = useModal();
  const { toast } = useToast();

  const [clients, setClients] = useState<Client[]>([]);
  const [pagination, setPagination] = useState<PaginationMeta | null>(null);
  const [searchTerm, setSearchTerm] = useState('');
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [busyClientId, setBusyClientId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const dirtyRef = useRef(false);

  const loadClients = useCallback(async (query: string, targetPage: number) => {
    setLoading(true);
    setError(null);
    try {
      const trimmed = query.trim();
      const criteria: Criteria = {
        orderBy: [{ field: 'created_at', direction: 'desc' }],
        pagination: { page: targetPage, limit: 10 },
      };

      if (trimmed) {
        criteria.filter = {
          or: [
            { field: 'full_name', operator: 'contains', value: trimmed },
            { field: 'email', operator: 'contains', value: trimmed },
            { field: 'tax_id', operator: 'contains', value: trimmed },
          ],
        };
      }

      const res = await bffClient.searchClients(criteria);
      setClients(res.data);
      setPagination(res.pagination);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'No pudimos cargar la lista de clientes.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => {
      void loadClients(searchTerm, page);
    }, 220);
    return () => clearTimeout(timer);
  }, [searchTerm, page, loadClients]);

  const openClientDrawer = useCallback(
    (client: Client | null) => {
      dirtyRef.current = false;
      openDrawer({
        title: client ? 'Editar cliente' : 'Nuevo cliente',
        subtitle: client ? client.full_name : 'Registra los datos de contacto del cliente.',
        size: 'md',
        hasUnsavedChanges: () => dirtyRef.current,
        content: (
          <ClientDrawerForm
            initialClient={client}
            onDirtyChange={(dirty) => {
              dirtyRef.current = dirty;
            }}
            onCancel={() => closeDrawer(false)}
            onSaved={() => {
              closeDrawer(true);
              void loadClients(searchTerm, page);
            }}
          />
        ),
      });
    },
    [openDrawer, closeDrawer, loadClients, searchTerm, page],
  );

  const openClientProfile = useCallback(
    (client: Client) => {
      dirtyRef.current = false;
      openDrawer({
        title: 'Ficha del cliente',
        subtitle: client.full_name,
        size: 'md',
        content: (
          <Client360Drawer
            client={client}
            onEdit={canWrite ? () => openClientDrawer(client) : undefined}
          />
        ),
      });
    },
    [openDrawer, openClientDrawer, canWrite],
  );

  const handleDelete = useCallback(
    async (client: Client) => {
      if (busyClientId === client.id) return;
      const ok = await confirm({
        title: 'Eliminar cliente',
        message: `¿Estás seguro de eliminar a "${client.full_name}"? Esta acción no se puede deshacer.`,
        variant: 'danger',
        confirmLabel: 'Eliminar',
      });
      if (!ok) return;

      setBusyClientId(client.id);
      try {
        await bffClient.deleteClient(client.id);
        toast.success(`Cliente "${client.full_name}" eliminado.`);
        await loadClients(searchTerm, page);
      } catch (err) {
        if (!(err instanceof BffApiError && err.notified)) {
          toast.error(err instanceof Error ? err.message : 'Error eliminando cliente');
        }
      } finally {
        setBusyClientId((prev) => (prev === client.id ? null : prev));
      }
    },
    [busyClientId, confirm, toast, loadClients, searchTerm, page],
  );

  const columns = useMemo<DataTableColumn<Client>[]>(
    () => [
      {
        key: 'full_name',
        header: 'Nombre',
        sortable: true,
        sortValue: (c) => c.full_name,
        render: (c) => (
          <div className="stack-xs">
            <span className="font-semibold">{c.full_name}</span>
            {c.address && <span className="text-xs text-muted">{c.address}</span>}
          </div>
        ),
      },
      {
        key: 'tax_id',
        header: 'RUT / Documento',
        sortable: true,
        sortValue: (c) => c.tax_id ?? '',
        render: (c) => c.tax_id ?? '—',
      },
      {
        key: 'contact',
        header: 'Contacto',
        render: (c) => (
          <div className="stack-xs">
            <span>{c.email ?? '—'}</span>
            {c.phone && <span className="text-xs text-muted">{c.phone}</span>}
          </div>
        ),
      },
      {
        key: 'metadata',
        header: 'Etiquetas',
        hideOnMobile: true,
        render: (c) => {
          const entries = Object.entries(c.metadata ?? {});
          if (entries.length === 0) {
            return <span className="text-muted">—</span>;
          }
          return (
            <div className="chip-list">
              {entries.map(([k, v]) => (
                <StatusBadge key={k} label={`${k}: ${String(v)}`} tone="gray" />
              ))}
            </div>
          );
        },
      },
      {
        key: 'status',
        header: 'Estado',
        sortable: true,
        sortValue: (c) => (c.is_active ? 'Activo' : 'Inactivo'),
        render: (c) => (
          <StatusBadge
            label={c.is_active ? 'Activo' : 'Inactivo'}
            tone={c.is_active ? 'green' : 'gray'}
          />
        ),
      },
      {
        key: 'actions',
        header: 'Acciones',
        align: 'right',
        render: (c) => (
          <div className="row-end">
            <Button
              variant="ghost"
              size="sm"
              icon={<Eye size={14} />}
              onClick={() => openClientProfile(c)}
              aria-label={`Ver ficha de ${c.full_name}`}
              title="Ver ficha"
            />
            {canWrite && (
              <>
                <Button
                  variant="secondary"
                  size="sm"
                  icon={<Pencil size={13} />}
                  onClick={() => openClientDrawer(c)}
                  aria-label={`Editar ${c.full_name}`}
                  title="Editar"
                />
                <Button
                  variant="danger"
                  size="sm"
                  icon={<Trash2 size={13} />}
                  onClick={() => handleDelete(c)}
                  disabled={busyClientId === c.id}
                  isLoading={busyClientId === c.id}
                  aria-label={`Eliminar ${c.full_name}`}
                  title="Eliminar"
                />
              </>
            )}
          </div>
        ),
      },
    ],
    [busyClientId, canWrite, openClientProfile, openClientDrawer, handleDelete],
  );

  return (
    <div className="stack-lg">
      <div className="page-header">
        <div>
          <h1 className="page-title">Clientes</h1>
          <p className="page-subtitle">
            Administra tu cartera de clientes, datos de contacto e historial de actividad.
          </p>
        </div>
        {canWrite && (
          <div className="page-actions">
            <Button
              variant="primary"
              icon={<Plus size={15} />}
              onClick={() => openClientDrawer(null)}
            >
              Nuevo cliente
            </Button>
          </div>
        )}
      </div>

      {error && (
        <div className="alert-banner alert-banner-danger" role="alert">
          <div className="alert-banner-body">{error}</div>
        </div>
      )}

      <DataTable<Client>
        columns={columns}
        data={clients}
        rowKey={(c) => c.id}
        isLoading={loading}
        searchValue={searchTerm}
        onSearchChange={(val) => {
          setSearchTerm(val);
          setPage(1);
        }}
        searchPlaceholder="Buscar por nombre, correo o RUT..."
        page={pagination?.page ?? page}
        totalPages={pagination?.total_pages ?? 1}
        total={pagination?.total_items}
        onPageChange={setPage}
        emptyTitle="No se encontraron clientes"
        emptyDescription={
          canWrite
            ? 'Prueba con otro término de búsqueda o agrega tu primer cliente.'
            : 'No hay clientes registrados que coincidan con tu búsqueda.'
        }
        emptyIcon={<Users size={22} />}
        emptyAction={
          canWrite ? (
            <Button
              variant="primary"
              size="sm"
              icon={<Plus size={14} />}
              onClick={() => openClientDrawer(null)}
            >
              Nuevo cliente
            </Button>
          ) : undefined
        }
      />
    </div>
  );
}
