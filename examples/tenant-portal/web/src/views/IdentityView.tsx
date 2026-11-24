import { Check, Copy, KeyRound, Trash2, UserPlus } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { bffClient } from '../api/bffClient';
import { Modal } from '../components/Modal';
import { StatusBadge, formatDateTime } from '../components/StatusBadge';
import type { ApiKeySummary, CreatedApiKey, TenantUser } from '../types/api';

export function IdentityView() {
  const [users, setUsers] = useState<TenantUser[]>([]);
  const [apiKeys, setApiKeys] = useState<ApiKeySummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Create user modal
  const [userModalOpen, setUserModalOpen] = useState(false);
  const [userName, setUserName] = useState('');
  const [userEmail, setUserEmail] = useState('');
  const [userPassword, setUserPassword] = useState('');
  const [userRoles, setUserRoles] = useState('tenant:member');
  const [savingUser, setSavingUser] = useState(false);
  const [userFormError, setUserFormError] = useState<string | null>(null);

  // Create API key modal
  const [keyModalOpen, setKeyModalOpen] = useState(false);
  const [keyName, setKeyName] = useState('');
  const [keyScopes, setKeyScopes] = useState('events:write, crm:write, catalog:write');
  const [savingKey, setSavingKey] = useState(false);
  const [keyFormError, setKeyFormError] = useState<string | null>(null);

  // Newly created API key (shown once)
  const [createdKey, setCreatedKey] = useState<CreatedApiKey | null>(null);
  const [copied, setCopied] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [u, k] = await Promise.all([bffClient.listUsers(), bffClient.listApiKeys()]);
      setUsers(u);
      setApiKeys(k);
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : 'Error cargando identidad (verifica scopes tenant:admin / platform:admin)',
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const handleCreateUser = async () => {
    setUserFormError(null);
    if (!userName.trim() || !userEmail.trim()) {
      setUserFormError('Nombre y email son obligatorios.');
      return;
    }
    setSavingUser(true);
    try {
      await bffClient.createUser({
        full_name: userName.trim(),
        email: userEmail.trim(),
        temporary_password: userPassword.trim() || undefined,
        roles: userRoles
          .split(',')
          .map((r) => r.trim())
          .filter(Boolean),
      });
      setUserName('');
      setUserEmail('');
      setUserPassword('');
      setUserModalOpen(false);
      await loadData();
    } catch (err) {
      setUserFormError(err instanceof Error ? err.message : 'Error creando usuario');
    } finally {
      setSavingUser(false);
    }
  };

  const handleDeleteUser = async (user: TenantUser) => {
    if (!confirm(`¿Deshabilitar y eliminar a "${user.full_name}"?`)) return;
    try {
      await bffClient.deleteUser(user.id);
      await loadData();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error eliminando usuario');
    }
  };

  const handleCreateApiKey = async () => {
    setKeyFormError(null);
    if (!keyName.trim()) {
      setKeyFormError('El nombre de la API Key es obligatorio.');
      return;
    }
    setSavingKey(true);
    try {
      const created = await bffClient.createApiKey({
        name: keyName.trim(),
        scopes: keyScopes
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
      });
      setKeyName('');
      setKeyModalOpen(false);
      setCopied(false);
      setCreatedKey(created);
      await loadData();
    } catch (err) {
      setKeyFormError(err instanceof Error ? err.message : 'Error generando API Key');
    } finally {
      setSavingKey(false);
    }
  };

  const handleRevokeKey = async (key: ApiKeySummary) => {
    if (!confirm(`¿Revocar la API Key "${key.name}" (${key.key_prefix}...)?`)) return;
    try {
      await bffClient.revokeApiKey(key.id);
      await loadData();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error revocando API Key');
    }
  };

  const handleCopyRawKey = async () => {
    if (!createdKey) return;
    await navigator.clipboard.writeText(createdKey.raw_key);
    setCopied(true);
    setTimeout(() => setCopied(false), 2500);
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <h1 className="page-title">Identidad y Acceso</h1>
          <p className="page-subtitle">
            Gestión de usuarios del tenant (Keycloak) y llaves M2M (API Keys) en identity-api.
          </p>
        </div>
        <div className="btn-row">
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              setUserFormError(null);
              setUserModalOpen(true);
            }}
          >
            <UserPlus size={15} />
            Nuevo Usuario
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              setKeyFormError(null);
              setKeyModalOpen(true);
            }}
          >
            <KeyRound size={15} />
            Nueva API Key
          </button>
        </div>
      </div>

      {error && <div className="alert alert-error">{error}</div>}

      {/* Users Table */}
      <div className="card" style={{ marginBottom: 24 }}>
        <div className="section-header">
          <h2 className="section-title">Usuarios del Tenant ({users.length})</h2>
          <StatusBadge label="Sincronizado con Keycloak" tone="blue" />
        </div>

        {loading ? (
          <div className="empty-state">Cargando usuarios...</div>
        ) : users.length === 0 ? (
          <div className="empty-state">No hay usuarios registrados para este tenant.</div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nombre</th>
                  <th>Email</th>
                  <th>Roles</th>
                  <th>Estado</th>
                  <th style={{ width: 80 }}>Acción</th>
                </tr>
              </thead>
              <tbody>
                {users.map((user) => (
                  <tr key={user.id}>
                    <td style={{ fontWeight: 600 }}>{user.full_name}</td>
                    <td>{user.email}</td>
                    <td>
                      <div className="btn-row">
                        {user.roles.map((r) => (
                          <StatusBadge key={r} label={r} tone="gray" />
                        ))}
                      </div>
                    </td>
                    <td>
                      <StatusBadge
                        label={user.is_active ? 'Activo' : 'Inactivo'}
                        tone={user.is_active ? 'green' : 'gray'}
                      />
                    </td>
                    <td>
                      <button
                        type="button"
                        className="btn btn-danger btn-sm"
                        onClick={() => handleDeleteUser(user)}
                        title="Eliminar usuario"
                      >
                        <Trash2 size={13} />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* API Keys Table */}
      <div className="card">
        <div className="section-header">
          <h2 className="section-title">API Keys M2M ({apiKeys.length})</h2>
          <StatusBadge label="SHA-256 en backend · Secreto visible una sola vez" tone="amber" />
        </div>

        {loading ? (
          <div className="empty-state">Cargando API Keys...</div>
        ) : apiKeys.length === 0 ? (
          <div className="empty-state">No hay API Keys emitidas.</div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nombre</th>
                  <th>Prefijo</th>
                  <th>Scopes</th>
                  <th>Creada</th>
                  <th>Estado</th>
                  <th style={{ width: 90 }}>Acción</th>
                </tr>
              </thead>
              <tbody>
                {apiKeys.map((key) => {
                  const isRevoked = Boolean(key.revoked_at) || !key.is_active;
                  return (
                    <tr key={key.id}>
                      <td style={{ fontWeight: 600 }}>{key.name}</td>
                      <td>
                        <code>{key.key_prefix}••••••••</code>
                      </td>
                      <td>
                        <div className="btn-row">
                          {key.scopes.length > 0 ? (
                            key.scopes.map((s) => <StatusBadge key={s} label={s} tone="blue" />)
                          ) : (
                            <span style={{ color: 'var(--text-muted)' }}>—</span>
                          )}
                        </div>
                      </td>
                      <td>{formatDateTime(key.created_at)}</td>
                      <td>
                        <StatusBadge
                          label={isRevoked ? 'Revocada' : 'Activa'}
                          tone={isRevoked ? 'red' : 'green'}
                        />
                      </td>
                      <td>
                        {!isRevoked && (
                          <button
                            type="button"
                            className="btn btn-danger btn-sm"
                            onClick={() => handleRevokeKey(key)}
                          >
                            Revocar
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Create User Modal */}
      <Modal
        open={userModalOpen}
        title="Provisionar Usuario del Tenant"
        onClose={() => setUserModalOpen(false)}
        footer={
          <>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => setUserModalOpen(false)}
            >
              Cancelar
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleCreateUser}
              disabled={savingUser}
            >
              {savingUser ? 'Creando...' : 'Crear Usuario'}
            </button>
          </>
        }
      >
        {userFormError && <div className="alert alert-error">{userFormError}</div>}
        <div className="form-grid">
          <div className="form-field full">
            <label className="form-label">Nombre Completo *</label>
            <input
              className="input"
              value={userName}
              onChange={(e) => setUserName(e.target.value)}
              placeholder="Ej: Ana María López"
            />
          </div>
          <div className="form-field full">
            <label className="form-label">Email *</label>
            <input
              type="email"
              className="input"
              value={userEmail}
              onChange={(e) => setUserEmail(e.target.value)}
              placeholder="ana@tenant.cl"
            />
          </div>
          <div className="form-field">
            <label className="form-label">Password Temporal</label>
            <input
              type="password"
              className="input"
              value={userPassword}
              onChange={(e) => setUserPassword(e.target.value)}
              placeholder="Mínimo 6 caracteres"
            />
          </div>
          <div className="form-field">
            <label className="form-label">Roles (separados por coma)</label>
            <input
              className="input"
              value={userRoles}
              onChange={(e) => setUserRoles(e.target.value)}
            />
          </div>
        </div>
      </Modal>

      {/* Create API Key Modal */}
      <Modal
        open={keyModalOpen}
        title="Generar Nueva API Key"
        onClose={() => setKeyModalOpen(false)}
        footer={
          <>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => setKeyModalOpen(false)}
            >
              Cancelar
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleCreateApiKey}
              disabled={savingKey}
            >
              {savingKey ? 'Generando...' : 'Generar API Key'}
            </button>
          </>
        }
      >
        {keyFormError && <div className="alert alert-error">{keyFormError}</div>}
        <div className="form-grid">
          <div className="form-field full">
            <label className="form-label">Nombre descriptivo *</label>
            <input
              className="input"
              value={keyName}
              onChange={(e) => setKeyName(e.target.value)}
              placeholder="Ej: Anami BFF Staging"
            />
          </div>
          <div className="form-field full">
            <label className="form-label">Scopes (separados por coma)</label>
            <input
              className="input"
              value={keyScopes}
              onChange={(e) => setKeyScopes(e.target.value)}
              placeholder="events:write, crm:write, catalog:write"
            />
          </div>
        </div>
      </Modal>

      {/* One-time API Key reveal modal */}
      <Modal
        open={Boolean(createdKey)}
        title="API Key Generada — Guárdala Ahora"
        onClose={() => setCreatedKey(null)}
        footer={
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => setCreatedKey(null)}
          >
            Entendido, ya la guardé
          </button>
        }
      >
        {createdKey && (
          <div>
            <div className="alert alert-warning">
              Esta llave se muestra <strong>una única vez</strong>. El servidor solo almacena su
              hash SHA-256 y no podrá recuperarla después de cerrar esta ventana.
            </div>
            <div style={{ marginBottom: 8, fontSize: 13 }}>
              <strong>{createdKey.key.name}</strong> ({createdKey.key.key_prefix}...)
            </div>
            <div className="code-box">
              <span>{createdKey.raw_key}</span>
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                onClick={handleCopyRawKey}
              >
                {copied ? <Check size={14} /> : <Copy size={14} />}
                {copied ? 'Copiada' : 'Copiar'}
              </button>
            </div>
          </div>
        )}
      </Modal>
    </div>
  );
}
