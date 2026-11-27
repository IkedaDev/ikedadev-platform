import { Image as ImageIcon, Link2, Upload } from 'lucide-react';
import { useCallback, useEffect, useRef, useState, type ChangeEvent } from 'react';
import { BffApiError, bffClient } from '../../api/bffClient';
import type { MediaAsset, MediaContext } from '../../types/api';
import { Button, EmptyState, Input, Skeleton, useModal, useToast } from '../../ui';

const ALLOWED_MIME = ['image/webp', 'image/jpeg', 'image/png', 'image/avif'];

export function isSafeMediaUrl(raw: string): boolean {
  try {
    const parsed = new URL(raw.trim());
    if (parsed.protocol === 'https:') return true;
    if (parsed.protocol === 'http:' && import.meta.env.DEV) return true;
    return false;
  } catch {
    return false;
  }
}

interface MediaPickerBodyProps {
  context: MediaContext;
  allowUpload: boolean;
  onSelect: (url: string, asset?: MediaAsset) => void;
  onClose: () => void;
}

function MediaPickerBody({ context, allowUpload, onSelect, onClose }: MediaPickerBodyProps) {
  const { toast } = useToast();
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const [scopeFilter, setScopeFilter] = useState<'context' | 'all'>('context');
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [loading, setLoading] = useState(true);
  const [uploading, setUploading] = useState(false);
  const [externalUrl, setExternalUrl] = useState('');

  const loadAssets = useCallback(async () => {
    setLoading(true);
    try {
      const res = await bffClient.listMediaAssets({
        context: scopeFilter === 'context' ? context : '',
        page: 1,
        page_size: 36,
      });
      setAssets(res.data);
    } catch {
      // handled globally
    } finally {
      setLoading(false);
    }
  }, [context, scopeFilter]);

  useEffect(() => {
    void loadAssets();
  }, [loadAssets]);

  const handleUpload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;

    if (!ALLOWED_MIME.includes(file.type)) {
      toast.error('Solo se permiten imágenes WebP, JPEG, PNG o AVIF.');
      return;
    }
    if (file.size > 5 * 1024 * 1024) {
      toast.error('El tamaño máximo por imagen es 5 MB.');
      return;
    }

    setUploading(true);
    try {
      const presigned = await bffClient.presignMediaUpload({
        file_name: file.name,
        mime_type: file.type,
        size_bytes: file.size,
        context,
      });

      const uploadRes = await fetch(presigned.upload_url, {
        method: 'PUT',
        headers: { 'Content-Type': file.type },
        body: file,
      });
      if (!uploadRes.ok) {
        throw new Error(`Error al subir archivo (${uploadRes.status})`);
      }

      const asset = await bffClient.confirmMediaUpload({
        asset_id: presigned.asset_id,
        alt_text: file.name.replace(/\.[^.]+$/, ''),
      });

      toast.success('Imagen subida y seleccionada');
      onSelect(asset.public_url, asset);
      onClose();
    } catch (err) {
      if (!(err instanceof BffApiError && err.notified)) {
        toast.error(err instanceof Error ? err.message : 'No se pudo subir la imagen');
      }
    } finally {
      setUploading(false);
    }
  };

  const handleUseExternalUrl = () => {
    const trimmed = externalUrl.trim();
    if (!isSafeMediaUrl(trimmed)) {
      toast.error('Ingresa una URL https:// válida');
      return;
    }
    onSelect(trimmed);
    onClose();
  };

  return (
    <div className="stack-md">
      {/* External URL */}
      <div className="row-sm">
        <Input
          placeholder="Pegar URL externa (https://...)"
          value={externalUrl}
          onChange={(e) => setExternalUrl(e.target.value)}
        />
        <Button
          type="button"
          size="md"
          variant="secondary"
          icon={<Link2 size={14} />}
          disabled={!isSafeMediaUrl(externalUrl)}
          onClick={handleUseExternalUrl}
        >
          Usar URL
        </Button>
      </div>

      {/* Filter + Upload */}
      <div className="row-between row-wrap gap-sm">
        <div className="filter-pills">
          <button
            type="button"
            className={`pill-btn ${scopeFilter === 'context' ? 'active' : ''}`}
            onClick={() => setScopeFilter('context')}
          >
            Contexto: {context}
          </button>
          <button
            type="button"
            className={`pill-btn ${scopeFilter === 'all' ? 'active' : ''}`}
            onClick={() => setScopeFilter('all')}
          >
            Toda la galería
          </button>
        </div>

        {allowUpload && (
          <div>
            <input
              ref={fileInputRef}
              type="file"
              accept="image/webp,image/jpeg,image/png,image/avif"
              className="visually-hidden"
              onChange={handleUpload}
            />
            <Button
              type="button"
              size="sm"
              variant="primary"
              icon={<Upload size={14} />}
              isLoading={uploading}
              onClick={() => fileInputRef.current?.click()}
            >
              Subir nueva
            </Button>
          </div>
        )}
      </div>

      {/* Assets Grid */}
      {loading ? (
        <div className="media-grid">
          {Array.from({ length: 6 }).map((_, idx) => (
            <div key={idx} className="media-card">
              <div className="media-card-preview">
                <Skeleton variant="card" />
              </div>
              <div className="media-card-body stack-xs">
                <Skeleton variant="line" />
                <Skeleton variant="line-sm" />
              </div>
            </div>
          ))}
        </div>
      ) : assets.length === 0 ? (
        <EmptyState
          icon={<ImageIcon size={24} />}
          title="No hay imágenes en este contexto"
          description={
            allowUpload
              ? 'Sube una nueva imagen o pega una URL externa arriba.'
              : 'No hay imágenes disponibles en este contexto.'
          }
        />
      ) : (
        <div className="media-grid">
          {assets.map((asset) => (
            <button
              key={asset.id}
              type="button"
              className="media-card media-card-selectable"
              onClick={() => {
                onSelect(asset.public_url, asset);
                onClose();
              }}
            >
              <div className="media-card-preview">
                <img
                  src={asset.public_url}
                  alt={asset.alt_text || asset.file_name}
                  loading="lazy"
                  referrerPolicy="no-referrer"
                />
              </div>
              <div className="media-card-body">
                <div className="text-xs truncate">
                  <strong>{asset.file_name}</strong>
                </div>
                <span className="text-xs text-muted">
                  {asset.context} · {(asset.size_bytes / 1024).toFixed(0)} KB
                </span>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Hook that opens the transversal MediaPicker inside `ModalProvider`,
 * guaranteeing automatic cleanup on logout / HTTP 401 via `clearModals()`.
 */
export function useMediaPicker() {
  const { openModal, closeModal } = useModal();

  const openMediaPicker = useCallback(
    (options: {
      context: MediaContext;
      title?: string;
      allowUpload?: boolean;
      onSelect: (url: string, asset?: MediaAsset) => void;
    }) => {
      const modalId = openModal({
        title: options.title ?? 'Seleccionar o subir imagen',
        subtitle: `Biblioteca de Medios (contexto: ${options.context})`,
        size: 'lg',
        content: (
          <MediaPickerBody
            context={options.context}
            allowUpload={options.allowUpload ?? true}
            onSelect={options.onSelect}
            onClose={() => closeModal(modalId)}
          />
        ),
      });
    },
    [openModal, closeModal],
  );

  return { openMediaPicker };
}
