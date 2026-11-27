import { FolderPlus, Pencil, Plus, Tag, Trash2 } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { bffClient } from '../../api/bffClient';
import type { ContentCategory, ContentTag } from '../../types/api';
import { Button, Input, StatusBadge, Textarea, useDrawer, useModal, useToast } from '../../ui';

export interface TaxonomyTabProps {
  categories: ContentCategory[];
  tags: ContentTag[];
  canWrite: boolean;
  onRefresh: () => void;
}

function CategoryForm({
  initial,
  onSaved,
  onClose,
}: {
  initial: ContentCategory | null;
  onSaved: () => void;
  onClose: () => void;
}) {
  const { toast } = useToast();
  const [name, setName] = useState(initial?.name ?? '');
  const [slug, setSlug] = useState(initial?.slug ?? '');
  const [description, setDescription] = useState(initial?.description ?? '');
  const [displayOrder, setDisplayOrder] = useState(String(initial?.display_order ?? 0));
  const [saving, setSaving] = useState(false);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    setSaving(true);
    try {
      const payload = {
        name: name.trim(),
        slug: slug.trim() || undefined,
        description: description.trim() || null,
        display_order: Number(displayOrder) || 0,
      };
      if (initial) {
        await bffClient.updateContentCategory(initial.id, payload);
        toast.success('Categoría actualizada');
      } else {
        await bffClient.createContentCategory(payload);
        toast.success('Categoría creada');
      }
      onSaved();
      onClose();
    } catch {
      // handled
    } finally {
      setSaving(false);
    }
  };

  return (
    <form className="stack-md" onSubmit={handleSubmit}>
      <Input
        label="Nombre"
        required
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Ej: Novedades"
      />
      <Input
        label="Slug"
        value={slug}
        onChange={(e) => setSlug(e.target.value)}
        placeholder="novedades (opcional)"
      />
      <Textarea
        label="Descripción"
        rows={3}
        value={description}
        onChange={(e) => setDescription(e.target.value)}
      />
      <Input
        label="Orden"
        type="number"
        value={displayOrder}
        onChange={(e) => setDisplayOrder(e.target.value)}
      />
      <div className="row-end gap-sm">
        <Button type="button" variant="ghost" onClick={onClose}>
          Cancelar
        </Button>
        <Button type="submit" variant="primary" isLoading={saving}>
          Guardar
        </Button>
      </div>
    </form>
  );
}

export function TaxonomyTab({ categories, tags, canWrite, onRefresh }: TaxonomyTabProps) {
  const { openDrawer, closeDrawer } = useDrawer();
  const { confirm } = useModal();
  const { toast } = useToast();
  const [newTagName, setNewTagName] = useState('');
  const [creatingTag, setCreatingTag] = useState(false);
  const [busyRowId, setBusyRowId] = useState<string | null>(null);

  const openCategoryDrawer = (cat: ContentCategory | null) => {
    openDrawer({
      title: cat ? 'Editar categoría' : 'Nueva categoría',
      size: 'sm',
      content: (
        <CategoryForm
          initial={cat}
          onSaved={onRefresh}
          onClose={() => closeDrawer(true)}
        />
      ),
    });
  };

  const handleDeleteCategory = async (cat: ContentCategory) => {
    const ok = await confirm({
      title: 'Eliminar categoría',
      message: `¿Eliminar la categoría "${cat.name}"? Los artículos asociados quedarán sin categoría.`,
      variant: 'danger',
      confirmLabel: 'Eliminar',
    });
    if (!ok) return;
    setBusyRowId(cat.id);
    try {
      await bffClient.deleteContentCategory(cat.id);
      toast.success('Categoría eliminada');
      onRefresh();
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  const handleCreateTag = async (e: FormEvent) => {
    e.preventDefault();
    if (!newTagName.trim()) return;
    setCreatingTag(true);
    try {
      await bffClient.createContentTag({ name: newTagName.trim() });
      setNewTagName('');
      toast.success('Etiqueta creada');
      onRefresh();
    } catch {
      // handled
    } finally {
      setCreatingTag(false);
    }
  };

  const handleDeleteTag = async (tag: ContentTag) => {
    if (!canWrite || busyRowId) return;
    const ok = await confirm({
      title: 'Eliminar etiqueta',
      message: `¿Eliminar la etiqueta "#${tag.name}"? Se desvinculará de los artículos asociados.`,
      variant: 'danger',
      confirmLabel: 'Eliminar',
    });
    if (!ok) return;
    setBusyRowId(tag.id);
    try {
      await bffClient.deleteContentTag(tag.id);
      toast.success('Etiqueta eliminada');
      onRefresh();
    } catch {
      // handled globally
    } finally {
      setBusyRowId(null);
    }
  };

  return (
    <div className="content-two-col">
      <div className="card">
        <div className="card-header row-between">
          <div>
            <h3 className="card-title">Categorías</h3>
            <p className="card-subtitle">Organiza los artículos por tema editorial</p>
          </div>
          {canWrite && (
            <Button
              size="sm"
              variant="primary"
              icon={<FolderPlus size={14} />}
              onClick={() => openCategoryDrawer(null)}
            >
              Nueva
            </Button>
          )}
        </div>

        <div className="stack-xs">
          {categories.length === 0 ? (
            <p className="text-sm text-muted">Aún no has creado categorías.</p>
          ) : (
            categories.map((cat) => (
              <div key={cat.id} className="revision-row">
                <div>
                  <div className="row-xs">
                    <strong className="text-sm">{cat.name}</strong>
                    <StatusBadge label={`/${cat.slug}`} tone="gray" />
                  </div>
                  {cat.description && <p className="text-xs text-muted">{cat.description}</p>}
                </div>
                {canWrite && (
                  <div className="row-xs">
                    <Button
                      size="sm"
                      variant="ghost"
                      icon={<Pencil size={13} />}
                      disabled={busyRowId === cat.id}
                      onClick={() => openCategoryDrawer(cat)}
                      aria-label={`Editar ${cat.name}`}
                    />
                    <Button
                      size="sm"
                      variant="ghost"
                      icon={<Trash2 size={13} />}
                      disabled={busyRowId === cat.id}
                      isLoading={busyRowId === cat.id}
                      onClick={() => handleDeleteCategory(cat)}
                      aria-label={`Eliminar ${cat.name}`}
                    />
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </div>

      <div className="card">
        <div className="card-header">
          <h3 className="card-title">Etiquetas</h3>
          <p className="card-subtitle">Palabras clave reutilizables para filtrado y SEO</p>
        </div>

        {canWrite && (
          <form className="row-sm" onSubmit={handleCreateTag}>
            <Input
              placeholder="Ej: Rust, SEO, Tutorial..."
              value={newTagName}
              onChange={(e) => setNewTagName(e.target.value)}
            />
            <Button
              type="submit"
              size="md"
              variant="primary"
              icon={<Plus size={14} />}
              isLoading={creatingTag}
              disabled={!newTagName.trim() || creatingTag}
            >
              Agregar
            </Button>
          </form>
        )}

        <div className="chip-list taxonomy-tag-list">
          {tags.length === 0 ? (
            <p className="text-sm text-muted">Aún no hay etiquetas creadas.</p>
          ) : (
            tags.map((tag) => (
              <span key={tag.id} className="tag-chip">
                <Tag size={12} />
                <span>{tag.name}</span>
                {canWrite && (
                  <button
                    type="button"
                    className="tag-chip-del"
                    disabled={busyRowId === tag.id}
                    onClick={() => handleDeleteTag(tag)}
                    aria-label={`Eliminar etiqueta ${tag.name}`}
                  >
                    ×
                  </button>
                )}
              </span>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
