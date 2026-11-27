import { Check, Copy, KeyRound, ShieldCheck, Trash2, UserPlus, Users } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { BffApiError, bffClient } from '../api/bffClient';
import { usePermissions } from '../auth/permissions';
import type { ApiKeySummary, AuthUser, CreatedApiKey, TenantUser } from '../types/api';
import {
  Button,
  DataTable,
  Input,
  Select,
  StatusBadge,
  formatDateTime,
  useDrawer,
  useModal,
  useToast,
  type DataTableColumn,
} from '../ui';

type IdentityTab = 'team' | 'integrations';

const PERMISSION_OPTIONS: Array<{ id: string; label: string; description: string }> = [
  { id: 'crm:write', label: 'Gestión de clientes', description: 'Crear, editar y consultar clientes' },
  {
    id: 'catalog:write',
    label: 'Gestión de catálogo',
    description: 'Administrar categorías, servicios y productos',
  },
  {
    id: 'events:write',
    label: 'Gestión de agenda y reservas',
    description: 'Consultar disponibilidad y crear reservas',
  },
  {
    id: 'telemetry:read',
    label: 'Lectura de actividad',
    description: 'Consultar historial y métricas de uso',
  },
];

function formatRoleLabel(roles: string[]): string {
  if (roles.includes('tenant:admin') || roles.includes('platform:admin')) {
    return 'Administrador';
  }
  return 'Colaborador';
}

function formatPermissionLabel(scope: string): string {
  const found = PERMISSION_OPTIONS.find((p) => p.id === scope);
  if (found) return found.label;
  if (scope.startsWith('crm')) return 'Clientes';
  if (scope.startsWith('catalog')) return 'Catálogo';
  if (scope.startsWith('events') || scope.startsWith('bookings')) return 'Agenda y reservas';
  if (scope.startsWith('telemetry')) return 'Actividad';
  return scope;
}

interface UserDrawerFormProps {
  onDirtyChange: (dirty: boolean) => void;
  onSaved: () => void;
  onCancel: () => void;
}

function UserDrawerForm({ onDirtyChange, onSaved, onCancel }: UserDrawerFormProps) {
  const { toast } = useToast();
  const [fullName, setFullName] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [role, setRole] = useState<'tenant:member' | 'tenant:admin'>('tenant:member');
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setFormError(null);
    if (!fullName.trim() || !email.trim()) {
      setFormError('Nombre y correo electrónico son obligatorios.');
      return;
    }

    setSaving(true);
    try {
      await bffClient.createUser({
        full_name: fullName.trim(),
        email: email.trim(),
        temporary_password: password.trim() || undefined,
        roles: [role],
      });
      toast.success(`Miembro "${fullName.trim()}" agregado al equipo.`);
      onDirtyChange(false);
      onSaved();
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Error agregando miembro';
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
        label="Nombre completo"
        required
        value={fullName}
        onChange={(e) => {
          setFullName(e.target.value);
          onDirtyChange(true);
        }}
        placeholder="Ej: Ana María López"
      />

      <Input
        label="Correo electrónico"
        required
        type="email"
        value={email}
        onChange={(e) => {
          setEmail(e.target.value);
          onDirtyChange(true);
        }}
        placeholder="ana@tunegocio.cl"
      />

      <Input
        label="Contraseña temporal"
        type="password"
        value={password}
        onChange={(e) => {
          setPassword(e.target.value);
          onDirtyChange(true);
        }}
        placeholder="Mínimo 6 caracteres"
        hint="El usuario podrá cambiarla en su primer ingreso."
      />

      <Select
        label="Rol en el equipo"
        value={role}
        onChange={(e) => {
          setRole(e.target.value as 'tenant:member' | 'tenant:admin');
          onDirtyChange(true);
        }}
      >
        <option value="tenant:member">Colaborador (gestión operativa diaria)</option>
        <option value="tenant:admin">Administrador (acceso total y ajustes)</option>
      </Select>

      <div className="row-end">
        <Button variant="secondary" type="button" onClick={onCancel} disabled={saving}>
          Cancelar
        </Button>
        <Button variant="primary" type="submit" isLoading={saving}>
          Agregar miembro
        </Button>
      </div>
    </form>
  );
}

interface ApiKeyDrawerFormProps {
  onDirtyChange: (dirty: boolean) => void;
  onCreated: (created: CreatedApiKey) => void;
  onCancel: () => void;
}

function ApiKeyDrawerForm({ onDirtyChange, onCreated, onCancel }: ApiKeyDrawerFormProps) {
  const { toast } = useToast();
  const [name, setName] = useState('');
  const [selectedPermissions, setSelectedPermissions] = useState<string[]>([
    'crm:write',
    'catalog:write',
    'events:write',
  ]);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const togglePermission = (id: string) => {
    setSelectedPermissions((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
    onDirtyChange(true);
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setFormError(null);
    if (!name.trim()) {
      setFormError('El nombre de la clave es obligatorio.');
      return;
    }
    if (selectedPermissions.length === 0) {
      setFormError('Selecciona al menos un permiso para esta integración.');
      return;
    }

    setSaving(true);
    try {
      const created = await bffClient.createApiKey({
        name: name.trim(),
        scopes: selectedPermissions,
      });
      toast.success(`Clave "${created.key.name}" generada correctamente.`);
      onDirtyChange(false);
      onCreated(created);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Error generando clave de integración';
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
        label="Nombre de la integración"
        required
        value={name}
        onChange={(e) => {
          setName(e.target.value);
          onDirtyChange(true);
        }}
        placeholder="Ej: Sitio web de reservas"
      />

      <div className="field">
        <span className="field-label">Permisos asignados</span>
        <div className="card card-compact stack-sm">
          {PERMISSION_OPTIONS.map((perm) => (
            <label key={perm.id} className="checkbox-row">
              <input
                type="checkbox"
                checked={selectedPermissions.includes(perm.id)}
                onChange={() => togglePermission(perm.id)}
              />
              <div className="stack-xs">
                <span className="text-sm font-semibold">{perm.label}</span>
                <span className="text-xs text-muted">{perm.description}</span>
              </div>
            </label>
          ))}
        </div>
      </div>

      <div className="row-end">
        <Button variant="secondary" type="button" onClick={onCancel} disabled={saving}>
          Cancelar
        </Button>
        <Button variant="primary" type="submit" isLoading={saving}>
          Generar clave
        </Button>
      </div>
    </form>
  );
}

function ApiKeyRevealContent({ createdKey }: { createdKey: CreatedApiKey }) {
  const { toast } = useToast();
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(createdKey.raw_key);
      setCopied(true);
      toast.info('Clave copiada al portapapeles.');
      setTimeout(() => setCopied(false), 2500);
    } catch {
      toast.error('No se pudo copiar al portapapeles.');
    }
  };

  return (
    <div className="stack-md">
      <div className="alert-banner alert-banner-warning">
        <div className="alert-banner-body">
          Esta clave se muestra <strong>una sola vez</strong> por seguridad. Guárdala en un lugar
          seguro antes de cerrar esta ventana.
        </div>
      </div>

      <div className="text-sm">
        <strong>{createdKey.key.name}</strong>{' '}
        <span className="text-muted">({createdKey.key.key_prefix}...)</span>
      </div>

      <div className="code-box row-between">
        <span>{createdKey.raw_key}</span>
        <Button
          variant="secondary"
          size="sm"
          icon={copied ? <Check size={14} /> : <Copy size={14} />}
          onClick={handleCopy}
        >
          {copied ? 'Copiada' : 'Copiar'}
        </Button>
      </div>
    </div>
  );
}

export interface IdentityViewProps {
  user: AuthUser;
}

export function IdentityView({ user }: IdentityViewProps) {
  const { can } = usePermissions(user);
  const canWrite = can('identity:write');
  const { openDrawer, closeDrawer } = useDrawer();
  const { openModal, closeModal, confirm } = useModal();
  const { toast } = useToast();

  const [activeTab, setActiveTab] = useState<IdentityTab>('team');
  const [users, setUsers] = useState<TenantUser[]>([]);
  const [apiKeys, setApiKeys] = useState<ApiKeySummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyRowId, setBusyRowId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const dirtyRef = useRef(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [u, k] = await Promise.all([
        bffClient.listUsers(),
        canWrite ? bffClient.listApiKeys() : Promise.resolve<ApiKeySummary[]>([]),
      ]);
      setUsers(u);
      setApiKeys(k);
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : 'No tienes permisos para administrar el equipo y los accesos.',
      );
    } finally {
      setLoading(false);
    }
  }, [canWrite]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const showRevealModal = useCallback(
    (created: CreatedApiKey) => {
      const modalId = openModal({
        title: 'Clave generada — Guárdala ahora',
        subtitle: 'Copia y almacena esta clave en un lugar seguro.',
        size: 'md',
        closeOnBackdrop: false,
        content: <ApiKeyRevealContent createdKey={created} />,
        footer: (
          <Button variant="primary" onClick={() => closeModal(modalId)}>
            Entendido, ya la guardé
          </Button>
        ),
      });
    },
    [openModal, closeModal],
  );

  const openCreateUserDrawer = useCallback(() => {
    dirtyRef.current = false;
    openDrawer({
      title: 'Agregar miembro al equipo',
      subtitle: 'Invita a una persona y define su nivel de acceso.',
      size: 'sm',
      hasUnsavedChanges: () => dirtyRef.current,
      content: (
        <UserDrawerForm
          onDirtyChange={(dirty) => {
            dirtyRef.current = dirty;
          }}
          onCancel={() => closeDrawer(false)}
          onSaved={() => {
            closeDrawer(true);
            void loadData();
          }}
        />
      ),
    });
  }, [openDrawer, closeDrawer, loadData]);

  const openCreateApiKeyDrawer = useCallback(() => {
    dirtyRef.current = false;
    openDrawer({
      title: 'Nueva clave de integración',
      subtitle: 'Permite conectar aplicaciones externas de forma segura.',
      size: 'sm',
      hasUnsavedChanges: () => dirtyRef.current,
      content: (
        <ApiKeyDrawerForm
          onDirtyChange={(dirty) => {
            dirtyRef.current = dirty;
          }}
          onCancel={() => closeDrawer(false)}
          onCreated={(created) => {
            closeDrawer(true);
            void loadData();
            showRevealModal(created);
          }}
        />
      ),
    });
  }, [openDrawer, closeDrawer, loadData, showRevealModal]);

  const handleDeleteUser = useCallback(
    async (user: TenantUser) => {
      if (busyRowId === user.id) return;
      const ok = await confirm({
        title: 'Quitar miembro del equipo',
        message: `¿Deshabilitar y eliminar el acceso de "${user.full_name}" (${user.email})?`,
        variant: 'danger',
        confirmLabel: 'Eliminar acceso',
      });
      if (!ok) return;

      setBusyRowId(user.id);
      try {
        await bffClient.deleteUser(user.id);
        toast.success(`Acceso de "${user.full_name}" eliminado.`);
        await loadData();
      } catch (err) {
        if (!(err instanceof BffApiError && err.notified)) {
          toast.error(err instanceof Error ? err.message : 'Error eliminando miembro');
        }
      } finally {
        setBusyRowId((prev) => (prev === user.id ? null : prev));
      }
    },
    [busyRowId, confirm, toast, loadData],
  );

  const handleRevokeKey = useCallback(
    async (key: ApiKeySummary) => {
      if (busyRowId === key.id) return;
      const ok = await confirm({
        title: 'Revocar clave de integración',
        message: `¿Revocar la clave "${key.name}"? Las aplicaciones conectadas con esta clave dejarán de tener acceso.`,
        variant: 'danger',
        confirmLabel: 'Revocar',
      });
      if (!ok) return;

      setBusyRowId(key.id);
      try {
        await bffClient.revokeApiKey(key.id);
        toast.success(`Clave "${key.name}" revocada.`);
        await loadData();
      } catch (err) {
        if (!(err instanceof BffApiError && err.notified)) {
          toast.error(err instanceof Error ? err.message : 'Error revocando clave');
        }
      } finally {
        setBusyRowId((prev) => (prev === key.id ? null : prev));
      }
    },
    [busyRowId, confirm, toast, loadData],
  );

  const userColumns = useMemo<DataTableColumn<TenantUser>[]>(
    () => [
      {
        key: 'full_name',
        header: 'Nombre',
        sortable: true,
        sortValue: (u) => u.full_name,
        render: (u) => <span className="font-semibold">{u.full_name}</span>,
      },
      {
        key: 'email',
        header: 'Correo electrónico',
        sortable: true,
        sortValue: (u) => u.email,
        render: (u) => u.email,
      },
      {
        key: 'roles',
        header: 'Rol',
        render: (u) => {
          const roleLabel = formatRoleLabel(u.roles);
          return (
            <StatusBadge
              label={roleLabel}
              tone={roleLabel === 'Administrador' ? 'blue' : 'gray'}
            />
          );
        },
      },
      {
        key: 'status',
        header: 'Estado',
        render: (u) => (
          <StatusBadge
            label={u.is_active ? 'Activo' : 'Inactivo'}
            tone={u.is_active ? 'green' : 'gray'}
          />
        ),
      },
      {
        key: 'actions',
        header: 'Acción',
        align: 'right',
        render: (u) => (
          <div className="row-end">
            <Button
              variant="danger"
              size="sm"
              icon={<Trash2 size={13} />}
              onClick={() => handleDeleteUser(u)}
              disabled={busyRowId === u.id}
              isLoading={busyRowId === u.id}
              aria-label={`Eliminar miembro ${u.full_name}`}
              title="Eliminar acceso"
            />
          </div>
        ),
      },
    ],
    [busyRowId, handleDeleteUser],
  );

  const apiKeyColumns = useMemo<DataTableColumn<ApiKeySummary>[]>(
    () => [
      {
        key: 'name',
        header: 'Nombre',
        sortable: true,
        sortValue: (k) => k.name,
        render: (k) => <span className="font-semibold">{k.name}</span>,
      },
      {
        key: 'prefix',
        header: 'Identificador',
        render: (k) => <code className="text-xs">{k.key_prefix}••••••••</code>,
      },
      {
        key: 'scopes',
        header: 'Permisos',
        hideOnMobile: true,
        render: (k) =>
          k.scopes.length > 0 ? (
            <div className="chip-list">
              {k.scopes.map((s) => (
                <StatusBadge key={s} label={formatPermissionLabel(s)} tone="blue" />
              ))}
            </div>
          ) : (
            <span className="text-muted">—</span>
          ),
      },
      {
        key: 'created_at',
        header: 'Creada',
        hideOnMobile: true,
        sortable: true,
        sortValue: (k) => k.created_at,
        render: (k) => <span className="text-xs">{formatDateTime(k.created_at)}</span>,
      },
      {
        key: 'status',
        header: 'Estado',
        render: (k) => {
          const isRevoked = Boolean(k.revoked_at) || !k.is_active;
          return (
            <StatusBadge
              label={isRevoked ? 'Revocada' : 'Activa'}
              tone={isRevoked ? 'red' : 'green'}
            />
          );
        },
      },
      {
        key: 'actions',
        header: 'Acción',
        align: 'right',
        render: (k) => {
          const isRevoked = Boolean(k.revoked_at) || !k.is_active;
          if (isRevoked) return <span className="text-xs text-muted">—</span>;
          return (
            <div className="row-end">
              <Button
                variant="danger"
                size="sm"
                onClick={() => handleRevokeKey(k)}
                disabled={busyRowId === k.id}
                isLoading={busyRowId === k.id}
              >
                Revocar
              </Button>
            </div>
          );
        },
      },
    ],
    [busyRowId, handleRevokeKey],
  );

  return (
    <div className="stack-lg">
      <div className="page-header">
        <div>
          <h1 className="page-title">Equipo y Accesos</h1>
          <p className="page-subtitle">
            Administra a los miembros de tu equipo y las integraciones conectadas a tu cuenta.
          </p>
        </div>
        <div className="page-actions">
          {activeTab === 'team' ? (
            <Button
              variant="primary"
              icon={<UserPlus size={15} />}
              onClick={openCreateUserDrawer}
            >
              Agregar miembro
            </Button>
          ) : (
            <Button
              variant="primary"
              icon={<KeyRound size={15} />}
              onClick={openCreateApiKeyDrawer}
            >
              Nueva clave
            </Button>
          )}
        </div>
      </div>

      {error && (
        <div className="alert-banner alert-banner-danger" role="alert">
          <div className="alert-banner-body">{error}</div>
        </div>
      )}

      <div className="row-between row-wrap">
        <div className="tabs">
          <button
            type="button"
            className={`tab-btn ${activeTab === 'team' ? 'active' : ''}`}
            onClick={() => setActiveTab('team')}
          >
            Equipo ({users.length})
          </button>
          <button
            type="button"
            className={`tab-btn ${activeTab === 'integrations' ? 'active' : ''}`}
            onClick={() => setActiveTab('integrations')}
          >
            Integraciones (Avanzado) ({apiKeys.length})
          </button>
        </div>
      </div>

      {activeTab === 'team' ? (
        <DataTable<TenantUser>
          columns={userColumns}
          data={users}
          rowKey={(u) => u.id}
          isLoading={loading}
          emptyTitle="Sin miembros registrados"
          emptyDescription="Agrega a los colaboradores de tu negocio para que puedan ingresar al portal."
          emptyIcon={<Users size={22} />}
          emptyAction={
            <Button
              variant="primary"
              size="sm"
              icon={<UserPlus size={14} />}
              onClick={openCreateUserDrawer}
            >
              Agregar miembro
            </Button>
          }
        />
      ) : (
        <div className="stack-sm">
          <div className="alert-banner alert-banner-info">
            <div className="alert-banner-body">
              Las claves de integración permiten conectar tu sitio web o sistemas externos con tu
              cuenta. Solo se muestran una vez al crearlas.
            </div>
          </div>

          <DataTable<ApiKeySummary>
            columns={apiKeyColumns}
            data={apiKeys}
            rowKey={(k) => k.id}
            isLoading={loading}
            emptyTitle="Sin integraciones configuradas"
            emptyDescription="Crea una clave cuando necesites conectar una aplicación externa."
            emptyIcon={<ShieldCheck size={22} />}
            emptyAction={
              <Button
                variant="primary"
                size="sm"
                icon={<KeyRound size={14} />}
                onClick={openCreateApiKeyDrawer}
              >
                Nueva clave
              </Button>
            }
          />
        </div>
      )}
    </div>
  );
}
