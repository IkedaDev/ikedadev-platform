import { config } from '../config.js';
import type {
  ApiKeySummary,
  CreateApiKeyCommand,
  CreateTenantUserCommand,
  CreatedApiKey,
  TenantUser,
} from '../types/platform.js';
import { ServiceHttpClient, type RequestContext } from './http.js';

const http = new ServiceHttpClient({
  serviceName: 'identity-api',
  baseUrl: config.IDENTITY_API_URL,
});

export const identityClient = {
  health: () => http.health(),

  listUsers(ctx?: RequestContext): Promise<TenantUser[]> {
    return http.requestData<TenantUser[]>('GET', '/v1/users', { ctx });
  },

  createUser(cmd: CreateTenantUserCommand, ctx?: RequestContext): Promise<TenantUser> {
    return http.requestData<TenantUser>('POST', '/v1/users', { body: cmd, ctx });
  },

  deleteUser(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/users/${id}`, { ctx });
  },

  listApiKeys(ctx?: RequestContext): Promise<ApiKeySummary[]> {
    return http.requestData<ApiKeySummary[]>('GET', '/v1/api-keys', { ctx });
  },

  createApiKey(cmd: CreateApiKeyCommand, ctx?: RequestContext): Promise<CreatedApiKey> {
    return http.requestData<CreatedApiKey>('POST', '/v1/api-keys', { body: cmd, ctx });
  },

  revokeApiKey(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/api-keys/${id}`, { ctx });
  },
};
