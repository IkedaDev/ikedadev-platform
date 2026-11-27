const ALLOWED_LOGO_PROTOCOLS = new Set(['https:']);

/**
 * Validates and sanitizes a tenant logo URL.
 * Allows:
 * - Single-slash relative paths (e.g. `/logo.png`), rejecting protocol-relative `//...`
 * - `https:` URLs
 * - `http:` URLs only in local development or on localhost/127.0.0.1
 * Rejects `javascript:`, `data:`, `vbscript:`, `file:`, and malformed URLs.
 */
export function sanitizeLogoUrl(raw?: string | null): string | null {
  if (typeof raw !== 'string') return null;
  const trimmed = raw.trim();
  if (!trimmed) return null;

  // Reject control characters / whitespace injection
  if (/[\u0000-\u001F\u007F]/.test(trimmed)) return null;

  // Allow single-slash relative path (reject protocol-relative `//`)
  if (trimmed.startsWith('/')) {
    return trimmed.startsWith('//') ? null : trimmed;
  }

  try {
    const parsed = new URL(trimmed);
    if (ALLOWED_LOGO_PROTOCOLS.has(parsed.protocol)) {
      return parsed.toString();
    }
    if (
      parsed.protocol === 'http:' &&
      (import.meta.env.DEV ||
        parsed.hostname === 'localhost' ||
        parsed.hostname === '127.0.0.1')
    ) {
      return parsed.toString();
    }
    return null;
  } catch {
    return null;
  }
}

/**
 * Validates and sanitizes an absolute media/catalog image URL before rendering in `<img src>`.
 * Allows `https:` always, and `http:` only in local development or on localhost/127.0.0.1.
 * Rejects `javascript:`, `data:`, protocol-relative `//...`, control characters, and URLs > 2048 chars.
 */
export function sanitizeImageUrl(raw?: string | null): string | null {
  if (typeof raw !== 'string') return null;
  const trimmed = raw.trim();
  if (!trimmed || trimmed.length > 2048) return null;
  if (/[\u0000-\u001F\u007F]/.test(trimmed)) return null;
  if (trimmed.startsWith('//')) return null;

  try {
    const parsed = new URL(trimmed);
    if (parsed.protocol === 'https:' && parsed.hostname) {
      return parsed.toString();
    }
    if (
      parsed.protocol === 'http:' &&
      parsed.hostname &&
      (import.meta.env.DEV ||
        parsed.hostname === 'localhost' ||
        parsed.hostname === '127.0.0.1')
    ) {
      return parsed.toString();
    }
    return null;
  } catch {
    return null;
  }
}

/**
 * Validates that a redirect URL (e.g. post-logout IdP URL) uses an explicit
 * `https:` or `http:` scheme before assigning `window.location.href`.
 */
export function isSafeRedirectUrl(raw?: string | null): boolean {
  if (typeof raw !== 'string') return false;
  const trimmed = raw.trim();
  if (!trimmed) return false;
  try {
    const base = typeof window !== 'undefined' ? window.location.origin : 'http://localhost';
    const parsed = new URL(trimmed, base);
    return parsed.protocol === 'https:' || parsed.protocol === 'http:';
  } catch {
    return false;
  }
}
