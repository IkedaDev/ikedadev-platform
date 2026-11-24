import { AlertTriangle, KeyRound, LogIn, ShieldCheck } from 'lucide-react';
import { bffClient } from '../api/bffClient';
import type { BffHealth } from '../types/api';

interface LoginViewProps {
  health: BffHealth | null;
  authError: string | null;
}

export function LoginView({ health, authError }: LoginViewProps) {
  const handleLogin = () => {
    window.location.href = bffClient.loginUrl();
  };

  return (
    <div
      style={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        padding: 20,
        background: 'linear-gradient(135deg, #0f172a 0%, #1e293b 100%)',
      }}
    >
      <div className="card" style={{ maxWidth: 460, width: '100%', padding: 32 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 20 }}>
          <div className="brand-mark" style={{ width: 42, height: 42, fontSize: 18 }}>
            IK
          </div>
          <div>
            <h1 style={{ margin: 0, fontSize: 20, fontWeight: 700 }}>IkedaDev Tenant Portal</h1>
            <div style={{ fontSize: 13, color: 'var(--text-muted)' }}>
              Autenticación OIDC vía Keycloak + BFF Session
            </div>
          </div>
        </div>

        {authError && (
          <div className="alert alert-error">
            <AlertTriangle size={17} style={{ flexShrink: 0, marginTop: 1 }} />
            <div>{authError}</div>
          </div>
        )}

        <p style={{ fontSize: 14, color: 'var(--text-muted)', lineHeight: 1.55, marginTop: 0 }}>
          Inicia sesión con tu cuenta de <strong>Keycloak</strong>. El BFF gestionará el flujo{' '}
          <code>Authorization Code + PKCE</code> en una cookie <code>HttpOnly</code> y enviará tu
          JWT firmado (RS256) a los 4 microservicios Rust para validación JWKS y aislamiento
          multi-tenant (RLS).
        </p>

        <button
          type="button"
          className="btn btn-primary"
          onClick={handleLogin}
          style={{ width: '100%', padding: '11px 16px', fontSize: 14, marginBottom: 20 }}
        >
          <LogIn size={16} />
          Iniciar sesión con Keycloak (SSO)
        </button>

        <div
          style={{
            background: '#f8fafc',
            border: '1px solid var(--border)',
            borderRadius: 8,
            padding: 14,
            fontSize: 12,
            marginBottom: 16,
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              fontWeight: 600,
              marginBottom: 8,
              color: 'var(--text)',
            }}
          >
            <KeyRound size={14} />
            Usuarios demo pre-cargados (Realm: ikeda)
          </div>
          <div style={{ display: 'grid', gap: 6, color: 'var(--text-muted)' }}>
            <div>
              <strong>Admin (tenant:admin):</strong> <code>admin@demo.ikedadev.com</code> /{' '}
              <code>Admin123!</code>
            </div>
            <div>
              <strong>Staff (tenant:member):</strong> <code>staff@demo.ikedadev.com</code> /{' '}
              <code>Staff123!</code>
            </div>
          </div>
        </div>

        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            fontSize: 12,
            color: 'var(--text-muted)',
            borderTop: '1px solid var(--border)',
            paddingTop: 12,
          }}
        >
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
            <ShieldCheck size={14} />
            Estado BFF / Keycloak
          </span>
          <span>
            <span
              className={`health-dot ${
                !health ? 'offline' : health.upstreams.keycloak?.ok ? 'ok' : 'degraded'
              }`}
            />
            {!health
              ? 'BFF offline'
              : health.upstreams.keycloak?.ok
                ? 'Keycloak listo'
                : 'Esperando Keycloak'}
          </span>
        </div>
      </div>
    </div>
  );
}
