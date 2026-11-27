import {
  Archive,
  Eye,
  FilePlus2,
  FileText,
  FolderTree,
  Pencil,
  Send,
  Trash2,
  Webhook,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { bffClient } from '../api/bffClient';
import { ArticleEditorDrawer } from '../components/content/ArticleEditorDrawer';
import { TaxonomyTab } from '../components/content/TaxonomyTab';
import { WebhooksTab } from '../components/content/WebhooksTab';
import type {
  Article,
  ArticleStatus,
  AuthUser,
  ContentCategory,
  ContentTag,
  PublishWebhook,
} from '../types/api';
import {
  Button,
  DataTable,
  StatusBadge,
  formatDateTime,
  useDrawer,
  useModal,
  useToast,
  type DataTableColumn,
  type StatusTone,
} from '../ui';

type ContentTab = 'articles' | 'taxonomy' | 'webhooks';

const STATUS_BADGE: Record<ArticleStatus, { label: string; tone: StatusTone }> = {
  DRAFT: { label: 'Borrador', tone: 'gray' },
  SCHEDULED: { label: 'Programado', tone: 'blue' },
  PUBLISHED: { label: 'Publicado', tone: 'emerald' },
  ARCHIVED: { label: 'Archivado', tone: 'amber' },
};

export interface ContentViewProps {
  user: AuthUser;
}

export function ContentView({ user }: ContentViewProps) {
  const { openDrawer, closeDrawer } = useDrawer();
  const { confirm } = useModal();
  const { toast } = useToast();

  const canPublish = user.roles.some(
    (r) => r === 'content:publish' || r === 'tenant:admin' || r === 'platform:admin',
  );

  const [tab, setTab] = useState<ContentTab>('articles');
  const [articles, setArticles] = useState<Article[]>([]);
  const [articlesLoading, setArticlesLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState<ArticleStatus | ''>('');
  const [searchQuery, setSearchQuery] = useState('');
  const [page, setPage] = useState(1);
  const [totalPages, setTotalPages] = useState(1);
  const [total, setTotal] = useState(0);

  const [categories, setCategories] = useState<ContentCategory[]>([]);
  const [tags, setTags] = useState<ContentTag[]>([]);
  const [webhooks, setWebhooks] = useState<PublishWebhook[]>([]);
  const [webhooksLoading, setWebhooksLoading] = useState(false);

  const [busyRowId, setBusyRowId] = useState<string | null>(null);

  const loadArticles = useCallback(async () => {
    setArticlesLoading(true);
    try {
      const res = await bffClient.listArticles({
        page,
        limit: 15,
        status: statusFilter,
        q: searchQuery.trim() || undefined,
      });
      setArticles(res.data);
      setTotalPages(Math.max(1, res.pagination.total_pages));
      setTotal(res.pagination.total_items);
    } catch {
      // handled globally
    } finally {
      setArticlesLoading(false);
    }
  }, [page, statusFilter, searchQuery]);

  const loadTaxonomy = useCallback(async () => {
    try {
      const [cats, tgs] = await Promise.all([
        bffClient.listContentCategories(),
        bffClient.listContentTags(),
      ]);
      setCategories(cats);
      setTags(tgs);
    } catch {
      // handled
    }
  }, []);

  const loadWebhooks = useCallback(async () => {
    if (!canPublish) return;
    setWebhooksLoading(true);
    try {
      const list = await bffClient.listContentWebhooks();
      setWebhooks(list);
    } catch {
      // handled
    } finally {
      setWebhooksLoading(false);
    }
  }, [canPublish]);

  useEffect(() => {
    void loadArticles();
  }, [loadArticles]);

  useEffect(() => {
    void loadTaxonomy();
  }, [loadTaxonomy]);

  useEffect(() => {
    if (tab === 'webhooks' && canPublish) {
      void loadWebhooks();
    }
  }, [tab, canPublish, loadWebhooks]);

  const openArticleEditor = (article: Article | null) => {
    openDrawer({
      title: article ? `Editar: ${article.title}` : 'Nuevo artículo',
      subtitle: article
        ? `/${article.slug} · ${article.word_count} palabras · ${article.reading_time_minutes} min`
        : 'Crea un nuevo borrador con el editor visual TipTap',
      size: 'lg',
      content: (
        <ArticleEditorDrawer
          article={article}
          categories={categories}
          tags={tags}
          authorName={user.name || user.email}
          canPublish={canPublish}
          onSaved={() => void loadArticles()}
          onClose={() => closeDrawer(true)}
        />
      ),
    });
  };

  const handleStatusChange = async (article: Article, nextStatus: ArticleStatus) => {
    if (!canPublish) return;
    setBusyRowId(article.id);
    try {
      await bffClient.updateArticleStatus(article.id, { status: nextStatus });
      toast.success(
        nextStatus === 'PUBLISHED'
          ? 'Artículo publicado'
          : nextStatus === 'ARCHIVED'
            ? 'Artículo archivado'
            : 'Estado actualizado',
      );
      await loadArticles();
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  const handlePreview = async (article: Article) => {
    setBusyRowId(article.id);
    try {
      const res = await bffClient.generateArticlePreviewToken(article.id);
      await navigator.clipboard.writeText(res.token).catch(() => {});
      toast.info(`Token copiado. Ruta: ${res.preview_path}`, {
        title: 'Vista previa generada (30 min)',
      });
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  const handleDeleteArticle = async (article: Article) => {
    if (!canPublish) return;
    const ok = await confirm({
      title: 'Eliminar artículo',
      message: `¿Eliminar "${article.title}"? Esta acción realiza un borrado lógico y notificará a tus Deploy Hooks si estaba publicado.`,
      variant: 'danger',
      confirmLabel: 'Eliminar',
    });
    if (!ok) return;
    setBusyRowId(article.id);
    try {
      await bffClient.deleteArticle(article.id);
      toast.success('Artículo eliminado');
      await loadArticles();
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  const columns: DataTableColumn<Article>[] = [
    {
      key: 'title',
      header: 'Artículo',
      render: (row) => (
        <div className="stack-xs" style={{ minWidth: 0 }}>
          <strong className="truncate">{row.title}</strong>
          <span className="text-xs text-muted truncate">
            /{row.slug}
            {row.category ? ` · ${row.category.name}` : ''}
            {` · ${row.reading_time_minutes} min`}
          </span>
        </div>
      ),
    },
    {
      key: 'status',
      header: 'Estado',
      render: (row) => {
        const badge = STATUS_BADGE[row.status];
        return <StatusBadge label={badge.label} tone={badge.tone} />;
      },
    },
    {
      key: 'updated_at',
      header: 'Actualizado',
      hideOnMobile: true,
      render: (row) => <span className="text-sm text-muted">{formatDateTime(row.updated_at)}</span>,
    },
    {
      key: 'actions',
      header: 'Acciones',
      align: 'right',
      render: (row) => {
        const isBusy = busyRowId === row.id;
        const canEditRow = row.status === 'DRAFT' || canPublish;
        return (
          <div className="row-xs row-end">
            <Button
              size="sm"
              variant="ghost"
              icon={<Pencil size={14} />}
              disabled={isBusy}
              onClick={() => openArticleEditor(row)}
              title={canEditRow ? 'Editar' : 'Ver artículo'}
            />
            <Button
              size="sm"
              variant="ghost"
              icon={<Eye size={14} />}
              disabled={isBusy}
              onClick={() => handlePreview(row)}
              title="Generar token de vista previa"
            />
            {canPublish && row.status !== 'PUBLISHED' && (
              <Button
                size="sm"
                variant="ghost"
                icon={<Send size={14} />}
                disabled={isBusy}
                onClick={() => handleStatusChange(row, 'PUBLISHED')}
                title="Publicar"
              />
            )}
            {canPublish && row.status === 'PUBLISHED' && (
              <Button
                size="sm"
                variant="ghost"
                icon={<Archive size={14} />}
                disabled={isBusy}
                onClick={() => handleStatusChange(row, 'ARCHIVED')}
                title="Archivar"
              />
            )}
            {canPublish && (
              <Button
                size="sm"
                variant="ghost"
                icon={<Trash2 size={14} />}
                disabled={isBusy}
                onClick={() => handleDeleteArticle(row)}
                title="Eliminar"
              />
            )}
          </div>
        );
      },
    },
  ];

  return (
    <div className="stack-lg">
      <div className="page-header row-between wrap gap-sm">
        <div>
          <h1 className="page-title">Contenido / Blog</h1>
          <p className="page-subtitle">
            Headless CMS multi-tenant con editor visual, vista previa SSR y Deploy Hooks SSG.
          </p>
        </div>
        {tab === 'articles' && (
          <Button
            variant="primary"
            icon={<FilePlus2 size={16} />}
            onClick={() => openArticleEditor(null)}
          >
            Nuevo artículo
          </Button>
        )}
      </div>

      {/* Tabs */}
      <div className="content-tabs" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'articles'}
          className={`content-tab ${tab === 'articles' ? 'is-active' : ''}`}
          onClick={() => setTab('articles')}
        >
          <FileText size={15} />
          <span>Artículos</span>
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'taxonomy'}
          className={`content-tab ${tab === 'taxonomy' ? 'is-active' : ''}`}
          onClick={() => setTab('taxonomy')}
        >
          <FolderTree size={15} />
          <span>Categorías y Etiquetas</span>
        </button>
        {canPublish && (
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'webhooks'}
            className={`content-tab ${tab === 'webhooks' ? 'is-active' : ''}`}
            onClick={() => setTab('webhooks')}
          >
            <Webhook size={15} />
            <span>Deploy Hooks (SSG)</span>
          </button>
        )}
      </div>

      {tab === 'articles' && (
        <DataTable
          columns={columns}
          data={articles}
          rowKey={(row) => row.id}
          isLoading={articlesLoading}
          searchValue={searchQuery}
          onSearchChange={(val) => {
            setPage(1);
            setSearchQuery(val);
          }}
          searchPlaceholder="Buscar artículos por título..."
          page={page}
          totalPages={totalPages}
          total={total}
          onPageChange={setPage}
          emptyTitle="Aún no hay artículos"
          emptyDescription="Crea tu primer borrador editorial con el editor visual TipTap."
          toolbar={
            <div className="filter-pills">
              {(['', 'DRAFT', 'SCHEDULED', 'PUBLISHED', 'ARCHIVED'] as const).map((st) => (
                <button
                  key={st || 'ALL'}
                  type="button"
                  className={`pill-btn ${statusFilter === st ? 'active' : ''}`}
                  onClick={() => {
                    setPage(1);
                    setStatusFilter(st);
                  }}
                >
                  {st === '' ? 'Todos' : STATUS_BADGE[st].label}
                </button>
              ))}
            </div>
          }
        />
      )}

      {tab === 'taxonomy' && (
        <TaxonomyTab
          categories={categories}
          tags={tags}
          onRefresh={() => void loadTaxonomy()}
        />
      )}

      {tab === 'webhooks' && canPublish && (
        <WebhooksTab
          webhooks={webhooks}
          loading={webhooksLoading}
          onRefresh={() => void loadWebhooks()}
        />
      )}
    </div>
  );
}
