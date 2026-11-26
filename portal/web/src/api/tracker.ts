import type { TrackEventInput } from '../types/api';
import { bffClient } from './bffClient';

const SESSION_STORAGE_KEY = 'ikeda_telemetry_session_id';
const FLUSH_INTERVAL_MS = 2500;
const MAX_BATCH_SIZE = 10;
const HARD_BATCH_LIMIT = 100;

function generateId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `sess_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
}

function getOrCreateSessionId(): string {
  try {
    const existing = window.sessionStorage.getItem(SESSION_STORAGE_KEY);
    if (existing) return existing;
    const created = generateId();
    window.sessionStorage.setItem(SESSION_STORAGE_KEY, created);
    return created;
  } catch {
    return generateId();
  }
}

class TelemetryTracker {
  private queue: TrackEventInput[] = [];
  private timer: ReturnType<typeof setTimeout> | null = null;
  private flushing = false;
  private sessionId: string;
  private listenersBound = false;

  constructor() {
    this.sessionId = getOrCreateSessionId();
    this.bindLifecycleListeners();
  }

  private bindLifecycleListeners() {
    if (this.listenersBound || typeof window === 'undefined') return;
    this.listenersBound = true;

    const handleVisibilityChange = () => {
      if (document.visibilityState === 'hidden') {
        void this.flush(true);
      }
    };

    const handlePageHide = () => {
      void this.flush(true);
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);
    window.addEventListener('pagehide', handlePageHide);
  }

  public getSessionId(): string {
    return this.sessionId;
  }

  public track(
    eventName: string,
    properties?: Record<string, unknown>,
    extra?: Partial<Omit<TrackEventInput, 'event_name' | 'properties'>>,
  ): void {
    const event: TrackEventInput = {
      kind: 'TRACKING',
      event_name: eventName,
      source: 'web',
      session_id: this.sessionId,
      occurred_at: new Date().toISOString(),
      properties: properties ?? {},
      context: {
        path: typeof window !== 'undefined' ? window.location.pathname : '',
        ...(extra?.context ?? {}),
      },
      ...extra,
    };

    this.queue.push(event);

    if (this.queue.length >= MAX_BATCH_SIZE) {
      void this.flush();
      return;
    }

    if (!this.timer) {
      this.timer = setTimeout(() => {
        this.timer = null;
        void this.flush();
      }, FLUSH_INTERVAL_MS);
    }
  }

  public trackPageView(section: string, properties?: Record<string, unknown>): void {
    this.track('page.viewed', {
      section,
      ...properties,
    });
  }

  public async flush(keepalive = false): Promise<void> {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }

    if (this.flushing || this.queue.length === 0) return;

    const batch = this.queue.splice(0, HARD_BATCH_LIMIT);
    this.flushing = true;

    try {
      if (keepalive && typeof fetch === 'function') {
        await fetch(`${bffClient.baseUrl}/api/telemetry/track`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            Accept: 'application/json',
          },
          credentials: 'include',
          keepalive: true,
          body: JSON.stringify({ events: batch }),
        });
      } else {
        await bffClient.trackTelemetryEvents(batch);
      }
    } catch {
      // Silently ignore telemetry network/HTTP errors so UX is never disrupted.
    } finally {
      this.flushing = false;
      if (this.queue.length > 0 && !keepalive) {
        void this.flush();
      }
    }
  }
}

export const tracker = new TelemetryTracker();
