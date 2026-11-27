import type { Context } from 'hono';
import type { MediaContext } from '../types/platform.js';

function normalizeRoles(roles: string[] | undefined): Set<string> {
  return new Set((roles ?? []).map((r) => r.trim()).filter(Boolean));
}

export function hasAnyRole(roles: string[] | undefined, allowed: readonly string[]): boolean {
  const set = normalizeRoles(roles);
  if (set.size === 0) return false;
  if (set.has('*')) return true;
  return allowed.some((role) => set.has(role));
}

export function isTenantAdmin(roles: string[] | undefined): boolean {
  return hasAnyRole(roles, ['tenant:admin', 'platform:admin']);
}

export function canReadIdentityUsers(roles: string[] | undefined): boolean {
  return isTenantAdmin(roles) || hasAnyRole(roles, ['identity:read']);
}

export function canWriteEvents(roles: string[] | undefined): boolean {
  return isTenantAdmin(roles) || hasAnyRole(roles, ['events:write']);
}

export function canWriteCrm(roles: string[] | undefined): boolean {
  return isTenantAdmin(roles) || hasAnyRole(roles, ['crm:write']);
}

export function canWriteCatalog(roles: string[] | undefined): boolean {
  return isTenantAdmin(roles) || hasAnyRole(roles, ['catalog:write']);
}

export function canWriteBookings(roles: string[] | undefined): boolean {
  return isTenantAdmin(roles) || hasAnyRole(roles, ['bookings:write']);
}

export function canReadTelemetry(roles: string[] | undefined): boolean {
  return isTenantAdmin(roles) || hasAnyRole(roles, ['telemetry:read']);
}

export function canWriteTelemetry(roles: string[] | undefined): boolean {
  return isTenantAdmin(roles) || hasAnyRole(roles, ['telemetry:write']);
}

export function canPublishContent(roles: string[] | undefined): boolean {
  return isTenantAdmin(roles) || hasAnyRole(roles, ['content:publish']);
}

export function canWriteContent(roles: string[] | undefined): boolean {
  return (
    canPublishContent(roles) ||
    hasAnyRole(roles, ['content:write', 'staff', 'tenant:member'])
  );
}

export function canReadContent(roles: string[] | undefined): boolean {
  return canWriteContent(roles) || hasAnyRole(roles, ['content:read']);
}

export function canDeleteMedia(roles: string[] | undefined): boolean {
  return isTenantAdmin(roles) || hasAnyRole(roles, ['media:delete']);
}

export function canWriteMedia(roles: string[] | undefined): boolean {
  return (
    canDeleteMedia(roles) ||
    hasAnyRole(roles, ['media:write', 'staff', 'tenant:member'])
  );
}

export function canReadMedia(roles: string[] | undefined): boolean {
  return canWriteMedia(roles) || hasAnyRole(roles, ['media:read']);
}

export function canWriteMediaContext(
  roles: string[] | undefined,
  context: MediaContext,
): boolean {
  if (context === 'branding') {
    return isTenantAdmin(roles);
  }
  return canWriteMedia(roles);
}

export function forbiddenResponse(c: Context, message: string) {
  return c.json(
    {
      success: false,
      code: 'FORBIDDEN',
      message,
    },
    403,
  );
}
