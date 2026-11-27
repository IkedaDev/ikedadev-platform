import { useMemo } from 'react';
import type { NavSection } from '../components/Sidebar';
import type { AuthUser, MediaContext, TenantFeatures } from '../types/api';

export type Permission =
  | 'admin:tenant'
  | 'settings:read'
  | 'settings:write'
  | 'identity:read'
  | 'identity:write'
  | 'crm:read'
  | 'crm:write'
  | 'catalog:read'
  | 'catalog:write'
  | 'events:read'
  | 'events:write'
  | 'bookings:read'
  | 'bookings:write'
  | 'telemetry:read'
  | 'telemetry:write'
  | 'content:read'
  | 'content:write'
  | 'content:publish'
  | 'media:read'
  | 'media:write'
  | 'media:delete'
  | 'media:branding:write';

function buildRoleSet(roles: string[] | undefined | null): Set<string> {
  return new Set((roles ?? []).map((r) => r.trim()).filter(Boolean));
}

function hasAny(set: Set<string>, allowed: readonly string[]): boolean {
  if (set.size === 0) return false;
  if (set.has('*')) return true;
  return allowed.some((r) => set.has(r));
}

export function isAdminRoles(roles: string[] | undefined | null): boolean {
  return hasAny(buildRoleSet(roles), ['tenant:admin', 'platform:admin']);
}

export function can(user: AuthUser | null | undefined, permission: Permission): boolean {
  if (!user) return false;
  const set = buildRoleSet(user.roles);
  const admin = hasAny(set, ['tenant:admin', 'platform:admin']);

  switch (permission) {
    case 'admin:tenant':
    case 'settings:read':
    case 'settings:write':
    case 'identity:read':
    case 'identity:write':
    case 'media:branding:write':
      return admin;

    case 'crm:read':
    case 'catalog:read':
    case 'events:read':
    case 'bookings:read':
      return true;

    case 'crm:write':
      return admin || hasAny(set, ['crm:write']);

    case 'catalog:write':
      return admin || hasAny(set, ['catalog:write']);

    case 'events:write':
      return admin || hasAny(set, ['events:write']);

    case 'bookings:write':
      return admin || hasAny(set, ['bookings:write']);

    case 'telemetry:read':
      return admin || hasAny(set, ['telemetry:read']);

    case 'telemetry:write':
      return admin || hasAny(set, ['telemetry:write']);

    case 'content:publish':
      return admin || hasAny(set, ['content:publish']);

    case 'content:write':
      return admin || hasAny(set, ['content:publish', 'content:write', 'staff', 'tenant:member']);

    case 'content:read':
      return (
        admin ||
        hasAny(set, ['content:publish', 'content:write', 'content:read', 'staff', 'tenant:member'])
      );

    case 'media:delete':
      return admin || hasAny(set, ['media:delete']);

    case 'media:write':
      return admin || hasAny(set, ['media:delete', 'media:write', 'staff', 'tenant:member']);

    case 'media:read':
      return (
        admin ||
        hasAny(set, ['media:delete', 'media:write', 'media:read', 'staff', 'tenant:member'])
      );
  }
}

export function canWriteMediaContext(
  user: AuthUser | null | undefined,
  context: MediaContext,
): boolean {
  if (context === 'branding') {
    return can(user, 'media:branding:write');
  }
  return can(user, 'media:write');
}

export function canAccessSection(
  user: AuthUser | null | undefined,
  section: NavSection,
  features: TenantFeatures,
): boolean {
  if (!user) return false;

  switch (section) {
    case 'dashboard':
      return true;
    case 'clients':
      return features.crm !== false && can(user, 'crm:read');
    case 'catalog':
      return features.catalog !== false && can(user, 'catalog:read');
    case 'bookings':
      return features.bookings !== false && can(user, 'bookings:read');
    case 'content':
      return features.content !== false && can(user, 'content:read');
    case 'media':
      return can(user, 'media:read');
    case 'telemetry':
      return features.telemetry !== false && can(user, 'telemetry:read');
    case 'identity':
      return can(user, 'identity:read');
    case 'settings':
      return can(user, 'settings:read');
  }
}

export function usePermissions(user: AuthUser | null | undefined) {
  return useMemo(
    () => ({
      isAdmin: isAdminRoles(user?.roles),
      can: (permission: Permission) => can(user, permission),
      canWriteMediaContext: (context: MediaContext) => canWriteMediaContext(user, context),
      canAccessSection: (section: NavSection, features: TenantFeatures) =>
        canAccessSection(user, section, features),
    }),
    [user],
  );
}
