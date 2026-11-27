import { Check, Copy, ImagePlus, Trash2, Upload } from 'lucide-react';
import { useRef, useState, type ChangeEvent } from 'react';
import { bffClient } from '../../api/bffClient';
import type { MediaAsset } from '../../types/api';
import { Button, EmptyState, Input, useModal, useToast } from '../../ui';

export interface MediaLibraryTabProps {
  assets: MediaAsset[];
  loading: boolean;
  onRefresh: () => void;
}

const ALLOWED_MIME = ['image/webp', 'image/jpeg', 'image/png', 'image/avif'];

export function MediaLibraryTab({ assets, loading, onRefresh }: MediaLibraryTabProps) {
  const { toast } = useToast();
  const { confirm } = useModal();
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const [uploading, setUploading] = useState(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [editingAltId, setEditingAltId] = useState<string | null>(null);
  const [altDraft, setAltDraft] = useState('');
  const [busyRowId, setBusyRowId] = useState<string | null>(null);

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

    if (file.size > 5 * 1024 * 1024) {
      toast.error('El tamaño máximo permitido es 5 MB.', {
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
        file_name: file.name,
        storage_key: presigned.storage_key,
        public_url: presigned.public_url,
        mime_type: file.type,
        size_bytes: file.size,
        alt_text: file.name.replace(/\.[^.]+$/, ''),
      });

      toast.success('Imagen agregada a la Biblioteca de Medios');
      onRefresh();
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
      onRefresh();
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  const handleDelete = async (asset: MediaAsset) => {
    const ok = await confirm({
      title: 'Eliminar imagen',
      message: `¿Eliminar "${asset.file_name}" de la biblioteca?`,
      variant: 'danger',
      confirmLabel: 'Eliminar',
    });
    if (!ok) return;
    setBusyRowId(asset.id);
    try {
      await bffClient.deleteMediaAsset(asset.id);
      toast.success('Imagen eliminada');
      onRefresh();
    } catch {
      // handled
    } finally {
      setBusyRowId(null);
    }
  };

  return (
    <div className="stack-md">
      <div className="row-between wrap gap-sm">
        <div>
          <h3 className="card-title">Biblioteca de Medios</h3>
          <p className="card-subtitle">
            Sube imágenes optimizadas (WebP, JPEG, PNG, AVIF hasta 5 MB) directamente a R2/S3.
          </p>
        </div>
        <div>
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

      {!loading && assets.length === 0 ? (
        <EmptyState
          icon={<ImagePlus size={28} />}
          title="Tu biblioteca está vacía"
          description="Sube tu primera imagen para usarla como portada o dentro de tus artículos."
        />
      ) : (
        <div className="media-grid">
          {assets.map((asset) => (
            <div key={asset.id} className="media-card">
              <div className="media-card-preview">
                <img src={asset.public_url} alt={asset.alt_text || asset.file_name} loading="lazy" />
              </div>
              <div className="media-card-body stack-xs">
                <div className="text-sm truncate" title={asset.file_name}>
                  <strong>{asset.file_name}</strong>
                </div>
                <span className="text-xs text-muted">
                  {asset.storage_provider} · {(asset.size_bytes / 1024).toFixed(1)} KB
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
                    onClick={() => {
                      setEditingAltId(asset.id);
                      setAltDraft(asset.alt_text ?? '');
                    }}
                  >
                    Alt: {asset.alt_text || 'Agregar descripción...'}
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
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<Trash2 size={13} />}
                    disabled={busyRowId === asset.id}
                    onClick={() => handleDelete(asset)}
                    aria-label={`Eliminar ${asset.file_name}`}
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
