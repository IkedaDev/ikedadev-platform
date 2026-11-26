import { AlertTriangle, KeyRound, LogIn, ShieldCheck } from 'lucide-react';
import { bffClient } from '../api/bffClient';
import { useTheme } from '../context/ThemeProvider';
import type { BffHealth } from '../types/api';
import { BrandLogo, Button } from '../ui';

interface LoginViewProps {
  health: BffHealth | null;
  authError: string | null;
}

export function LoginView({ health, authError }: LoginViewProps) {
  const { settings } = useTheme();
  const { branding } = settings;
  const displayName = branding.displayName?.trim() || 'Kropy';

  const handleLogin = () => {
    window.location.href = bffClient.loginUrl();
  };

  const healthClass = !health ? 'offline' : health.status === 'healthy' ? 'ok' : 'degraded';
  const healthLabel = !health
    ? 'Sin conexión'
    : health.status === 'healthy'
      ? 'Sistema operativo'
      : 'Servicio con intermitencias';

  return (
    <div className="login-shell">
      <div className="login-card">
        <div className="login-brand">
          <BrandLogo logoUrl={branding.logoUrl} name={displayName} className="login-logo" />
          <div className="stack-xs">
            <h1 className="page-title">{displayName}</h1>
            <p className="page-subtitle">Bienvenido a tu portal de gestión</p>
          </div>
        </div>

        {authError && (
          <div className="alert-banner alert-banner-danger" role="alert">
            <AlertTriangle size={17} />
            <div className="alert-banner-body">{authError}</div>
          </div>
        )}

        <p className="text-sm text-muted">
          Accede de forma segura con tu cuenta corporativa para gestionar clientes, catálogo,
          reservas y la configuración de tu negocio.
        </p>

        <Button
          variant="primary"
          size="lg"
          block
          icon={<LogIn size={17} />}
          onClick={handleLogin}
        >
          Iniciar sesión
        </Button>

        {import.meta.env.DEV && (
          <div className="login-features">
            <div className="row-sm font-semibold">
              <KeyRound size={14} />
              <span>Cuentas de prueba (entorno local)</span>
            </div>
            <div>
              <strong>Administrador:</strong> <code>admin@demo.ikedadev.com</code> /{' '}
              <code>Admin123!</code>
            </div>
            <div>
              <strong>Colaborador:</strong> <code>staff@demo.ikedadev.com</code> /{' '}
              <code>Staff123!</code>
            </div>
          </div>
        )}

        <div className="row-between text-xs text-muted">
          <span className="row-sm">
            <ShieldCheck size={14} />
            <span>Estado del servicio</span>
          </span>
          <span className="row-sm">
            <span className={`health-dot ${healthClass}`} />
            <span>{healthLabel}</span>
          </span>
        </div>
      </div>
    </div>
  );
}
