import { Play, Plus, Trash2, Webhook } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { bffClient } from '../../api/bffClient';
import type { PublishWebhook } from '../../types/api';
import {
  Button,
  EmptyState,
  Input,
  StatusBadge,
  formatDateTime,
  useDrawer,
  useModal,
  useToast,
} from '../../ui';

export interface WebhooksTabProps {
  webhooks: PublishWebhook[];
  loading: boolean;
  onRefresh: () => void;
}

const ALL_EVENTS = ['article.published', 'article.updated', 'article.unpublished'];

function WebhookForm({
  onSaved,
  onClose,
}: {
  onSaved: () => void;
  onClose: () => void;
}) {
  const { toast } = useToast();
  const [name, setName] = useState('');
  const [targetUrl, setTargetUrl] = useState('');
  const [events, setEvents] = useState<string[]>([...ALL_EVENTS]);
  const [saving, setSaving] = useState(false);

  const toggleEvent = (ev: string) => {
    setEvents((prev) => (prev.includes(ev) ? prev.filter((e) => e !== ev) : [...prev, ev]));
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !targetUrl.trim() || events.length === 0) return;
    setSaving(true);
    try {
      await bffClient.createContentWebhook({
        name: name.trim(),
        target_url: targetUrl.trim(),
        events,
        is_active: true,
      });
      toast.success('Deploy Hook creado');
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
        placeholder="Ej: Cloudflare Pages Producción"
      />
      <Input
        label="URL del Deploy Hook (solo https:// público)"
        required
        value={targetUrl}
        onChange={(e) => setTargetUrl(e.target.value)}
        placeholder="https://api.cloudflare.com/client/v4/pages/webhooks/deploy_hooks/..."
      />
      <div className="field">
        <span className="field-label">Eventos que disparan el rebuild</span>
        <div className="stack-xs">
          {ALL_EVENTS.map((ev) => (
            <label key={ev} className="checkbox-row">
              <input
                type="checkbox"
                checked={events.includes(ev)}
                onChange={() => toggleEvent(ev)}
              />
              <code>{ev}</code>
            </label>
          ))}
        </div>
      </div>
      <div className="row-end gap-sm">
        <Button type="button" variant="ghost" onClick={onClose}>
          Cancelar
        </Button>
        <Button type="submit" variant="primary" isLoading={saving} disabled={events.length === 0}>
          Crear Deploy Hook
        </Button>
      </div>
    </form>
  );
}

export function WebhooksTab({ webhooks, loading, onRefresh }: WebhooksTabProps) {
  const { openDrawer, closeDrawer } = useDrawer();
  const { confirm } = useModal();
  const { toast } = useToast();
  const [busyRowId, setBusyRowId] = useState<string | null>(null);

  const openCreate = () => {
    openDrawer({
      title: 'Nuevo Deploy Hook (SSG)',
      subtitle: 'Reconstruye tu sitio Astro, Cloudflare Pages, Vercel o Netlify al publicar.',
      size: 'sm',
      content: <WebhookForm onSaved={onRefresh} onClose={() => closeDrawer(true)} />,
    });
  };

  const handleTest = async (hook: PublishWebhook) => {
    setBusyRowId(hook.id);
    try {
      const res = await bffClient.testContentWebhook(hook.id);
      if (res.ok) {
        toast.success(`HTTP ${res.status_code}`, {
          title: 'Deploy Hook respondió correctamente',
        });
      } else {
        toast.error(`HTTP ${res.status_code || 'sin respuesta'}`, {
          title: 'El Deploy Hook devolvió error',
        });
      }
      onRefresh();
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  const handleDelete = async (hook: PublishWebhook) => {
    const ok = await confirm({
      title: 'Eliminar Deploy Hook',
      message: `¿Eliminar "${hook.name}"?`,
      variant: 'danger',
      confirmLabel: 'Eliminar',
    });
    if (!ok) return;
    setBusyRowId(hook.id);
    try {
      await bffClient.deleteContentWebhook(hook.id);
      toast.success('Deploy Hook eliminado');
      onRefresh();
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  return (
    <div className="stack-md">
      <div className="row-between row-wrap gap-sm">
        <div>
          <h3 className="card-title">Deploy Hooks (SSG)</h3>
          <p className="card-subtitle">
            Notifica automáticamente a Cloudflare Pages, Vercel o Netlify cuando cambia el contenido.
          </p>
        </div>
        <Button variant="primary" icon={<Plus size={15} />} onClick={openCreate}>
          Nuevo Deploy Hook
        </Button>
      </div>

      {!loading && webhooks.length === 0 ? (
        <EmptyState
          icon={<Webhook size={28} />}
          title="Sin Deploy Hooks configurados"
          description="Agrega una URL HTTPS de Cloudflare Pages, Vercel o Netlify para automatizar tus despliegues SSG."
        />
      ) : (
        <div className="stack-sm">
          {webhooks.map((hook) => (
            <div key={hook.id} className="card">
              <div className="row-between row-wrap gap-sm">
                <div className="stack-xs min-w-0">
                  <div className="row-xs row-wrap">
                    <strong>{hook.name}</strong>
                    <StatusBadge
                      label={hook.is_active ? 'Activo' : 'Inactivo'}
                      tone={hook.is_active ? 'emerald' : 'gray'}
                    />
                    {hook.last_status_code !== null && (
                      <StatusBadge
                        label={`HTTP ${hook.last_status_code}`}
                        tone={
                          hook.last_status_code >= 200 && hook.last_status_code < 300
                            ? 'green'
                            : 'red'
                        }
                      />
                    )}
                  </div>
                  <code className="text-xs text-muted truncate">{hook.target_url}</code>
                  <div className="chip-list">
                    {hook.events.map((ev) => (
                      <StatusBadge key={ev} label={ev} tone="blue" />
                    ))}
                    {hook.last_triggered_at && (
                      <span className="text-xs text-muted">
                        Último disparo: {formatDateTime(hook.last_triggered_at)}
                      </span>
                    )}
                  </div>
                </div>

                <div className="row-xs">
                  <Button
                    size="sm"
                    variant="secondary"
                    icon={<Play size={13} />}
                    isLoading={busyRowId === hook.id}
                    disabled={busyRowId !== null}
                    onClick={() => handleTest(hook)}
                  >
                    Probar
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<Trash2 size={13} />}
                    disabled={busyRowId !== null}
                    onClick={() => handleDelete(hook)}
                    aria-label={`Eliminar ${hook.name}`}
                  />
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
