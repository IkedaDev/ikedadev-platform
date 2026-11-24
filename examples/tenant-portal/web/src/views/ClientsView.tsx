import { Pencil, Plus, Search, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { bffClient } from '../api/bffClient';
import { Modal } from '../components/Modal';
import { StatusBadge } from '../components/StatusBadge';
import type { Client, Criteria, PaginationMeta } from '../types/api';

interface ClientFormState {
  full_name: string;
  email: string;
  phone: string;
  tax_id: string;
  address: string;
  notes: string;
  metadataText: string;
}

const EMPTY_FORM: ClientFormState = {
  full_name: '',
  email: '',
  phone: '',
  tax_id: '',
  address: '',
  notes: '',
  metadataText: '{}',
};

export function ClientsView() {
  const [clients, setClients] = useState<Client[]>([]);
  const [pagination, setPagination] = useState<PaginationMeta | null>(null);
  const [searchTerm, setSearchTerm] = useState('');
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<Client | null>(null);
  const [form, setForm] = useState<ClientFormState>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

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
      setError(err instanceof Error ? err.message : 'Error consultando clientes');
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

  const openCreate = () => {
    setEditing(null);
    setForm(EMPTY_FORM);
    setFormError(null);
    setModalOpen(true);
  };

  const openEdit = (client: Client) => {
    setEditing(client);
    setForm({
      full_name: client.full_name,
      email: client.email ?? '',
      phone: client.phone ?? '',
      tax_id: client.tax_id ?? '',
      address: client.address ?? '',
      notes: client.notes ?? '',
      metadataText: JSON.stringify(client.metadata ?? {}, null, 2),
    });
    setFormError(null);
    setModalOpen(true);
  };

  const handleSave = async () => {
    setFormError(null);
    if (!form.full_name.trim()) {
      setFormError('El nombre completo es obligatorio.');
      return;
    }

    let metadata: Record<string, unknown> = {};
    try {
      const parsed = JSON.parse(form.metadataText || '{}');
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        throw new Error('Metadata debe ser un objeto JSON.');
      }
      metadata = parsed as Record<string, unknown>;
    } catch (e) {
      setFormError(e instanceof Error ? `Metadata JSON inválido: ${e.message}` : 'Metadata inválido');
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
        metadata,
      };

      if (editing) {
        await bffClient.updateClient(editing.id, payload);
      } else {
        await bffClient.createClient(payload);
      }

      setModalOpen(false);
      await loadClients(searchTerm, page);
    } catch (err) {
      setFormError(err instanceof Error ? err.message : 'Error guardando cliente');
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (client: Client) => {
    if (!confirm(`¿Eliminar al cliente "${client.full_name}"?`)) return;
    try {
      await bffClient.deleteClient(client.id);
      await loadClients(searchTerm, page);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error eliminando cliente');
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <h1 className="page-title">Clientes (CRM)</h1>
          <p className="page-subtitle">
            Búsqueda dinámica con Criteria (nombre, email o RUT/tax_id) sobre crm-api.
          </p>
        </div>
        <button type="button" className="btn btn-primary" onClick={openCreate}>
          <Plus size={15} />
          Nuevo Cliente
        </button>
      </div>

      {error && <div className="alert alert-error">{error}</div>}

      <div className="card">
        <div className="toolbar">
          <div className="search-input-wrap">
            <Search size={15} />
            <input
              type="text"
              className="input"
              placeholder="Buscar por nombre, email o RUT/tax_id..."
              value={searchTerm}
              onChange={(e) => {
                setSearchTerm(e.target.value);
                setPage(1);
              }}
            />
          </div>
          {pagination && (
            <span style={{ fontSize: 13, color: 'var(--text-muted)' }}>
              {pagination.total_items} cliente(s) encontrados
            </span>
          )}
        </div>

        {loading ? (
          <div className="empty-state">Cargando clientes...</div>
        ) : clients.length === 0 ? (
          <div className="empty-state">No se encontraron clientes con ese criterio.</div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nombre</th>
                  <th>RUT / Tax ID</th>
                  <th>Contacto</th>
                  <th>Metadata</th>
                  <th>Estado</th>
                  <th style={{ width: 110 }}>Acciones</th>
                </tr>
              </thead>
              <tbody>
                {clients.map((client) => (
                  <tr key={client.id}>
                    <td>
                      <div style={{ fontWeight: 600 }}>{client.full_name}</div>
                      {client.address && (
                        <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                          {client.address}
                        </div>
                      )}
                    </td>
                    <td>{client.tax_id ?? '—'}</td>
                    <td>
                      <div>{client.email ?? '—'}</div>
                      {client.phone && (
                        <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>{client.phone}</div>
                      )}
                    </td>
                    <td>
                      <div className="btn-row">
                        {Object.entries(client.metadata ?? {}).length === 0 ? (
                          <span style={{ color: 'var(--text-muted)' }}>—</span>
                        ) : (
                          Object.entries(client.metadata).map(([k, v]) => (
                            <StatusBadge key={k} label={`${k}: ${String(v)}`} tone="gray" />
                          ))
                        )}
                      </div>
                    </td>
                    <td>
                      <StatusBadge
                        label={client.is_active ? 'Activo' : 'Inactivo'}
                        tone={client.is_active ? 'green' : 'gray'}
                      />
                    </td>
                    <td>
                      <div className="btn-row">
                        <button
                          type="button"
                          className="btn btn-secondary btn-sm"
                          onClick={() => openEdit(client)}
                          title="Editar"
                        >
                          <Pencil size={13} />
                        </button>
                        <button
                          type="button"
                          className="btn btn-danger btn-sm"
                          onClick={() => handleDelete(client)}
                          title="Eliminar"
                        >
                          <Trash2 size={13} />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {pagination && pagination.total_pages > 1 && (
          <div className="pagination-bar">
            <span>
              Página {pagination.page} de {pagination.total_pages}
            </span>
            <div className="btn-row">
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                disabled={!pagination.has_prev_page}
                onClick={() => setPage((p) => Math.max(1, p - 1))}
              >
                Anterior
              </button>
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                disabled={!pagination.has_next_page}
                onClick={() => setPage((p) => p + 1)}
              >
                Siguiente
              </button>
            </div>
          </div>
        )}
      </div>

      <Modal
        open={modalOpen}
        title={editing ? `Editar Cliente — ${editing.full_name}` : 'Nuevo Cliente'}
        onClose={() => setModalOpen(false)}
        footer={
          <>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => setModalOpen(false)}
            >
              Cancelar
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleSave}
              disabled={saving}
            >
              {saving ? 'Guardando...' : editing ? 'Guardar Cambios' : 'Crear Cliente'}
            </button>
          </>
        }
      >
        {formError && <div className="alert alert-error">{formError}</div>}
        <div className="form-grid">
          <div className="form-field full">
            <label className="form-label">Nombre Completo *</label>
            <input
              className="input"
              value={form.full_name}
              onChange={(e) => setForm({ ...form, full_name: e.target.value })}
              placeholder="Ej: Camila Rojas Valdés"
            />
          </div>
          <div className="form-field">
            <label className="form-label">RUT / Tax ID</label>
            <input
              className="input"
              value={form.tax_id}
              onChange={(e) => setForm({ ...form, tax_id: e.target.value })}
              placeholder="Ej: 18.456.789-2"
            />
          </div>
          <div className="form-field">
            <label className="form-label">Email</label>
            <input
              type="email"
              className="input"
              value={form.email}
              onChange={(e) => setForm({ ...form, email: e.target.value })}
              placeholder="cliente@ejemplo.cl"
            />
          </div>
          <div className="form-field">
            <label className="form-label">Teléfono</label>
            <input
              className="input"
              value={form.phone}
              onChange={(e) => setForm({ ...form, phone: e.target.value })}
              placeholder="+56 9 8123 4567"
            />
          </div>
          <div className="form-field">
            <label className="form-label">Dirección</label>
            <input
              className="input"
              value={form.address}
              onChange={(e) => setForm({ ...form, address: e.target.value })}
              placeholder="Av. Providencia 1234"
            />
          </div>
          <div className="form-field full">
            <label className="form-label">Notas internas</label>
            <textarea
              className="textarea"
              value={form.notes}
              onChange={(e) => setForm({ ...form, notes: e.target.value })}
              placeholder="Preferencias, observaciones..."
            />
          </div>
          <div className="form-field full">
            <label className="form-label">Metadata (JSONB)</label>
            <textarea
              className="textarea"
              style={{ fontFamily: 'monospace' }}
              value={form.metadataText}
              onChange={(e) => setForm({ ...form, metadataText: e.target.value })}
              placeholder='{"segment": "VIP", "channel": "WhatsApp"}'
            />
          </div>
        </div>
      </Modal>
    </div>
  );
}
