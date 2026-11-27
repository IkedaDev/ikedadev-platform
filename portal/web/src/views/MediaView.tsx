import { Check, Copy, HardDrive, ImagePlus, Trash2, Upload } from 'lucide-react';
import { useCallback, useEffect, useRef, useState, type ChangeEvent } from 'react';
import { bffClient } from '../api/bffClient';
import type { AuthUser, MediaAsset, MediaContext, MediaQuotaUsage } from '../types/api';
import { Button, EmptyState, Input, Select, StatusBadge, useModal, useToast } from '../ui';

const ALLOWED_MIME = ['image/webp', 'image/jpeg', 'image/png', 'image/avif'];

const CONTEXT_LABELS: Record<MediaContext | '', string> = {
  '': 'Todos',
  general: 'General',
  branding: 'Branding',
  catalog: 'Catálogo',
  content: 'Blog / Contenido',
  avatars: 'Avatares',
};

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export interface MediaViewProps {
  user: AuthUser;
}

export function MediaView({ user }: MediaViewProps) {
  const { toast } = useToast();
  const { confirm } = useModal();
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const isTenantAdmin = user.roles.some(
    (r) => r === 'tenant:admin' || r === 'platform:admin',
  );
  const canDelete = isTenantAdmin || user.roles.includes('media:delete');

  const [contextFilter, setContextFilter] = useState<MediaContext | ''>('');
  const [uploadContext, setUploadContext] = useState<MediaContext>('general');
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [quota, setQuota] = useState<MediaQuotaUsage | null>(null);
  const [loading, setLoading] = useState(true);
  const [uploading, setUploading] = useState(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [editingAltId, setEditingAltId] = useState<string | null>(null);
  const [altDraft, setAltDraft] = useState('');
  const [busyRowId, setBusyRowId] = useState<string | null>(null);

  const loadAll = useCallback(async () => {
    setLoading(true);
    try {
      const [listRes, quotaRes] = await Promise.all([
        bffClient.listMediaAssets({ context: contextFilter, page: 1, page_size: 48 }),
        bffClient.getMediaQuota(),
      ]);
      setAssets(listRes.data);
      setQuota(quotaRes);
    } catch {
      // handled globally
    } finally {
      setLoading(false);
    }
  }, [contextFilter]);

  useEffect(() => {
    void loadAll();
  }, [loadAll]);

  const handleFileChange = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;

    if (!ALLOWED_MIME.includes(file.type)) {
      toast.error('Solo se permiten imágenes WebP, JPEG, PNG o AVIF.', {
        title: 'Formato no soportado',
      });
      return;
    }

    const maxFile = quota?.max_file_size_bytes ?? 5 * 1024 * 1024;
    if (file.size > maxFile) {
      toast.error(`El tamaño máximo permitido es ${formatBytes(maxFile)}.`, {
        title: 'Archivo demasiado grande',
      });
      return;
    }

    setUploading(true);
    try {
      const presigned = await bffClient.presignMediaUpload({
        file_name: file.name,
        mime_type: file.type,
        size_bytes: file.size,
        context: uploadContext,
      });

      const uploadRes = await fetch(presigned.upload_url, {
        method: 'PUT',
        headers: { 'Content-Type': file.type },
        body: file,
      });
      if (!uploadRes.ok) {
        throw new Error(`Error al subir archivo al almacenamiento (${uploadRes.status})`);
      }

      await bffClient.confirmMediaUpload({
        asset_id: presigned.asset_id,
        alt_text: file.name.replace(/\.[^.]+$/, ''),
      });

      toast.success('Imagen agregada a la Biblioteca de Medios');
      await loadAll();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Error inesperado', {
        title: 'No se pudo subir la imagen',
      });
    } finally {
      setUploading(false);
    }
  };

  const handleCopyUrl = async (asset: MediaAsset) => {
    await navigator.clipboard.writeText(asset.public_url).catch(() => {});
    setCopiedId(asset.id);
    toast.info('URL copiada al portapapeles');
    setTimeout(() => setCopiedId((prev) => (prev === asset.id ? null : prev)), 1500);
  };

  const handleSaveAlt = async (asset: MediaAsset) => {
    setBusyRowId(asset.id);
    try {
      await bffClient.updateMediaAsset(asset.id, { alt_text: altDraft.trim() || null });
      setEditingAltId(null);
      toast.success('Texto alternativo actualizado');
      await loadAll();
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  const handleDelete = async (asset: MediaAsset, permanent: boolean) => {
    if (!canDelete) return;
    const ok = await confirm({
      title: permanent ? 'Eliminar permanentemente' : 'Ocultar de la galería',
      message: permanent
        ? `¿Eliminar definitivamente "${asset.file_name}" del almacenamiento? ADVERTENCIA: Cualquier artículo, producto o logo que referencie esta URL dejará de cargar la imagen.`
        : `¿Ocultar "${asset.file_name}" de la galería? Se mantiene un período de gracia en el almacenamiento para no romper referencias existentes.`,
      variant: 'danger',
      confirmLabel: permanent ? 'Purgar definitivamente' : 'Ocultar',
    });
    if (!ok) return;

    setBusyRowId(asset.id);
    try {
      await bffClient.deleteMediaAsset(asset.id, permanent);
      toast.success(permanent ? 'Archivo purgado definitivamente' : 'Archivo ocultado de la galería');
      await loadAll();
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  const uploadContexts: MediaContext[] = isTenantAdmin
    ? ['general', 'content', 'catalog', 'branding', 'avatars']
    : ['general', 'content', 'catalog', 'avatars'];

  return (
    <div className="stack-lg">
      <div className="page-header row-between wrap gap-sm">
        <div>
          <h1 className="page-title">Biblioteca de Medios</h1>
          <p className="page-subtitle">
            Almacenamiento transversal multi-tenant para Blog, Catálogo, Branding y Avatares.
          </p>
        </div>

        <div className="row-sm wrap">
          <Select
            value={uploadContext}
            onChange={(e) => setUploadContext(e.target.value as MediaContext)}
            aria-label="Contexto de subida"
          >
            {uploadContexts.map((ctx) => (
              <option key={ctx} value={ctx}>
                Subir a: {CONTEXT_LABELS[ctx]}
              </option>
            ))}
          </Select>

          <input
            ref={fileInputRef}
            type="file"
            accept="image/webp,image/jpeg,image/png,image/avif"
            style={{ display: 'none' }}
            onChange={handleFileChange}
          />
          <Button
            variant="primary"
            icon={<Upload size={15} />}
            isLoading={uploading}
            onClick={() => fileInputRef.current?.click()}
          >
            Subir imagen
          </Button>
        </div>
      </div>

      {/* Quota Card */}
      {quota && (
        <div className="card">
          <div className="row-between wrap gap-sm" style={{ marginBottom: 8 }}>
            <div className="row-sm">
              <HardDrive size={16} />
              <strong className="text-sm">Cuota de almacenamiento del tenant</strong>
              <StatusBadge label={`${quota.asset_count} archivos`} tone="blue" />
            </div>
            <span className="text-sm text-muted">
              {formatBytes(quota.used_bytes)} / {formatBytes(quota.max_bytes)} ({quota.usage_percent}%)
            </span>
          </div>
          <div
            style={{
              height: 8,
              borderRadius: 999,
              background: 'var(--color-bg-subtle)',
              overflow: 'hidden',
              border: '1px solid var(--color-border)',
            }}
          >
            <div
              style={{
                width: `${Math.min(100, quota.usage_percent)}%`,
                height: '100%',
                background:
                  quota.usage_percent > 85 ? 'var(--color-danger)' : 'var(--color-primary)',
                transition: 'width 0.2s ease',
              }}
            />
          </div>
        </div>
      )}

      {/* Context filter pills (Kropy standard) */}
      <div className="filter-pills">
        {(['', 'general', 'content', 'catalog', 'branding', 'avatars'] as const).map((ctx) => (
          <button
            key={ctx || 'ALL'}
            type="button"
            className={`pill-btn ${contextFilter === ctx ? 'active' : ''}`}
            onClick={() => setContextFilter(ctx)}
          >
            {CONTEXT_LABELS[ctx]}
          </button>
        ))}
      </div>

      {!loading && assets.length === 0 ? (
        <EmptyState
          icon={<ImagePlus size={28} />}
          title="No hay archivos en esta vista"
          description="Sube tu primera imagen para utilizarla en tus artículos, catálogo o branding."
        />
      ) : (
        <div className="media-grid">
          {assets.map((asset) => {
            const canEditAsset = asset.context !== 'branding' || isTenantAdmin;
            return (
              <div key={asset.id} className="media-card">
                <div className="media-card-preview">
                  <img
                    src={asset.public_url}
                    alt={asset.alt_text || asset.file_name}
                    loading="lazy"
                    referrerPolicy="no-referrer"
                  />
                </div>
                <div className="media-card-body stack-xs">
                  <div className="row-between gap-xs">
                    <strong className="text-sm truncate" title={asset.file_name}>
                      {asset.file_name}
                    </strong>
                    <StatusBadge label={asset.context} tone="gray" />
                  </div>
                  <span className="text-xs text-muted">
                    {asset.storage_provider} · {formatBytes(asset.size_bytes)}
                  </span>

                  {editingAltId === asset.id ? (
                    <div className="row-xs">
                      <Input
                        value={altDraft}
                        onChange={(e) => setAltDraft(e.target.value)}
                        placeholder="Texto alternativo (alt)"
                      />
                      <Button
                        size="sm"
                        variant="primary"
                        disabled={busyRowId === asset.id}
                        onClick={() => handleSaveAlt(asset)}
                      >
                        OK
                      </Button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      className="media-alt-btn"
                      disabled={!canEditAsset}
                      onClick={() => {
                        if (!canEditAsset) return;
                        setEditingAltId(asset.id);
                        setAltDraft(asset.alt_text ?? '');
                      }}
                    >
                      Alt: {asset.alt_text || (canEditAsset ? 'Agregar descripción...' : 'Solo admin')}
                    </button>
                  )}

                  <div className="row-between">
                    <Button
                      size="sm"
                      variant="ghost"
                      icon={copiedId === asset.id ? <Check size={13} /> : <Copy size={13} />}
                      onClick={() => handleCopyUrl(asset)}
                    >
                      Copiar URL
                    </Button>
                    {canDelete && (
                      <div className="row-xs">
                        <Button
                          size="sm"
                          variant="ghost"
                          icon={<Trash2 size={13} />}
                          disabled={busyRowId === asset.id}
                          onClick={() => handleDelete(asset, false)}
                          title="Ocultar (Soft Delete)"
                        />
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={busyRowId === asset.id}
                          onClick={() => handleDelete(asset, true)}
                          title="Purgar definitivamente de almacenamiento"
                        >
                          Purgar
                        </Button>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
