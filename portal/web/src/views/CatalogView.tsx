import { Folder, FolderPlus, Package, Pencil, Plus, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { BffApiError, bffClient } from '../api/bffClient';
import { useTheme } from '../context/ThemeProvider';
import type { CatalogItem, CatalogItemType, Category, Criteria } from '../types/api';
import {
  Button,
  DataTable,
  Input,
  Select,
  StatusBadge,
  Textarea,
  formatMoney,
  useDrawer,
  useModal,
  useToast,
  type DataTableColumn,
} from '../ui';

type TypeFilter = 'ALL' | CatalogItemType;

const ITEM_TYPE_LABEL: Record<CatalogItemType, string> = {
  SERVICE: 'Servicio',
  PRODUCT: 'Producto',
  BUNDLE: 'Paquete',
};

const TYPE_FILTER_LABEL: Record<TypeFilter, string> = {
  ALL: 'Todos',
  SERVICE: 'Servicios',
  PRODUCT: 'Productos',
  BUNDLE: 'Paquetes',
};

const CURRENCY_OPTIONS = ['CLP', 'USD', 'EUR', 'MXN', 'ARS', 'COP', 'PEN'];

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

function slugify(value: string): string {
  return value
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

interface ItemDrawerFormProps {
  initialItem: CatalogItem | null;
  categories: Category[];
  defaultCurrency: string;
  onDirtyChange: (dirty: boolean) => void;
  onSaved: () => void;
  onCancel: () => void;
}

function ItemDrawerForm({
  initialItem,
  categories,
  defaultCurrency,
  onDirtyChange,
  onSaved,
  onCancel,
}: ItemDrawerFormProps) {
  const { toast } = useToast();
  const [form, setForm] = useState<ItemFormState>(() =>
    initialItem
      ? {
          name: initialItem.name,
          sku: initialItem.sku ?? '',
          description: initialItem.description ?? '',
          category_id: initialItem.category_id ?? '',
          base_price: String(initialItem.base_price),
          currency: initialItem.currency,
          item_type: initialItem.item_type,
          duration_minutes: String(initialItem.duration_minutes ?? 30),
          buffer_minutes: String(initialItem.buffer_minutes ?? 0),
          unit_of_measure: initialItem.unit_of_measure ?? 'UNIT',
          track_inventory: Boolean(initialItem.track_inventory),
        }
      : {
          name: '',
          sku: '',
          description: '',
          category_id: categories[0]?.id ?? '',
          base_price: '25000',
          currency: defaultCurrency || 'CLP',
          item_type: 'SERVICE',
          duration_minutes: '45',
          buffer_minutes: '10',
          unit_of_measure: 'UNIT',
          track_inventory: true,
        },
  );
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const currencyList = useMemo(
    () =>
      CURRENCY_OPTIONS.includes(form.currency)
        ? CURRENCY_OPTIONS
        : [form.currency, ...CURRENCY_OPTIONS],
    [form.currency],
  );

  const updateField = <K extends keyof ItemFormState>(key: K, value: ItemFormState[K]) => {
    setForm((prev) => ({ ...prev, [key]: value }));
    onDirtyChange(true);
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setFormError(null);

    if (!form.name.trim()) {
      setFormError('El nombre es obligatorio.');
      return;
    }

    const payload: Record<string, unknown> = {
      name: form.name.trim(),
      sku: form.sku.trim() || null,
      description: form.description.trim() || null,
      category_id: form.category_id || null,
      base_price: Number(form.base_price) || 0,
      currency: form.currency.trim().toUpperCase() || defaultCurrency || 'CLP',
      item_type: form.item_type,
    };

    if (form.item_type === 'SERVICE') {
      payload.duration_minutes = Number(form.duration_minutes) || 30;
      payload.buffer_minutes = Number(form.buffer_minutes) || 0;
    } else if (form.item_type === 'PRODUCT') {
      payload.unit_of_measure = form.unit_of_measure.trim() || null;
      payload.track_inventory = form.track_inventory;
    } else if (form.item_type === 'BUNDLE') {
      payload.items = initialItem?.items ?? [];
    }

    setSaving(true);
    try {
      if (initialItem) {
        await bffClient.updateItem(initialItem.id, payload);
        toast.success(`"${form.name.trim()}" actualizado.`);
      } else {
        await bffClient.createItem(payload);
        toast.success(`"${form.name.trim()}" agregado al catálogo.`);
      }
      onDirtyChange(false);
      onSaved();
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Error guardando ítem';
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

      <Select
        label="Tipo"
        required
        value={form.item_type}
        onChange={(e) => updateField('item_type', e.target.value as CatalogItemType)}
      >
        <option value="SERVICE">Servicio (agendable con duración)</option>
        <option value="PRODUCT">Producto (físico o digital)</option>
        <option value="BUNDLE">Paquete (combinación de servicios o productos)</option>
      </Select>

      <Input
        label="Nombre"
        required
        value={form.name}
        onChange={(e) => updateField('name', e.target.value)}
        placeholder="Ej: Consulta Inicial 45 min"
      />

      <div className="form-grid-2">
        <Input
          label="Código interno"
          value={form.sku}
          onChange={(e) => updateField('sku', e.target.value)}
          placeholder="Ej: SRV-CONS-45"
        />
        <Select
          label="Categoría"
          value={form.category_id}
          onChange={(e) => updateField('category_id', e.target.value)}
        >
          <option value="">Sin categoría</option>
          {categories.map((cat) => (
            <option key={cat.id} value={cat.id}>
              {cat.name}
            </option>
          ))}
        </Select>
      </div>

      <div className="form-grid-2">
        <Input
          label="Precio base"
          required
          type="number"
          min={0}
          value={form.base_price}
          onChange={(e) => updateField('base_price', e.target.value)}
        />
        <Select
          label="Moneda"
          value={form.currency}
          onChange={(e) => updateField('currency', e.target.value)}
        >
          {currencyList.map((curr) => (
            <option key={curr} value={curr}>
              {curr}
            </option>
          ))}
        </Select>
      </div>

      {form.item_type === 'SERVICE' && (
        <div className="form-grid-2">
          <Input
            label="Duración (min)"
            required
            type="number"
            min={1}
            value={form.duration_minutes}
            onChange={(e) => updateField('duration_minutes', e.target.value)}
          />
          <Input
            label="Tiempo entre citas (min)"
            type="number"
            min={0}
            value={form.buffer_minutes}
            onChange={(e) => updateField('buffer_minutes', e.target.value)}
          />
        </div>
      )}

      {form.item_type === 'PRODUCT' && (
        <div className="stack-sm">
          <Input
            label="Unidad de medida"
            value={form.unit_of_measure}
            onChange={(e) => updateField('unit_of_measure', e.target.value)}
            placeholder="Unidad, ml, kg..."
          />
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={form.track_inventory}
              onChange={(e) => updateField('track_inventory', e.target.checked)}
            />
            <span>Controlar stock</span>
          </label>
        </div>
      )}

      <Textarea
        label="Descripción"
        value={form.description}
        onChange={(e) => updateField('description', e.target.value)}
        placeholder="Detalle comercial del servicio o producto..."
      />

      <div className="row-end">
        <Button variant="secondary" type="button" onClick={onCancel} disabled={saving}>
          Cancelar
        </Button>
        <Button variant="primary" type="submit" isLoading={saving}>
          {initialItem ? 'Guardar cambios' : 'Crear ítem'}
        </Button>
      </div>
    </form>
  );
}

interface CategoryDrawerFormProps {
  initialCategory: Category | null;
  onDirtyChange: (dirty: boolean) => void;
  onSaved: () => void;
  onCancel: () => void;
}

function CategoryDrawerForm({
  initialCategory,
  onDirtyChange,
  onSaved,
  onCancel,
}: CategoryDrawerFormProps) {
  const { toast } = useToast();
  const [name, setName] = useState(initialCategory?.name ?? '');
  const [description, setDescription] = useState(initialCategory?.description ?? '');
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setFormError(null);
    const trimmedName = name.trim();
    if (!trimmedName) {
      setFormError('El nombre de la categoría es obligatorio.');
      return;
    }

    const generatedSlug = slugify(trimmedName) || `categoria-${Date.now()}`;

    setSaving(true);
    try {
      if (initialCategory) {
        await bffClient.updateCategory(initialCategory.id, {
          name: trimmedName,
          slug: generatedSlug,
          description: description.trim() || null,
        });
        toast.success(`Categoría "${trimmedName}" actualizada.`);
      } else {
        await bffClient.createCategory({
          name: trimmedName,
          slug: generatedSlug,
          description: description.trim() || null,
        });
        toast.success(`Categoría "${trimmedName}" creada.`);
      }
      onDirtyChange(false);
      onSaved();
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Error guardando categoría';
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
        placeholder="Ej: Servicios Premium"
      />

      <Textarea
        label="Descripción"
        value={description}
        onChange={(e) => {
          setDescription(e.target.value);
          onDirtyChange(true);
        }}
        placeholder="Descripción opcional de la categoría..."
      />

      <div className="row-end">
        <Button variant="secondary" type="button" onClick={onCancel} disabled={saving}>
          Cancelar
        </Button>
        <Button variant="primary" type="submit" isLoading={saving}>
          {initialCategory ? 'Guardar cambios' : 'Crear categoría'}
        </Button>
      </div>
    </form>
  );
}

export function CatalogView() {
  const { settings } = useTheme();
  const { openDrawer, closeDrawer } = useDrawer();
  const { confirm } = useModal();
  const { toast } = useToast();

  const [typeFilter, setTypeFilter] = useState<TypeFilter>('ALL');
  const [searchItem, setSearchItem] = useState('');
  const [categories, setCategories] = useState<Category[]>([]);
  const [items, setItems] = useState<CatalogItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyRowId, setBusyRowId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const dirtyRef = useRef(false);

  const loadData = useCallback(async (filter: TypeFilter) => {
    setLoading(true);
    setError(null);
    try {
      const criteria: Criteria = {
        orderBy: [{ field: 'created_at', direction: 'desc' }],
        pagination: { page: 1, limit: 100 },
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
      setError(err instanceof Error ? err.message : 'No pudimos cargar el catálogo.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadData(typeFilter);
  }, [typeFilter, loadData]);

  const categoryMap = useMemo(
    () => new Map(categories.map((c) => [c.id, c.name])),
    [categories],
  );

  const filteredItems = useMemo(() => {
    const q = searchItem.trim().toLowerCase();
    if (!q) return items;
    return items.filter(
      (i) =>
        i.name.toLowerCase().includes(q) ||
        (i.sku && i.sku.toLowerCase().includes(q)) ||
        (i.description && i.description.toLowerCase().includes(q)),
    );
  }, [items, searchItem]);

  const openItemDrawer = useCallback(
    (item: CatalogItem | null) => {
      dirtyRef.current = false;
      openDrawer({
        title: item ? 'Editar ítem' : 'Nuevo ítem del catálogo',
        subtitle: item ? item.name : 'Configura un servicio, producto o paquete.',
        size: 'md',
        hasUnsavedChanges: () => dirtyRef.current,
        content: (
          <ItemDrawerForm
            initialItem={item}
            categories={categories}
            defaultCurrency={settings.locale.currency}
            onDirtyChange={(dirty) => {
              dirtyRef.current = dirty;
            }}
            onCancel={() => closeDrawer(false)}
            onSaved={() => {
              closeDrawer(true);
              void loadData(typeFilter);
            }}
          />
        ),
      });
    },
    [openDrawer, closeDrawer, categories, settings.locale.currency, loadData, typeFilter],
  );

  const openCategoryDrawer = useCallback(
    (category: Category | null) => {
      dirtyRef.current = false;
      openDrawer({
        title: category ? 'Editar categoría' : 'Nueva categoría',
        subtitle: category
          ? category.name
          : 'Organiza los servicios y productos de tu catálogo.',
        size: 'sm',
        hasUnsavedChanges: () => dirtyRef.current,
        content: (
          <CategoryDrawerForm
            initialCategory={category}
            onDirtyChange={(dirty) => {
              dirtyRef.current = dirty;
            }}
            onCancel={() => closeDrawer(false)}
            onSaved={() => {
              closeDrawer(true);
              void loadData(typeFilter);
            }}
          />
        ),
      });
    },
    [openDrawer, closeDrawer, loadData, typeFilter],
  );

  const handleDeleteItem = useCallback(
    async (item: CatalogItem) => {
      if (busyRowId === item.id) return;
      const ok = await confirm({
        title: 'Eliminar del catálogo',
        message: `¿Seguro que deseas eliminar "${item.name}"?`,
        variant: 'danger',
        confirmLabel: 'Eliminar',
      });
      if (!ok) return;

      setBusyRowId(item.id);
      try {
        await bffClient.deleteItem(item.id);
        toast.success(`"${item.name}" eliminado del catálogo.`);
        await loadData(typeFilter);
      } catch (err) {
        if (!(err instanceof BffApiError && err.notified)) {
          toast.error(err instanceof Error ? err.message : 'Error eliminando ítem');
        }
      } finally {
        setBusyRowId((prev) => (prev === item.id ? null : prev));
      }
    },
    [busyRowId, confirm, toast, loadData, typeFilter],
  );

  const handleDeleteCategory = useCallback(
    async (cat: Category) => {
      if (busyRowId === cat.id) return;
      const ok = await confirm({
        title: 'Eliminar categoría',
        message: `¿Seguro que deseas eliminar la categoría "${cat.name}"?`,
        variant: 'danger',
        confirmLabel: 'Eliminar',
      });
      if (!ok) return;

      setBusyRowId(cat.id);
      try {
        await bffClient.deleteCategory(cat.id);
        toast.success(`Categoría "${cat.name}" eliminada.`);
        await loadData(typeFilter);
      } catch (err) {
        if (!(err instanceof BffApiError && err.notified)) {
          toast.error(err instanceof Error ? err.message : 'Error eliminando categoría');
        }
      } finally {
        setBusyRowId((prev) => (prev === cat.id ? null : prev));
      }
    },
    [busyRowId, confirm, toast, loadData, typeFilter],
  );

  const categoryColumns = useMemo<DataTableColumn<Category>[]>(
    () => [
      {
        key: 'name',
        header: 'Categoría',
        sortable: true,
        sortValue: (c) => c.name,
        render: (c) => <span className="font-semibold">{c.name}</span>,
      },
      {
        key: 'description',
        header: 'Descripción',
        hideOnMobile: true,
        render: (c) => <span className="text-sm text-muted">{c.description ?? '—'}</span>,
      },
      {
        key: 'status',
        header: 'Estado',
        render: (c) => (
          <StatusBadge
            label={c.is_active ? 'Activa' : 'Inactiva'}
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
              variant="secondary"
              size="sm"
              icon={<Pencil size={13} />}
              onClick={() => openCategoryDrawer(c)}
              aria-label={`Editar categoría ${c.name}`}
              title="Editar categoría"
            />
            <Button
              variant="danger"
              size="sm"
              icon={<Trash2 size={13} />}
              onClick={() => handleDeleteCategory(c)}
              disabled={busyRowId === c.id}
              isLoading={busyRowId === c.id}
              aria-label={`Eliminar categoría ${c.name}`}
              title="Eliminar categoría"
            />
          </div>
        ),
      },
    ],
    [busyRowId, openCategoryDrawer, handleDeleteCategory],
  );

  const itemColumns = useMemo<DataTableColumn<CatalogItem>[]>(
    () => [
      {
        key: 'name',
        header: 'Nombre',
        sortable: true,
        sortValue: (i) => i.name,
        render: (i) => (
          <div className="stack-xs">
            <span className="font-semibold">{i.name}</span>
            {i.sku && <span className="text-xs text-muted">Código: {i.sku}</span>}
          </div>
        ),
      },
      {
        key: 'type',
        header: 'Tipo',
        sortable: true,
        sortValue: (i) => ITEM_TYPE_LABEL[i.item_type],
        render: (i) => (
          <StatusBadge
            label={ITEM_TYPE_LABEL[i.item_type] ?? i.item_type}
            tone={
              i.item_type === 'SERVICE'
                ? 'blue'
                : i.item_type === 'PRODUCT'
                  ? 'green'
                  : 'amber'
            }
          />
        ),
      },
      {
        key: 'category',
        header: 'Categoría',
        hideOnMobile: true,
        render: (i) => (i.category_id ? (categoryMap.get(i.category_id) ?? '—') : '—'),
      },
      {
        key: 'details',
        header: 'Detalles',
        hideOnMobile: true,
        render: (i) => (
          <span className="text-sm text-muted">
            {i.item_type === 'SERVICE' &&
              `${i.duration_minutes ?? 0} min${i.buffer_minutes ? ` (+${i.buffer_minutes} min entre citas)` : ''}`}
            {i.item_type === 'PRODUCT' &&
              `${i.unit_of_measure ?? 'Unidad'} · ${i.track_inventory ? 'Con control de stock' : 'Sin control de stock'}`}
            {i.item_type === 'BUNDLE' && `${i.items?.length ?? 0} componente(s)`}
          </span>
        ),
      },
      {
        key: 'price',
        header: 'Precio base',
        align: 'right',
        sortable: true,
        sortValue: (i) => i.base_price,
        render: (i) => (
          <span className="font-semibold">{formatMoney(i.base_price, i.currency)}</span>
        ),
      },
      {
        key: 'actions',
        header: 'Acciones',
        align: 'right',
        render: (i) => (
          <div className="row-end">
            <Button
              variant="secondary"
              size="sm"
              icon={<Pencil size={13} />}
              onClick={() => openItemDrawer(i)}
              aria-label={`Editar ${i.name}`}
              title="Editar"
            />
            <Button
              variant="danger"
              size="sm"
              icon={<Trash2 size={13} />}
              onClick={() => handleDeleteItem(i)}
              disabled={busyRowId === i.id}
              isLoading={busyRowId === i.id}
              aria-label={`Eliminar ${i.name}`}
              title="Eliminar"
            />
          </div>
        ),
      },
    ],
    [busyRowId, categoryMap, openItemDrawer, handleDeleteItem],
  );

  return (
    <div className="stack-lg">
      <div className="page-header">
        <div>
          <h1 className="page-title">Catálogo</h1>
          <p className="page-subtitle">
            Organiza tus categorías, servicios agendables, productos y paquetes.
          </p>
        </div>
        <div className="page-actions">
          <Button
            variant="secondary"
            icon={<FolderPlus size={15} />}
            onClick={() => openCategoryDrawer(null)}
          >
            Nueva categoría
          </Button>
          <Button
            variant="primary"
            icon={<Plus size={15} />}
            onClick={() => openItemDrawer(null)}
          >
            Nuevo ítem
          </Button>
        </div>
      </div>

      {error && (
        <div className="alert-banner alert-banner-danger" role="alert">
          <div className="alert-banner-body">{error}</div>
        </div>
      )}

      <div className="stack-sm">
        <div className="row-between">
          <h2 className="card-title">Categorías ({categories.length})</h2>
        </div>
        <DataTable<Category>
          columns={categoryColumns}
          data={categories}
          rowKey={(c) => c.id}
          isLoading={loading}
          skeletonRows={3}
          emptyTitle="Sin categorías"
          emptyDescription="Crea categorías para agrupar tus servicios y productos."
          emptyIcon={<Folder size={22} />}
          emptyAction={
            <Button
              variant="secondary"
              size="sm"
              icon={<FolderPlus size={14} />}
              onClick={() => openCategoryDrawer(null)}
            >
              Nueva categoría
            </Button>
          }
        />
      </div>

      <div className="stack-sm">
        <div className="row-between">
          <h2 className="card-title">Servicios y productos ({filteredItems.length})</h2>
        </div>
        <DataTable<CatalogItem>
          columns={itemColumns}
          data={filteredItems}
          rowKey={(i) => i.id}
          isLoading={loading}
          searchValue={searchItem}
          onSearchChange={setSearchItem}
          searchPlaceholder="Buscar por nombre o código..."
          toolbar={
            <div className="filter-pills">
              {(['ALL', 'SERVICE', 'PRODUCT', 'BUNDLE'] as TypeFilter[]).map((tab) => (
                <button
                  key={tab}
                  type="button"
                  className={`pill-btn ${typeFilter === tab ? 'active' : ''}`}
                  onClick={() => setTypeFilter(tab)}
                >
                  {TYPE_FILTER_LABEL[tab]}
                </button>
              ))}
            </div>
          }
          emptyTitle="Sin elementos en el catálogo"
          emptyDescription="No hay ítems que coincidan con el filtro seleccionado."
          emptyIcon={<Package size={22} />}
          emptyAction={
            <Button
              variant="primary"
              size="sm"
              icon={<Plus size={14} />}
              onClick={() => openItemDrawer(null)}
            >
              Nuevo ítem
            </Button>
          }
        />
      </div>
    </div>
  );
}
