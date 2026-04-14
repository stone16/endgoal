"use client";

import type { WsFrontendMessage } from "@/bindings/WsFrontendMessage";
import { getFrontendWebSocketUrl } from "@/lib/api";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  type ReactNode,
} from "react";

type RealtimeListener = (message: WsFrontendMessage) => void;

type RealtimeContextValue = {
  subscribe: (listener: RealtimeListener) => () => void;
};

const RealtimeContext = createContext<RealtimeContextValue | null>(null);

function parseRealtimeMessage(data: string): WsFrontendMessage | null {
  try {
    const parsed = JSON.parse(data) as Partial<WsFrontendMessage>;

    if (typeof parsed.type !== "string" || typeof parsed.id !== "string") {
      return null;
    }

    return {
      type: parsed.type,
      id: parsed.id,
    };
  } catch {
    return null;
  }
}

export function RealtimeProvider({ children }: { children: ReactNode }) {
  const listenersRef = useRef<Set<RealtimeListener>>(new Set());
  const reconnectTimerRef = useRef<number | null>(null);
  const shouldReconnectRef = useRef(true);

  const subscribe = useCallback((listener: RealtimeListener) => {
    listenersRef.current.add(listener);

    return () => {
      listenersRef.current.delete(listener);
    };
  }, []);

  useEffect(() => {
    let socket: WebSocket | null = null;
    let reconnectDelay = 500;

    const clearReconnectTimer = () => {
      if (reconnectTimerRef.current !== null) {
        window.clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
    };

    const scheduleReconnect = () => {
      if (!shouldReconnectRef.current) {
        return;
      }

      clearReconnectTimer();
      reconnectTimerRef.current = window.setTimeout(() => {
        connect();
      }, reconnectDelay);
      reconnectDelay = Math.min(reconnectDelay * 2, 5000);
    };

    const connect = () => {
      clearReconnectTimer();
      socket = new WebSocket(getFrontendWebSocketUrl());

      socket.onopen = () => {
        reconnectDelay = 500;
      };

      socket.onmessage = (event) => {
        if (typeof event.data !== "string") {
          return;
        }

        const message = parseRealtimeMessage(event.data);

        if (!message) {
          return;
        }

        listenersRef.current.forEach((listener) => {
          listener(message);
        });
      };

      socket.onerror = () => {
        socket?.close();
      };

      socket.onclose = () => {
        scheduleReconnect();
      };
    };

    shouldReconnectRef.current = true;
    connect();

    return () => {
      shouldReconnectRef.current = false;
      clearReconnectTimer();
      socket?.close();
    };
  }, []);

  const value = useMemo<RealtimeContextValue>(
    () => ({
      subscribe,
    }),
    [subscribe],
  );

  return (
    <RealtimeContext.Provider value={value}>
      {children}
    </RealtimeContext.Provider>
  );
}

export function useRealtimeSubscription(listener: RealtimeListener) {
  const context = useContext(RealtimeContext);
  const listenerRef = useRef(listener);

  useEffect(() => {
    listenerRef.current = listener;
  }, [listener]);

  useEffect(() => {
    if (!context) {
      return undefined;
    }

    return context.subscribe((message) => {
      listenerRef.current(message);
    });
  }, [context]);
}
