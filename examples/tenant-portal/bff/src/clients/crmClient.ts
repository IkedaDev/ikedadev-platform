import { config } from '../config.js';
import type {
  Client,
  CreateClientCommand,
  Criteria,
  PaginatedApiResponse,
  UpdateClientCommand,
} from '../types/platform.js';
import { ServiceHttpClient, type RequestContext } from './http.js';

const http = new ServiceHttpClient({
  serviceName: 'crm-api',
  baseUrl: config.CRM_API_URL,
});

export const crmClient = {
  health: () => http.health(),

  searchClients(criteria: Criteria, ctx?: RequestContext): Promise<PaginatedApiResponse<Client>> {
    return http.request<PaginatedApiResponse<Client>>('POST', '/v1/clients/search', {
      body: criteria,
      ctx,
    });
  },

  getClient(id: string, ctx?: RequestContext): Promise<Client> {
    return http.requestData<Client>('GET', `/v1/clients/${id}`, { ctx });
  },

  createClient(cmd: CreateClientCommand, ctx?: RequestContext): Promise<Client> {
    return http.requestData<Client>('POST', '/v1/clients', { body: cmd, ctx });
  },

  updateClient(id: string, cmd: UpdateClientCommand, ctx?: RequestContext): Promise<Client> {
    return http.requestData<Client>('PATCH', `/v1/clients/${id}`, { body: cmd, ctx });
  },

  deleteClient(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/clients/${id}`, { ctx });
  },

  async findByIds(ids: string[], ctx?: RequestContext): Promise<Client[]> {
    const unique = Array.from(new Set(ids.filter(Boolean)));
    if (unique.length === 0) return [];
    const res = await this.searchClients(
      {
        filter: {
          and: [{ field: 'id', operator: 'in', value: unique }],
        },
        pagination: { page: 1, limit: Math.min(unique.length, 100) },
      },
      ctx,
    );
    return res.data;
  },
};
