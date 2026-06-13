import { useEffect, useRef, useState } from "react";
import { sseUrl } from "./auth";

export type Level = "trace" | "debug" | "info" | "warn" | "error";

export interface ControlEvent {
  id: number;
  level: Level;
  subsystem: string | null;
  target: string;
  message: string;
  fields: Record<string, unknown>;
  timestamp: number;
}

const MAX_EVENTS = 500;

export function useEvents(): {
  events: ControlEvent[];
  connected: boolean;
  laggedCount: number;
} {
  const [events, setEvents] = useState<ControlEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const [laggedCount, setLaggedCount] = useState(0);
  const reconnectRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    let src: EventSource | null = null;

    const connect = () => {
      if (cancelled) return;
      src = new EventSource(sseUrl("/api/events"));
      src.onopen = () => setConnected(true);
      src.onerror = () => {
        setConnected(false);
        src?.close();
        if (!cancelled) {
          reconnectRef.current = window.setTimeout(connect, 2000);
        }
      };
      src.onmessage = (e) => {
        try {
          const ev = JSON.parse(e.data) as ControlEvent;
          setEvents((prev) => {
            const next = [...prev, ev];
            return next.length > MAX_EVENTS ? next.slice(-MAX_EVENTS) : next;
          });
        } catch {
          // ignore malformed
        }
      };
      src.addEventListener("lagged", (e) => {
        const n = Number((e as MessageEvent).data) || 0;
        setLaggedCount((c) => c + n);
      });
    };

    connect();
    return () => {
      cancelled = true;
      if (reconnectRef.current !== null) {
        window.clearTimeout(reconnectRef.current);
      }
      src?.close();
    };
  }, []);

  return { events, connected, laggedCount };
}
