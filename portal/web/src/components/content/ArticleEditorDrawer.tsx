import {
  ChevronDown,
  ChevronUp,
  Clock,
  Eye,
  History,
  Image as ImageIcon,
  RotateCcw,
  Save,
  Send,
} from 'lucide-react';
import { useEffect, useState, type FormEvent } from 'react';
import { bffClient } from '../../api/bffClient';
import type { Article, ArticleRevision, ContentCategory, ContentTag } from '../../types/api';
import { Button, Input, Select, StatusBadge, Textarea, formatDateTime, useToast } from '../../ui';
import { useMediaPicker } from '../shared/MediaPickerModal';
import { RichTextEditor } from './RichTextEditor';

export interface ArticleEditorDrawerProps {
  article: Article | null;
  categories: ContentCategory[];
  tags: ContentTag[];
  authorName: string;
  canPublish: boolean;
  onSaved: () => void;
  onClose: () => void;
}

export function ArticleEditorDrawer({
  article,
  categories,
  tags,
  authorName,
  canPublish,
  onSaved,
  onClose,
}: ArticleEditorDrawerProps) {
  const { toast } = useToast();
  const { openMediaPicker } = useMediaPicker();
  const isLockedForStaff = Boolean(article && article.status !== 'DRAFT' && !canPublish);

  const [title, setTitle] = useState(article?.title ?? '');
  const [slug, setSlug] = useState(article?.slug ?? '');
  const [excerpt, setExcerpt] = useState(article?.excerpt ?? '');
  const [categoryId, setCategoryId] = useState(article?.category_id ?? '');
  const [selectedTagIds, setSelectedTagIds] = useState<string[]>(
    article?.tags?.map((t) => t.id) ?? [],
  );
  const [coverImageUrl, setCoverImageUrl] = useState(article?.cover_image_url ?? '');
  const [contentJson, setContentJson] = useState<Record<string, unknown>>(
    article?.content_json ?? { type: 'doc', content: [] },
  );
  const [contentHtml, setContentHtml] = useState(article?.content_html ?? '');

  // SEO & Scheduling
  const [seoOpen, setSeoOpen] = useState(false);
  const [metaTitle, setMetaTitle] = useState(article?.seo?.meta_title ?? '');
  const [metaDescription, setMetaDescription] = useState(article?.seo?.meta_description ?? '');
  const [canonicalUrl, setCanonicalUrl] = useState(article?.seo?.canonical_url ?? '');
  const [ogImageUrl, setOgImageUrl] = useState(article?.seo?.og_image_url ?? '');
  const [noindex, setNoindex] = useState(Boolean(article?.seo?.noindex));
  const [scheduledFor, setScheduledFor] = useState(
    article?.scheduled_for ? article.scheduled_for.slice(0, 16) : '',
  );

  // Revisions
  const [revisionsOpen, setRevisionsOpen] = useState(false);
  const [revisions, setRevisions] = useState<ArticleRevision[]>([]);
  const [revisionsLoading, setRevisionsLoading] = useState(false);

  const [saving, setSaving] = useState(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);

  useEffect(() => {
    if (!article || !revisionsOpen) return;
    let active = true;
    setRevisionsLoading(true);
    bffClient
      .listArticleRevisions(article.id)
      .then((list) => {
        if (active) setRevisions(list);
      })
      .catch(() => {
        // handled by global notifier
      })
      .finally(() => {
        if (active) setRevisionsLoading(false);
      });
    return () => {
      active = false;
    };
  }, [article, revisionsOpen]);

  const toggleTag = (tagId: string) => {
    setSelectedTagIds((prev) =>
      prev.includes(tagId) ? prev.filter((id) => id !== tagId) : [...prev, tagId],
    );
  };

  const buildPayload = () => ({
    title: title.trim(),
    slug: slug.trim() || undefined,
    excerpt: excerpt.trim() || null,
    category_id: categoryId || null,
    tag_ids: selectedTagIds,
    cover_image_url: coverImageUrl.trim() || null,
    content_json: contentJson,
    content_html: contentHtml,
    ...(article ? {} : { author_name_snapshot: authorName }),
    seo: {
      meta_title: metaTitle.trim() || null,
      meta_description: metaDescription.trim() || null,
      canonical_url: canonicalUrl.trim() || null,
      og_image_url: ogImageUrl.trim() || null,
      noindex,
    },
  });

  const handleSave = async (e: FormEvent) => {
    e.preventDefault();
    if (!title.trim() || isLockedForStaff) return;

    setSaving(true);
    try {
      if (article) {
        await bffClient.updateArticle(article.id, buildPayload());
        toast.success('Artículo guardado');
      } else {
        await bffClient.createArticle(buildPayload());
        toast.success('Borrador creado');
      }
      onSaved();
      onClose();
    } catch {
      // global toast handles error
    } finally {
      setSaving(false);
    }
  };

  const handlePublishNow = async () => {
    if (!article || !canPublish) return;
    setBusyAction('publish');
    try {
      await bffClient.updateArticleStatus(article.id, { status: 'PUBLISHED' });
      toast.success('Artículo publicado');
      onSaved();
      onClose();
    } catch {
      // handled
    } finally {
      setBusyAction(null);
    }
  };

  const handleSchedule = async () => {
    if (!article || !canPublish || !scheduledFor) return;
    setBusyAction('schedule');
    try {
      const iso = new Date(scheduledFor).toISOString();
      await bffClient.updateArticleStatus(article.id, {
        status: 'SCHEDULED',
        scheduled_for: iso,
      });
      toast.success('Publicación programada');
      onSaved();
      onClose();
    } catch {
      // handled
    } finally {
      setBusyAction(null);
    }
  };

  const handlePreview = async () => {
    if (!article) return;
    setBusyAction('preview');
    try {
      const res = await bffClient.generateArticlePreviewToken(article.id);
      await navigator.clipboard.writeText(res.token).catch(() => {});
      toast.info(`Ruta: ${res.preview_path} (token copiado al portapapeles)`, {
        title: 'Vista previa generada (30 min)',
      });
    } catch {
      // handled
    } finally {
      setBusyAction(null);
    }
  };

  const handleRestoreRevision = async (revisionId: string) => {
    if (!article || !canPublish) return;
    setBusyAction(revisionId);
    try {
      await bffClient.restoreArticleRevision(article.id, revisionId);
      toast.success('Revisión restaurada');
      onSaved();
      onClose();
    } catch {
      // handled
    } finally {
      setBusyAction(null);
    }
  };

  return (
    <form className="stack-md" onSubmit={handleSave}>
      {isLockedForStaff && (
        <div className="content-notice">
          Este artículo está en estado <strong>{article?.status}</strong>. Solo un administrador
          (`content:publish`) puede editar artículos publicados o programados.
        </div>
      )}

      <Input
        label="Título"
        required
        disabled={isLockedForStaff}
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Ej: Cómo escalar arquitectura multi-tenant en Rust"
      />

      <div className="field">
        <span className="field-label">Contenido editorial</span>
        <RichTextEditor
          initialContentJson={article?.content_json}
          initialContentHtml={article?.content_html}
          disabled={isLockedForStaff}
          onChange={({ json, html }) => {
            setContentJson(json);
            setContentHtml(html);
          }}
        />
      </div>

      {/* Collapsible SEO & Metadata Panel */}
      <div className="content-collapsible">
        <button
          type="button"
          className="content-collapsible-trigger"
          onClick={() => setSeoOpen((v) => !v)}
        >
          <span>SEO, Metadatos y Portada</span>
          {seoOpen ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
        </button>

        {seoOpen && (
          <div className="content-collapsible-body stack-sm">
            <div className="form-grid-2">
              <Input
                label="Slug (URL)"
                disabled={isLockedForStaff}
                value={slug}
                onChange={(e) => setSlug(e.target.value)}
                placeholder="se-genera-automaticamente"
              />
              <Select
                label="Categoría"
                disabled={isLockedForStaff}
                value={categoryId}
                onChange={(e) => setCategoryId(e.target.value)}
              >
                <option value="">Sin categoría</option>
                {categories.map((cat) => (
                  <option key={cat.id} value={cat.id}>
                    {cat.name}
                  </option>
                ))}
              </Select>
            </div>

            <Textarea
              label="Extracto"
              rows={2}
              disabled={isLockedForStaff}
              value={excerpt}
              onChange={(e) => setExcerpt(e.target.value)}
              placeholder="Resumen breve para tarjetas y listados de blog..."
            />

            {tags.length > 0 && (
              <div className="field">
                <span className="field-label">Etiquetas</span>
                <div className="filter-pills">
                  {tags.map((tag) => {
                    const active = selectedTagIds.includes(tag.id);
                    return (
                      <button
                        key={tag.id}
                        type="button"
                        disabled={isLockedForStaff}
                        className={`pill-btn ${active ? 'active' : ''}`}
                        onClick={() => toggleTag(tag.id)}
                      >
                        #{tag.name}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}

            <div className="stack-xs">
              <div className="row-sm">
                <Input
                  label="Imagen de portada"
                  disabled={isLockedForStaff}
                  value={coverImageUrl}
                  onChange={(e) => setCoverImageUrl(e.target.value)}
                  placeholder="https://media.ejemplo.com/portada.webp"
                />
                {!isLockedForStaff && (
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    icon={<ImageIcon size={14} />}
                    onClick={() =>
                      openMediaPicker({
                        context: 'content',
                        title: 'Elegir imagen de portada',
                        onSelect: (url) => setCoverImageUrl(url),
                      })
                    }
                  >
                    Galería
                  </Button>
                )}
              </div>
            </div>

            <div className="form-grid-2">
              <Input
                label="Meta Title (SEO)"
                disabled={isLockedForStaff}
                value={metaTitle}
                onChange={(e) => setMetaTitle(e.target.value)}
                maxLength={160}
              />
              <Input
                label="Canonical URL"
                disabled={isLockedForStaff}
                value={canonicalUrl}
                onChange={(e) => setCanonicalUrl(e.target.value)}
                placeholder="https://mi-sitio.com/blog/..."
              />
            </div>

            <Textarea
              label="Meta Description (SEO)"
              rows={2}
              disabled={isLockedForStaff}
              value={metaDescription}
              onChange={(e) => setMetaDescription(e.target.value)}
              maxLength={320}
            />

            <Input
              label="OG Image URL"
              disabled={isLockedForStaff}
              value={ogImageUrl}
              onChange={(e) => setOgImageUrl(e.target.value)}
              placeholder="https://..."
            />

            <label className="row-sm text-sm">
              <input
                type="checkbox"
                disabled={isLockedForStaff}
                checked={noindex}
                onChange={(e) => setNoindex(e.target.checked)}
              />
              <span>No indexar en buscadores (`noindex`)</span>
            </label>

            {article && canPublish && (
              <div className="row-sm">
                <Input
                  label="Programar publicación"
                  type="datetime-local"
                  value={scheduledFor}
                  onChange={(e) => setScheduledFor(e.target.value)}
                />
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  icon={<Clock size={14} />}
                  disabled={!scheduledFor || busyAction !== null}
                  onClick={handleSchedule}
                >
                  Programar
                </Button>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Revisions History */}
      {article && (
        <div className="content-collapsible">
          <button
            type="button"
            className="content-collapsible-trigger"
            onClick={() => setRevisionsOpen((v) => !v)}
          >
            <span className="row-xs">
              <History size={15} />
              <span>Historial de Revisiones</span>
            </span>
            {revisionsOpen ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
          </button>
          {revisionsOpen && (
            <div className="content-collapsible-body stack-xs">
              {revisionsLoading ? (
                <p className="text-sm text-muted">Cargando revisiones...</p>
              ) : revisions.length === 0 ? (
                <p className="text-sm text-muted">
                  Aún no hay revisiones guardadas. Se crea una versión automáticamente al editar un
                  artículo publicado.
                </p>
              ) : (
                revisions.map((rev) => (
                  <div key={rev.id} className="revision-row">
                    <div>
                      <div className="row-xs">
                        <StatusBadge label={`v${rev.version_number}`} tone="blue" />
                        <strong className="text-sm">{rev.title}</strong>
                      </div>
                      <span className="text-xs text-muted">{formatDateTime(rev.created_at)}</span>
                    </div>
                    {canPublish && (
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        icon={<RotateCcw size={13} />}
                        disabled={busyAction !== null}
                        onClick={() => handleRestoreRevision(rev.id)}
                      >
                        Restaurar
                      </Button>
                    )}
                  </div>
                ))
              )}
            </div>
          )}
        </div>
      )}

      {/* Action bar */}
      <div className="row-between wrap gap-sm">
        <div className="row-xs">
          {article && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              icon={<Eye size={14} />}
              disabled={busyAction !== null}
              onClick={handlePreview}
            >
              Vista previa
            </Button>
          )}
          {article && canPublish && article.status !== 'PUBLISHED' && (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              icon={<Send size={14} />}
              disabled={busyAction !== null}
              onClick={handlePublishNow}
            >
              Publicar ahora
            </Button>
          )}
        </div>

        <div className="row-xs">
          <Button type="button" variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          {!isLockedForStaff && (
            <Button
              type="submit"
              variant="primary"
              icon={<Save size={14} />}
              isLoading={saving}
              disabled={!title.trim() || saving}
            >
              {article ? 'Guardar cambios' : 'Crear borrador'}
            </Button>
          )}
        </div>
      </div>
    </form>
  );
}
