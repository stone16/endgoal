"use client";

import type { Run } from "@/bindings/Run";
import { useRealtimeSubscription } from "@/features/realtime/provider";
import { getRun, getRunEventStreamUrl } from "@/lib/api";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

type RunDetailOverlayProps = {
  runId: string | null;
  initialRun: Run | null;
  onClose: () => void;
};

type RunStreamEvent = {
  run_id: string | null;
  seq: number;
  event_type: string;
  data_text: string | null;
  created_at: string;
};

type StreamState = "connecting" | "live" | "reconnecting" | "closed";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isTerminalStatus(status: string): boolean {
  return status === "completed" || status === "complete" || status === "failed";
}

function runStatusClass(status: string): string {
  switch (status) {
    case "completed":
    case "complete":
      return "bg-emerald-50 text-emerald-700 ring-emerald-200";
    case "failed":
      return "bg-rose-50 text-rose-700 ring-rose-200";
    case "running":
      return "bg-indigo-50 text-indigo-700 ring-indigo-200";
    default:
      return "bg-stone-100 text-stone-600 ring-stone-200";
  }
}

function formatTimestamp(isoTimestamp: string | null): string {
  if (!isoTimestamp) {
    return "unset";
  }

  const timestamp = new Date(isoTimestamp);

  if (Number.isNaN(timestamp.getTime())) {
    return "time unknown";
  }

  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(timestamp);
}

function formatJsonPrimitive(value: unknown): string {
  if (typeof value === "string") {
    return JSON.stringify(value);
  }

  if (value === null) {
    return "null";
  }

  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }

  return JSON.stringify(value);
}

function JsonTree({
  label,
  value,
  depth = 0,
}: {
  label: string;
  value: unknown;
  depth?: number;
}) {
  if (Array.isArray(value)) {
    return (
      <details open={depth < 2} className="group">
        <summary className="cursor-pointer rounded-md px-2 py-1 font-mono text-xs text-stone-700 hover:bg-stone-100">
          <span className="font-semibold text-stone-950">{label}</span>: [
          {value.length}]
        </summary>
        <div className="ml-4 border-l border-stone-200 pl-3">
          {value.map((item, index) => (
            <JsonTree
              key={`${label}-${index}`}
              label={String(index)}
              value={item}
              depth={depth + 1}
            />
          ))}
        </div>
      </details>
    );
  }

  if (isRecord(value)) {
    const entries = Object.entries(value);

    return (
      <details open={depth < 2} className="group">
        <summary className="cursor-pointer rounded-md px-2 py-1 font-mono text-xs text-stone-700 hover:bg-stone-100">
          <span className="font-semibold text-stone-950">{label}</span>: {"{"}
          {entries.length}
          {"}"}
        </summary>
        <div className="ml-4 border-l border-stone-200 pl-3">
          {entries.map(([key, entryValue]) => (
            <JsonTree
              key={key}
              label={key}
              value={entryValue}
              depth={depth + 1}
            />
          ))}
        </div>
      </details>
    );
  }

  return (
    <div className="rounded-md px-2 py-1 font-mono text-xs text-stone-700">
      <span className="font-semibold text-stone-950">{label}</span>:{" "}
      <span>{formatJsonPrimitive(value)}</span>
    </div>
  );
}

function parseInputSnapshot(raw: string | null | undefined): {
  value: unknown | null;
  error: string | null;
} {
  if (!raw) {
    return { value: null, error: null };
  }

  try {
    return { value: JSON.parse(raw) as unknown, error: null };
  } catch {
    return { value: null, error: "Input snapshot is not valid JSON" };
  }
}

function parseStreamEvent(
  data: string,
  eventType: string,
): RunStreamEvent | null {
  try {
    const parsed = JSON.parse(data) as Record<string, unknown>;
    const rawSeq = parsed.seq;
    const seq =
      typeof rawSeq === "number"
        ? rawSeq
        : typeof rawSeq === "string"
          ? Number(rawSeq)
          : Date.now();

    return {
      run_id:
        typeof parsed.run_id === "string" || parsed.run_id === null
          ? parsed.run_id
          : null,
      seq: Number.isFinite(seq) ? seq : Date.now(),
      event_type:
        typeof parsed.event_type === "string" ? parsed.event_type : eventType,
      data_text:
        typeof parsed.data_text === "string" || parsed.data_text === null
          ? parsed.data_text
          : null,
      created_at:
        typeof parsed.created_at === "string"
          ? parsed.created_at
          : new Date().toISOString(),
    };
  } catch {
    return null;
  }
}

function streamStateText(streamState: StreamState): string {
  switch (streamState) {
    case "connecting":
      return "connecting";
    case "live":
      return "live";
    case "reconnecting":
      return "reconnecting";
    case "closed":
      return "closed";
  }
}

export function RunDetailOverlay({
  runId,
  initialRun,
  onClose,
}: RunDetailOverlayProps) {
  if (!runId) {
    return null;
  }

  return (
    <RunDetailOverlayContent
      key={runId}
      initialRun={initialRun}
      runId={runId}
      onClose={onClose}
    />
  );
}

function RunDetailOverlayContent({
  runId,
  initialRun,
  onClose,
}: {
  runId: string;
  initialRun: Run | null;
  onClose: () => void;
}) {
  const [fetchedRun, setFetchedRun] = useState<Run | null>(null);
  const [streamEvents, setStreamEvents] = useState<RunStreamEvent[]>([]);
  const [streamState, setStreamState] = useState<StreamState>("connecting");
  const [loadError, setLoadError] = useState<string | null>(null);
  const terminalRef = useRef<HTMLDivElement | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);
  const run = fetchedRun ?? initialRun;

  const refreshRun = useCallback(async () => {
    try {
      const nextRun = await getRun(runId);
      setFetchedRun(nextRun);
      setLoadError(null);
    } catch (error) {
      setLoadError(
        error instanceof Error ? error.message : "Run failed to load",
      );
    }
  }, [runId]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.stopImmediatePropagation();
        onClose();
      }
    }

    document.addEventListener("keydown", handleKeyDown, true);

    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [onClose, runId]);

  useEffect(() => {
    const activeRunId = runId;
    const source = new EventSource(getRunEventStreamUrl(activeRunId));
    eventSourceRef.current = source;

    function appendEvent(nextEvent: RunStreamEvent) {
      setStreamEvents((currentEvents) => {
        if (
          currentEvents.some(
            (event) =>
              event.seq === nextEvent.seq &&
              event.event_type === nextEvent.event_type,
          )
        ) {
          return currentEvents;
        }

        return [...currentEvents, nextEvent].sort(
          (left, right) => left.seq - right.seq,
        );
      });
    }

    function handleStreamMessage(event: MessageEvent<string>) {
      const parsed = parseStreamEvent(event.data, event.type);

      if (!parsed) {
        return;
      }

      setStreamState("live");
      appendEvent(parsed);
    }

    function handleOpen() {
      setStreamState("live");
    }

    function handleError() {
      void getRun(activeRunId)
        .then((nextRun) => {
          setFetchedRun(nextRun);

          if (isTerminalStatus(nextRun.status)) {
            source.close();

            if (eventSourceRef.current === source) {
              eventSourceRef.current = null;
            }

            setStreamState("closed");
            return;
          }

          setStreamState("reconnecting");
        })
        .catch(() => {
          setStreamState("reconnecting");
        });
    }

    source.addEventListener("open", handleOpen);
    source.addEventListener("stdout", handleStreamMessage);
    source.addEventListener("stderr", handleStreamMessage);
    source.addEventListener("system", handleStreamMessage);
    source.addEventListener("error", handleError);

    return () => {
      source.removeEventListener("open", handleOpen);
      source.removeEventListener("stdout", handleStreamMessage);
      source.removeEventListener("stderr", handleStreamMessage);
      source.removeEventListener("system", handleStreamMessage);
      source.removeEventListener("error", handleError);
      source.close();

      if (eventSourceRef.current === source) {
        eventSourceRef.current = null;
      }
    };
  }, [runId]);

  useEffect(() => {
    if (!terminalRef.current) {
      return;
    }

    terminalRef.current.scrollTop = terminalRef.current.scrollHeight;
  }, [streamEvents]);

  useRealtimeSubscription((message) => {
    if (message.type === "run:updated" && message.id === runId) {
      void refreshRun();
    }
  });

  const inputSnapshot = useMemo(
    () => parseInputSnapshot(run?.input_snapshot_json),
    [run?.input_snapshot_json],
  );

  const groupedEvents = useMemo(() => {
    const groups = new Map<string, RunStreamEvent[]>();

    for (const event of streamEvents) {
      const group = groups.get(event.event_type) ?? [];
      group.push(event);
      groups.set(event.event_type, group);
    }

    return Array.from(groups.entries());
  }, [streamEvents]);

  return (
    <div
      data-node-panel-overlay="true"
      className="fixed inset-0 z-50 bg-stone-950/20"
      role="dialog"
      aria-label="Run detail"
    >
      <div className="absolute bottom-0 right-0 top-0 flex w-full max-w-3xl flex-col border-l border-stone-200 bg-white shadow-[-16px_0_36px_rgba(28,25,23,0.18)]">
        <div className="flex items-center justify-between gap-4 border-b border-stone-200 px-5 py-4">
          <div>
            <div className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
              Run
            </div>
            <div className="mt-1 font-mono text-xs text-stone-500">{runId}</div>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="inline-flex h-9 items-center rounded-md border border-stone-300 px-3 text-sm font-medium text-stone-600 transition-colors hover:border-stone-400 hover:text-stone-950"
          >
            Return to panel
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-6">
          {loadError ? (
            <div className="mb-5 rounded-lg border border-rose-200 bg-rose-50 p-4 text-sm text-rose-700">
              {loadError}
            </div>
          ) : null}

          {!run ? (
            <div className="space-y-4">
              <div className="h-8 w-32 animate-pulse rounded-md bg-stone-100" />
              <div className="h-28 animate-pulse rounded-lg bg-stone-100" />
              <div className="h-56 animate-pulse rounded-lg bg-stone-100" />
            </div>
          ) : (
            <div className="space-y-5">
              <section className="rounded-lg border border-stone-200 p-4">
                <div className="flex flex-wrap items-center gap-3">
                  <span
                    className={`inline-flex rounded-md px-2 py-1 text-xs font-semibold ring-1 ring-inset ${runStatusClass(
                      run.status,
                    )}`}
                  >
                    {run.status}
                  </span>
                  <span className="inline-flex rounded-md bg-stone-100 px-2 py-1 text-xs font-semibold text-stone-600 ring-1 ring-inset ring-stone-200">
                    stream {streamStateText(streamState)}
                  </span>
                </div>
                <dl className="mt-4 grid gap-3 text-sm sm:grid-cols-2">
                  <div>
                    <dt className="text-stone-500">Type</dt>
                    <dd className="mt-1 font-medium text-stone-950">
                      {run.type}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-stone-500">Runtime</dt>
                    <dd className="mt-1 font-medium text-stone-950">
                      {run.runtime}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-stone-500">Started</dt>
                    <dd className="mt-1 text-stone-800">
                      {formatTimestamp(run.started_at)}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-stone-500">Ended</dt>
                    <dd className="mt-1 text-stone-800">
                      {formatTimestamp(run.ended_at)}
                    </dd>
                  </div>
                </dl>
              </section>

              <section className="rounded-lg border border-stone-200 p-4">
                <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
                  Input Snapshot
                </h2>
                {inputSnapshot.error ? (
                  <p className="mt-3 text-sm text-rose-700">
                    {inputSnapshot.error}
                  </p>
                ) : null}
                {!inputSnapshot.value && !inputSnapshot.error ? (
                  <p className="mt-3 text-sm text-stone-500">
                    No input snapshot
                  </p>
                ) : null}
                {inputSnapshot.value ? (
                  <div className="mt-3 rounded-md border border-stone-200 bg-stone-50 p-3">
                    <JsonTree
                      label="run_input_snapshot"
                      value={inputSnapshot.value}
                    />
                  </div>
                ) : null}
              </section>

              <section className="rounded-lg border border-stone-200 p-4">
                <div className="flex items-center justify-between gap-3">
                  <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
                    Stdout
                  </h2>
                  <span className="text-xs text-stone-500">
                    {streamEvents.length} events
                  </span>
                </div>
                <div
                  ref={terminalRef}
                  className="mt-3 max-h-72 overflow-y-auto rounded-md bg-stone-950 p-3 font-mono text-xs leading-6 text-stone-50"
                >
                  {streamEvents.length === 0 ? (
                    <div className="text-stone-400">Waiting for output</div>
                  ) : null}
                  {streamEvents.map((event) => (
                    <div key={`${event.event_type}-${event.seq}`}>
                      <span className="text-stone-400">
                        [{event.event_type}]
                      </span>{" "}
                      <span>{event.data_text ?? ""}</span>
                    </div>
                  ))}
                </div>
              </section>

              <section className="rounded-lg border border-stone-200 p-4">
                <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
                  Audit Trail
                </h2>
                {groupedEvents.length === 0 ? (
                  <p className="mt-3 text-sm text-stone-500">
                    No events received
                  </p>
                ) : null}
                <div className="mt-4 space-y-4">
                  {groupedEvents.map(([eventType, events]) => (
                    <div key={eventType}>
                      <div className="text-sm font-semibold text-stone-950">
                        {eventType}
                      </div>
                      <div className="mt-2 space-y-2 border-l border-stone-200 pl-3">
                        {events.map((event) => (
                          <div
                            key={`${event.event_type}-${event.seq}`}
                            className="text-sm text-stone-700"
                          >
                            <div className="font-mono text-xs text-stone-500">
                              #{event.seq} · {formatTimestamp(event.created_at)}
                            </div>
                            <div className="mt-1 leading-6">
                              {event.data_text ?? "(no text)"}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              </section>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
