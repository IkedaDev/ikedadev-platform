import { Hono } from 'hono';
import { deleteCookie, getCookie, setCookie } from 'hono/cookie';
import {
  computePkceChallenge,
  createSession,
  deleteSession,
  exchangeCodeForTokens,
  generateRandomUrlSafe,
  getValidSession,
  keycloakAuthUrl,
  keycloakLogoutUrl,
} from '../auth/sessionStore.js';
import { config } from '../config.js';

const OIDC_TX_COOKIE = 'ikeda_oidc_tx';
const IS_HTTPS = config.BFF_PUBLIC_URL.startsWith('https://');

interface OidcTx {
  state: string;
  codeVerifier: string;
}

export const authRoutes = new Hono();

// GET /auth/login — Starts OAuth2 Authorization Code + PKCE flow with Keycloak
authRoutes.get('/login', (c) => {
  const state = generateRandomUrlSafe(24);
  const codeVerifier = generateRandomUrlSafe(48);
  const codeChallenge = computePkceChallenge(codeVerifier);
  const redirectUri = `${config.BFF_PUBLIC_URL.replace(/\/+$/, '')}/auth/callback`;

  const txPayload = Buffer.from(JSON.stringify({ state, codeVerifier } satisfies OidcTx)).toString(
    'base64url',
  );

  setCookie(c, OIDC_TX_COOKIE, txPayload, {
    httpOnly: true,
    secure: IS_HTTPS,
    sameSite: 'Lax',
    path: '/auth',
    maxAge: 300, // 5 minutes
  });

  const url = new URL(keycloakAuthUrl());
  url.searchParams.set('client_id', config.KEYCLOAK_CLIENT_ID);
  url.searchParams.set('response_type', 'code');
  url.searchParams.set('scope', 'openid profile email');
  url.searchParams.set('redirect_uri', redirectUri);
  url.searchParams.set('state', state);
  url.searchParams.set('code_challenge', codeChallenge);
  url.searchParams.set('code_challenge_method', 'S256');

  return c.redirect(url.toString());
});

// GET /auth/callback — Exchanges authorization code for Keycloak tokens and sets HttpOnly session cookie
authRoutes.get('/callback', async (c) => {
  const error = c.req.query('error');
  const errorDescription = c.req.query('error_description');
  if (error) {
    const redirectErr = new URL(config.WEB_APP_URL);
    redirectErr.searchParams.set('auth_error', errorDescription || error);
    return c.redirect(redirectErr.toString());
  }

  const code = c.req.query('code');
  const state = c.req.query('state');
  const rawTx = getCookie(c, OIDC_TX_COOKIE);
  deleteCookie(c, OIDC_TX_COOKIE, { path: '/auth' });

  if (!code || !state || !rawTx) {
    const redirectErr = new URL(config.WEB_APP_URL);
    redirectErr.searchParams.set('auth_error', 'Parámetros OIDC inválidos o expirados');
    return c.redirect(redirectErr.toString());
  }

  let tx: OidcTx;
  try {
    tx = JSON.parse(Buffer.from(rawTx, 'base64url').toString('utf8')) as OidcTx;
  } catch {
    const redirectErr = new URL(config.WEB_APP_URL);
    redirectErr.searchParams.set('auth_error', 'Transacción OIDC corrupta');
    return c.redirect(redirectErr.toString());
  }

  if (tx.state !== state) {
    const redirectErr = new URL(config.WEB_APP_URL);
    redirectErr.searchParams.set('auth_error', 'CSRF state mismatch');
    return c.redirect(redirectErr.toString());
  }

  const redirectUri = `${config.BFF_PUBLIC_URL.replace(/\/+$/, '')}/auth/callback`;

  try {
    const tokens = await exchangeCodeForTokens({
      code,
      codeVerifier: tx.codeVerifier,
      redirectUri,
    });
    const session = createSession(tokens);

    setCookie(c, config.SESSION_COOKIE_NAME, session.id, {
      httpOnly: true,
      secure: IS_HTTPS,
      sameSite: 'Lax',
      path: '/',
      maxAge: 60 * 60 * 8, // 8 hours
    });

    return c.redirect(config.WEB_APP_URL);
  } catch (err) {
    console.error('[OIDC Callback Error]', err);
    const redirectErr = new URL(config.WEB_APP_URL);
    redirectErr.searchParams.set('auth_error', 'Error autenticando con Keycloak');
    return c.redirect(redirectErr.toString());
  }
});

// GET /auth/me — Returns current authenticated user profile (auto-refreshes token if needed)
authRoutes.get('/me', async (c) => {
  const sessionId = getCookie(c, config.SESSION_COOKIE_NAME);
  const session = await getValidSession(sessionId);

  if (!session) {
    deleteCookie(c, config.SESSION_COOKIE_NAME, { path: '/' });
    return c.json({ authenticated: false, user: null }, 401);
  }

  return c.json({
    authenticated: true,
    user: session.user,
    expiresAt: new Date(session.expiresAt).toISOString(),
  });
});

// POST /auth/logout — Destroys BFF session and returns Keycloak SSO end-session URL
authRoutes.post('/logout', (c) => {
  const sessionId = getCookie(c, config.SESSION_COOKIE_NAME);
  const removed = deleteSession(sessionId);
  deleteCookie(c, config.SESSION_COOKIE_NAME, { path: '/' });

  return c.json({
    success: true,
    logoutUrl: keycloakLogoutUrl(removed?.idToken),
  });
});
