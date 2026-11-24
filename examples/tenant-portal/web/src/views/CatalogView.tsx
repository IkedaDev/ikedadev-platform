import { FolderPlus, Pencil, Plus, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { bffClient } from '../api/bffClient';
import { Modal } from '../components/Modal';
import { StatusBadge, formatMoney } from '../components/StatusBadge';
import type { CatalogItem, CatalogItemType, Category, Criteria } from '../types/api';

type TypeFilter = 'ALL' | CatalogItemType;

interface ItemFormState {
  name: string;
  sku: string;
  description: string;
  category_id: string;
  base_price: string;
  currency: string;
  item_type: CatalogItemType;
  duration_minutes: string;
  buffer_minutes: string;
  unit_of_measure: string;
  track_inventory: boolean;
}

const EMPTY_ITEM_FORM: ItemFormState = {
  name: '',
  sku: '',
  description: '',
  category_id: '',
  base_price: '25000',
  currency: 'CLP',
  item_type: 'SERVICE',
  duration_minutes: '45',
  buffer_minutes: '10',
  unit_of_measure: 'UNIT',
  track_inventory: true,
};

export function CatalogView() {
  const [typeFilter, setTypeFilter] = useState<TypeFilter>('ALL');
  const [categories, setCategories] = useState<Category[]>([]);
  const [items, setItems] = useState<CatalogItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Item modal
  const [itemModalOpen, setItemModalOpen] = useState(false);
  const [editingItem, setEditingItem] = useState<CatalogItem | null>(null);
  const [itemForm, setItemForm] = useState<ItemFormState>(EMPTY_ITEM_FORM);
  const [savingItem, setSavingItem] = useState(false);
  const [itemFormError, setItemFormError] = useState<string | null>(null);

  // Category modal
  const [catModalOpen, setCatModalOpen] = useState(false);
  const [catName, setCatName] = useState('');
  const [catSlug, setCatSlug] = useState('');
  const [catDescription, setCatDescription] = useState('');
  const [savingCat, setSavingCat] = useState(false);
  const [catError, setCatError] = useState<string | null>(null);

  const loadData = useCallback(async (filter: TypeFilter) => {
    setLoading(true);
    setError(null);
    try {
      const criteria: Criteria = {
        orderBy: [{ field: 'created_at', direction: 'desc' }],
        pagination: { page: 1, limit: 50 },
      };
      if (filter !== 'ALL') {
        criteria.filter = {
          and: [{ field: 'item_type', operator: 'eq', value: filter }],
        };
      }

      const [cats, itemsPage] = await Promise.all([
        bffClient.listCategories(),
        bffClient.searchItems(criteria),
      ]);
      setCategories(cats);
      setItems(itemsPage.data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error cargando catálogo');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadData(typeFilter);
  }, [typeFilter, loadData]);

  const categoryMap = new Map(categories.map((c) => [c.id, c.name]));

  const openCreateItem = () => {
    setEditingItem(null);
    setItemForm({
      ...EMPTY_ITEM_FORM,
      category_id: categories[0]?.id ?? '',
    });
    setItemFormError(null);
    setItemModalOpen(true);
  };

  const openEditItem = (item: CatalogItem) => {
    setEditingItem(item);
    setItemForm({
      name: item.name,
      sku: item.sku ?? '',
      description: item.description ?? '',
      category_id: item.category_id ?? '',
      base_price: String(item.base_price),
      currency: item.currency,
      item_type: item.item_type,
      duration_minutes: String(item.duration_minutes ?? 30),
      buffer_minutes: String(item.buffer_minutes ?? 0),
      unit_of_measure: item.unit_of_measure ?? 'UNIT',
      track_inventory: Boolean(item.track_inventory),
    });
    setItemFormError(null);
    setItemModalOpen(true);
  };

  const handleSaveItem = async () => {
    setItemFormError(null);
    if (!itemForm.name.trim()) {
      setItemFormError('El nombre del ítem es obligatorio.');
      return;
    }

    const basePayload: Record<string, unknown> = {
      name: itemForm.name.trim(),
      sku: itemForm.sku.trim() || null,
      description: itemForm.description.trim() || null,
      category_id: itemForm.category_id || null,
      base_price: Number(itemForm.base_price) || 0,
      currency: itemForm.currency.trim().toUpperCase() || 'CLP',
      item_type: itemForm.item_type,
    };

    if (itemForm.item_type === 'SERVICE') {
      basePayload.duration_minutes = Number(itemForm.duration_minutes) || 30;
      basePayload.buffer_minutes = Number(itemForm.buffer_minutes) || 0;
    } else if (itemForm.item_type === 'PRODUCT') {
      basePayload.unit_of_measure = itemForm.unit_of_measure.trim() || null;
      basePayload.track_inventory = itemForm.track_inventory;
    } else if (itemForm.item_type === 'BUNDLE') {
      basePayload.items = [];
    }

    setSavingItem(true);
    try {
      if (editingItem) {
        await bffClient.updateItem(editingItem.id, basePayload);
      } else {
        await bffClient.createItem(basePayload);
      }
      setItemModalOpen(false);
      await loadData(typeFilter);
    } catch (err) {
      setItemFormError(err instanceof Error ? err.message : 'Error guardando ítem');
    } finally {
      setSavingItem(false);
    }
  };

  const handleDeleteItem = async (item: CatalogItem) => {
    if (!confirm(`¿Eliminar "${item.name}" del catálogo?`)) return;
    try {
      await bffClient.deleteItem(item.id);
      await loadData(typeFilter);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error eliminando ítem');
    }
  };

  const handleCreateCategory = async () => {
    setCatError(null);
    if (!catName.trim() || !catSlug.trim()) {
      setCatError('Nombre y slug son obligatorios.');
      return;
    }
    setSavingCat(true);
    try {
      await bffClient.createCategory({
        name: catName.trim(),
        slug: catSlug.trim().toLowerCase(),
        description: catDescription.trim() || null,
      });
      setCatName('');
      setCatSlug('');
      setCatDescription('');
      setCatModalOpen(false);
      await loadData(typeFilter);
    } catch (err) {
      setCatError(err instanceof Error ? err.message : 'Error creando categoría');
    } finally {
      setSavingCat(false);
    }
  };

  const handleDeleteCategory = async (cat: Category) => {
    if (!confirm(`¿Eliminar categoría "${cat.name}"?`)) return;
    try {
      await bffClient.deleteCategory(cat.id);
      await loadData(typeFilter);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error eliminando categoría');
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <h1 className="page-title">Catálogo Comercial</h1>
          <p className="page-subtitle">
            Gestiona categorías e ítems polimórficos (SERVICE, PRODUCT, BUNDLE) en catalog-api.
          </p>
        </div>
        <div className="btn-row">
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              setCatError(null);
              setCatModalOpen(true);
            }}
          >
            <FolderPlus size={15} />
            Nueva Categoría
          </button>
          <button type="button" className="btn btn-primary" onClick={openCreateItem}>
            <Plus size={15} />
            Nuevo Ítem
          </button>
        </div>
      </div>

      {error && <div className="alert alert-error">{error}</div>}

      {/* Categories strip */}
      <div className="card" style={{ marginBottom: 20 }}>
        <div className="section-header" style={{ marginBottom: 10 }}>
          <h2 className="section-title">Categorías ({categories.length})</h2>
        </div>
        {categories.length === 0 ? (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>
            Aún no hay categorías creadas.
          </div>
        ) : (
          <div className="btn-row">
            {categories.map((cat) => (
              <div
                key={cat.id}
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: 8,
                  padding: '6px 10px',
                  borderRadius: 8,
                  border: '1px solid var(--border)',
                  background: '#f8fafc',
                  fontSize: 13,
                }}
              >
                <span style={{ fontWeight: 600 }}>{cat.name}</span>
                <span style={{ color: 'var(--text-muted)', fontSize: 11 }}>/{cat.slug}</span>
                <button
                  type="button"
                  onClick={() => handleDeleteCategory(cat)}
                  style={{
                    background: 'transparent',
                    border: 'none',
                    color: 'var(--danger)',
                    cursor: 'pointer',
                    padding: 0,
                    display: 'inline-flex',
                  }}
                  title="Eliminar categoría"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Items table */}
      <div className="card">
        <div className="toolbar">
          <div className="tabs">
            {(['ALL', 'SERVICE', 'PRODUCT', 'BUNDLE'] as TypeFilter[]).map((tab) => (
              <button
                key={tab}
                type="button"
                className={`tab ${typeFilter === tab ? 'active' : ''}`}
                onClick={() => setTypeFilter(tab)}
              >
                {tab === 'ALL' ? 'Todos' : tab}
              </button>
            ))}
          </div>
          <span style={{ fontSize: 13, color: 'var(--text-muted)' }}>{items.length} ítem(s)</span>
        </div>

        {loading ? (
          <div className="empty-state">Cargando catálogo...</div>
        ) : items.length === 0 ? (
          <div className="empty-state">No hay ítems en esta vista.</div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Ítem</th>
                  <th>Tipo</th>
                  <th>Categoría</th>
                  <th>Detalles</th>
                  <th>Precio Base</th>
                  <th style={{ width: 110 }}>Acciones</th>
                </tr>
              </thead>
              <tbody>
                {items.map((item) => (
                  <tr key={item.id}>
                    <td>
                      <div style={{ fontWeight: 600 }}>{item.name}</div>
                      {item.sku && (
                        <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                          SKU: {item.sku}
                        </div>
                      )}
                    </td>
                    <td>
                      <StatusBadge
                        label={item.item_type}
                        tone={
                          item.item_type === 'SERVICE'
                            ? 'blue'
                            : item.item_type === 'PRODUCT'
                              ? 'green'
                              : 'amber'
                        }
                      />
                    </td>
                    <td>{item.category_id ? (categoryMap.get(item.category_id) ?? '—') : '—'}</td>
                    <td>
                      {item.item_type === 'SERVICE' && (
                        <span>
                          {item.duration_minutes} min
                          {item.buffer_minutes ? ` (+${item.buffer_minutes}m buffer)` : ''}
                        </span>
                      )}
                      {item.item_type === 'PRODUCT' && (
                        <span>
                          {item.unit_of_measure ?? 'UNIT'} ·{' '}
                          {item.track_inventory ? 'Con inventario' : 'Sin inventario'}
                        </span>
                      )}
                      {item.item_type === 'BUNDLE' && (
                        <span>{item.items?.length ?? 0} componente(s)</span>
                      )}
                    </td>
                    <td style={{ fontWeight: 600 }}>
                      {formatMoney(item.base_price, item.currency)}
                    </td>
                    <td>
                      <div className="btn-row">
                        <button
                          type="button"
                          className="btn btn-secondary btn-sm"
                          onClick={() => openEditItem(item)}
                        >
                          <Pencil size={13} />
                        </button>
                        <button
                          type="button"
                          className="btn btn-danger btn-sm"
                          onClick={() => handleDeleteItem(item)}
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
      </div>

      {/* Item Modal */}
      <Modal
        open={itemModalOpen}
        title={editingItem ? `Editar Ítem — ${editingItem.name}` : 'Nuevo Ítem de Catálogo'}
        onClose={() => setItemModalOpen(false)}
        footer={
          <>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => setItemModalOpen(false)}
            >
              Cancelar
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleSaveItem}
              disabled={savingItem}
            >
              {savingItem ? 'Guardando...' : 'Guardar Ítem'}
            </button>
          </>
        }
      >
        {itemFormError && <div className="alert alert-error">{itemFormError}</div>}
        <div className="form-grid">
          <div className="form-field full">
            <label className="form-label">Tipo de Ítem *</label>
            <select
              className="select"
              value={itemForm.item_type}
              onChange={(e) =>
                setItemForm({ ...itemForm, item_type: e.target.value as CatalogItemType })
              }
            >
              <option value="SERVICE">SERVICE (Agendable con duración)</option>
              <option value="PRODUCT">PRODUCT (Producto físico o digital)</option>
              <option value="BUNDLE">BUNDLE (Paquete combinado)</option>
            </select>
          </div>

          <div className="form-field full">
            <label className="form-label">Nombre *</label>
            <input
              className="input"
              value={itemForm.name}
              onChange={(e) => setItemForm({ ...itemForm, name: e.target.value })}
              placeholder="Ej: Consulta Inicial 45 min"
            />
          </div>

          <div className="form-field">
            <label className="form-label">SKU</label>
            <input
              className="input"
              value={itemForm.sku}
              onChange={(e) => setItemForm({ ...itemForm, sku: e.target.value })}
              placeholder="Ej: SRV-CONS-45"
            />
          </div>

          <div className="form-field">
            <label className="form-label">Categoría</label>
            <select
              className="select"
              value={itemForm.category_id}
              onChange={(e) => setItemForm({ ...itemForm, category_id: e.target.value })}
            >
              <option value="">Sin categoría</option>
              {categories.map((cat) => (
                <option key={cat.id} value={cat.id}>
                  {cat.name}
                </option>
              ))}
            </select>
          </div>

          <div className="form-field">
            <label className="form-label">Precio Base *</label>
            <input
              type="number"
              min={0}
              className="input"
              value={itemForm.base_price}
              onChange={(e) => setItemForm({ ...itemForm, base_price: e.target.value })}
            />
          </div>

          <div className="form-field">
            <label className="form-label">Moneda (ISO 4217)</label>
            <input
              className="input"
              maxLength={3}
              value={itemForm.currency}
              onChange={(e) => setItemForm({ ...itemForm, currency: e.target.value.toUpperCase() })}
            />
          </div>

          {itemForm.item_type === 'SERVICE' && (
            <>
              <div className="form-field">
                <label className="form-label">Duración (minutos) *</label>
                <input
                  type="number"
                  min={1}
                  className="input"
                  value={itemForm.duration_minutes}
                  onChange={(e) => setItemForm({ ...itemForm, duration_minutes: e.target.value })}
                />
              </div>
              <div className="form-field">
                <label className="form-label">Buffer (minutos)</label>
                <input
                  type="number"
                  min={0}
                  className="input"
                  value={itemForm.buffer_minutes}
                  onChange={(e) => setItemForm({ ...itemForm, buffer_minutes: e.target.value })}
                />
              </div>
            </>
          )}

          {itemForm.item_type === 'PRODUCT' && (
            <>
              <div className="form-field">
                <label className="form-label">Unidad de medida</label>
                <input
                  className="input"
                  value={itemForm.unit_of_measure}
                  onChange={(e) => setItemForm({ ...itemForm, unit_of_measure: e.target.value })}
                  placeholder="UNIT, ML, KG..."
                />
              </div>
              <div className="form-field" style={{ justifyContent: 'flex-end' }}>
                <label className="checkbox-row">
                  <input
                    type="checkbox"
                    checked={itemForm.track_inventory}
                    onChange={(e) =>
                      setItemForm({ ...itemForm, track_inventory: e.target.checked })
                    }
                  />
                  Controlar inventario (track_inventory)
                </label>
              </div>
            </>
          )}

          <div className="form-field full">
            <label className="form-label">Descripción</label>
            <textarea
              className="textarea"
              value={itemForm.description}
              onChange={(e) => setItemForm({ ...itemForm, description: e.target.value })}
            />
          </div>
        </div>
      </Modal>

      {/* Category Modal */}
      <Modal
        open={catModalOpen}
        title="Nueva Categoría"
        onClose={() => setCatModalOpen(false)}
        footer={
          <>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => setCatModalOpen(false)}
            >
              Cancelar
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleCreateCategory}
              disabled={savingCat}
            >
              {savingCat ? 'Creando...' : 'Crear Categoría'}
            </button>
          </>
        }
      >
        {catError && <div className="alert alert-error">{catError}</div>}
        <div className="form-grid">
          <div className="form-field full">
            <label className="form-label">Nombre *</label>
            <input
              className="input"
              value={catName}
              onChange={(e) => {
                const name = e.target.value;
                setCatName(name);
                setCatSlug(
                  name
                    .toLowerCase()
                    .normalize('NFD')
                    .replace(/[\u0300-\u036f]/g, '')
                    .replace(/[^a-z0-9]+/g, '-')
                    .replace(/^-|-$/g, ''),
                );
              }}
              placeholder="Ej: Servicios Premium"
            />
          </div>
          <div className="form-field full">
            <label className="form-label">Slug *</label>
            <input
              className="input"
              value={catSlug}
              onChange={(e) => setCatSlug(e.target.value)}
              placeholder="servicios-premium"
            />
          </div>
          <div className="form-field full">
            <label className="form-label">Descripción</label>
            <textarea
              className="textarea"
              value={catDescription}
              onChange={(e) => setCatDescription(e.target.value)}
            />
          </div>
        </div>
      </Modal>
    </div>
  );
}
