import { config } from '../config.js';
import type {
  AvailabilityResult,
  Calendar,
  CreateCalendarCommand,
  CreateEventCommand,
  EventInstance,
  EventRecord,
} from '../types/platform.js';
import { ServiceHttpClient, type RequestContext } from './http.js';

const http = new ServiceHttpClient({
  serviceName: 'events-api',
  baseUrl: config.EVENTS_API_URL,
});

export const eventsClient = {
  health: () => http.health(),

  listCalendars(ctx?: RequestContext): Promise<Calendar[]> {
    return http.requestData<Calendar[]>('GET', '/v1/calendars', { ctx });
  },

  getCalendar(id: string, ctx?: RequestContext): Promise<Calendar> {
    return http.requestData<Calendar>('GET', `/v1/calendars/${encodeURIComponent(id)}`, { ctx });
  },

  createCalendar(cmd: CreateCalendarCommand, ctx?: RequestContext): Promise<Calendar> {
    return http.requestData<Calendar>('POST', '/v1/calendars', { body: cmd, ctx });
  },

  getCalendarAvailability(
    calendarId: string,
    params: { starts_at: string; ends_at: string; slot_duration_minutes?: number },
    ctx?: RequestContext,
  ): Promise<AvailabilityResult> {
    return http.requestData<AvailabilityResult>(
      'GET',
      `/v1/calendars/${encodeURIComponent(calendarId)}/availability`,
      {
        query: params,
        ctx,
      },
    );
  },

  listEventsByRange(
    params: { calendar_id: string; starts_at: string; ends_at: string },
    ctx?: RequestContext,
  ): Promise<EventInstance[]> {
    return http.requestData<EventInstance[]>('GET', '/v1/events', {
      query: params,
      ctx,
    });
  },

  getEvent(id: string, ctx?: RequestContext): Promise<EventRecord> {
    return http.requestData<EventRecord>('GET', `/v1/events/${encodeURIComponent(id)}`, { ctx });
  },

  createEvent(cmd: CreateEventCommand, ctx?: RequestContext): Promise<EventRecord> {
    return http.requestData<EventRecord>('POST', '/v1/events', { body: cmd, ctx });
  },

  deleteEvent(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/events/${encodeURIComponent(id)}`, { ctx });
  },
};
