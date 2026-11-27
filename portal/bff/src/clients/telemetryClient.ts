import { config } from '../config.js';
import type {
  Criteria,
  IngestBatchResponse,
  IngestTelemetryEventPayload,
  PaginatedApiResponse,
  TelemetryEvent,
  TelemetryEventKind,
  TelemetryMetricsQuery,
  TelemetryMetricsSummary,
} from '../types/platform.js';
import { ServiceHttpClient, type RequestContext } from './http.js';

const UUID_REGEX =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function normalizeUuid(value?: string | null): string | null {
  if (!value) return null;
  const trimmed = value.trim();
  return UUID_REGEX.test(trimmed) ? trimmed : null;
}

const http = new ServiceHttpClient({
  serviceName: 'telemetry-api',
  baseUrl: config.TELEMETRY_API_URL,
});

export interface AuditEventInput {
  event_name: string;
  resource_type: string;
  resource_id?: string | null;
  client_id?: string | null;
  actor_id?: string | null;
  request_id?: string | null;
  properties?: Record<string, unknown>;
  context?: Record<string, unknown>;
}

export const telemetryClient = {
  health: () => http.health(),

  ingestEvent(
    payload: IngestTelemetryEventPayload,
    ctx?: RequestContext,
  ): Promise<TelemetryEvent> {
    return http.requestData<TelemetryEvent>('POST', '/v1/events', {
      body: payload,
      ctx,
    });
  },

  ingestBatch(
    events: IngestTelemetryEventPayload[],
    ctx?: RequestContext,
  ): Promise<IngestBatchResponse> {
    return http.requestData<IngestBatchResponse>('POST', '/v1/events/batch', {
      body: { events },
      ctx,
    });
  },

  searchEvents(
    criteria: Criteria,
    ctx?: RequestContext,
  ): Promise<PaginatedApiResponse<TelemetryEvent>> {
    return http.request<PaginatedApiResponse<TelemetryEvent>>('POST', '/v1/events/search', {
      body: criteria,
      ctx,
    });
  },

  getMetrics(
    query: TelemetryMetricsQuery,
    ctx?: RequestContext,
  ): Promise<TelemetryMetricsSummary> {
    return http.requestData<TelemetryMetricsSummary>('POST', '/v1/metrics/summary', {
      body: query,
      ctx,
    });
  },

  getClientTimeline(
    clientId: string,
    params?: { page?: number; limit?: number; kind?: TelemetryEventKind },
    ctx?: RequestContext,
  ): Promise<PaginatedApiResponse<TelemetryEvent>> {
    return http.request<PaginatedApiResponse<TelemetryEvent>>(
      'GET',
      `/v1/clients/${encodeURIComponent(clientId)}/timeline`,
      {
        query: params,
        ctx,
      },
    );
  },

  /**
   * Fire-and-forget audit event emitter. Never throws or blocks the caller response.
   */
  emitAuditEventAsync(input: AuditEventInput, ctx?: RequestContext): void {
    const payload: IngestTelemetryEventPayload = {
      kind: 'AUDIT',
      event_name: input.event_name,
      source: 'bff',
      resource_type: input.resource_type,
      resource_id: normalizeUuid(input.resource_id),
      client_id: normalizeUuid(input.client_id),
      actor_id: normalizeUuid(input.actor_id ?? ctx?.actorId),
      request_id: input.request_id ?? ctx?.requestId ?? null,
      properties: input.properties ?? {},
      context: {
        service: 'tenant-portal-bff',
        ...(input.context ?? {}),
      },
    };

    void this.ingestEvent(payload, ctx).catch((err) => {
      const reason = err instanceof Error ? err.message : String(err);
      console.warn(
        `[BFF Telemetry] Failed to emit AUDIT event "${input.event_name}" (request_id=${payload.request_id ?? 'none'}): ${reason}`,
      );
    });
  },
};
