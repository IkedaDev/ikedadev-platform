import { config } from '../config.js';
import type {
  BookingRecord,
  CreateBookingPayload,
  Criteria,
  PaginatedApiResponse,
  TransitionBookingStatusPayload,
} from '../types/platform.js';
import { ServiceHttpClient, type RequestContext } from './http.js';

const http = new ServiceHttpClient({
  serviceName: 'bookings-api',
  baseUrl: config.BOOKINGS_API_URL,
});

export const bookingsClient = {
  health: () => http.health(),

  createBooking(
    cmd: CreateBookingPayload,
    ctx?: RequestContext,
    idempotencyKey?: string,
  ): Promise<BookingRecord> {
    return http.requestData<BookingRecord>('POST', '/v1/bookings', {
      body: cmd,
      ctx,
      extraHeaders: idempotencyKey ? { 'Idempotency-Key': idempotencyKey } : undefined,
    });
  },

  getBooking(id: string, ctx?: RequestContext): Promise<BookingRecord> {
    return http.requestData<BookingRecord>('GET', `/v1/bookings/${id}`, { ctx });
  },

  searchBookings(
    criteria: Criteria,
    ctx?: RequestContext,
  ): Promise<PaginatedApiResponse<BookingRecord>> {
    return http.request<PaginatedApiResponse<BookingRecord>>('POST', '/v1/bookings/search', {
      body: criteria,
      ctx,
    });
  },

  transitionStatus(
    id: string,
    cmd: TransitionBookingStatusPayload,
    ctx?: RequestContext,
  ): Promise<BookingRecord> {
    return http.requestData<BookingRecord>('PATCH', `/v1/bookings/${id}/status`, {
      body: cmd,
      ctx,
    });
  },

  deleteBooking(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/bookings/${id}`, { ctx });
  },
};
