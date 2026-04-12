"use client";

import type { AssertionStatus } from "@/bindings/AssertionStatus";
import type { FreezeProposal } from "@/bindings/FreezeProposal";
import type {
  FreezeLayerCompleteEvent,
  FreezeStreamEvent,
} from "@/lib/api";
import {
  commitFreezeSession,
  getActiveFreezeSession,
  getNode,
  respondFreezeSession,
  startFreezeSession,
} from "@/lib/api";
import { parseNodeAcceptance } from "@/features/panel/lib/node-panel-data";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  FREEZE_LAYER_LABEL,
  layerProgressIndex,
  normalizeFreezeLayer,
  parseApprovedFreezeItems,
  parseEditableFreezeItem,
  serializeEditableFreezeItem,
  type ApprovedFreezeItem,
  type EditableFreezeItem,
  type FreezeLayer,
} from "./lib/freeze-session-data";

type FreezeSessionProps = {
  nodeId: string;
};

type SessionMessage = {
  id: string;
  role: "agent" | "user";
  text: string;
};

type PendingProposal = {
  proposal: FreezeProposal;
  editableItem: EditableFreezeItem;
};

type LayerCompletion = {
  layer: FreezeLayer;
  nextLayer: FreezeLayer | null;
};

const LAYERS: FreezeLayer[] = ["assertions", "metrics", "rubric", "complete"];

function isLayerCompleteEvent(
  event: FreezeStreamEvent,
): event is FreezeLayerCompleteEvent {
  return event.event_type === "layer_complete";
}

function nextMessageId() {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}`;
}

function formatApprovedSummary(item: ApprovedFreezeItem): string {
  try {
    const parsed = JSON.parse(item.item_json) as Record<string, unknown>;
    const label =
      typeof parsed.text === "string"
        ? parsed.text
        : typeof parsed.name === "string"
          ? parsed.name
          : typeof parsed.dimension === "string"
            ? parsed.dimension
            : item.item_json;

    return label;
  } catch {
    return item.item_json;
  }
}

function parseNullableNumber(value: string): number | null {
  if (!value.trim()) {
    return null;
  }

  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseRequiredNumber(value: string, fallback: number): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function EditableProposalFields({
  item,
  onChange,
}: {
  item: EditableFreezeItem;
  onChange: (item: EditableFreezeItem) => void;
}) {
  if (item.kind === "assertion") {
    const assertion = item.value;

    return (
      <div className="grid gap-3">
        <label className="grid gap-1 text-sm font-medium text-stone-700">
          Assertion
          <textarea
            value={assertion.text}
            onChange={(event) =>
              onChange({
                kind: "assertion",
                value: { ...assertion, text: event.target.value },
              })
            }
            className="min-h-24 resize-y rounded-md border border-stone-300 bg-white p-3 text-sm leading-6 text-stone-900 outline-none transition-colors focus:border-emerald-500"
          />
        </label>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="grid gap-1 text-sm font-medium text-stone-700">
            Status
            <select
              value={assertion.status}
              onChange={(event) =>
                onChange({
                  kind: "assertion",
                  value: {
                    ...assertion,
                    status: event.target.value as AssertionStatus,
                  },
                })
              }
              className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
            >
              <option value="pending">pending</option>
              <option value="pass">pass</option>
              <option value="fail">fail</option>
            </select>
          </label>
          <label className="grid gap-1 text-sm font-medium text-stone-700">
            Check function
            <input
              value={assertion.check_fn ?? ""}
              onChange={(event) =>
                onChange({
                  kind: "assertion",
                  value: {
                    ...assertion,
                    check_fn: event.target.value.trim() || null,
                  },
                })
              }
              className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
              placeholder="optional"
            />
          </label>
        </div>
      </div>
    );
  }

  if (item.kind === "metric") {
    const metric = item.value;

    return (
      <div className="grid gap-3">
        <label className="grid gap-1 text-sm font-medium text-stone-700">
          Metric name
          <input
            value={metric.name}
            onChange={(event) =>
              onChange({
                kind: "metric",
                value: { ...metric, name: event.target.value },
              })
            }
            className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
          />
        </label>
        <div className="grid gap-3 sm:grid-cols-3">
          <label className="grid gap-1 text-sm font-medium text-stone-700">
            Baseline
            <input
              type="number"
              value={metric.baseline ?? ""}
              onChange={(event) =>
                onChange({
                  kind: "metric",
                  value: {
                    ...metric,
                    baseline: parseNullableNumber(event.target.value),
                  },
                })
              }
              className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
            />
          </label>
          <label className="grid gap-1 text-sm font-medium text-stone-700">
            Current
            <input
              type="number"
              value={metric.current ?? ""}
              onChange={(event) =>
                onChange({
                  kind: "metric",
                  value: {
                    ...metric,
                    current: parseNullableNumber(event.target.value),
                  },
                })
              }
              className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
            />
          </label>
          <label className="grid gap-1 text-sm font-medium text-stone-700">
            Target
            <input
              type="number"
              value={metric.target}
              onChange={(event) =>
                onChange({
                  kind: "metric",
                  value: {
                    ...metric,
                    target: parseRequiredNumber(event.target.value, metric.target),
                  },
                })
              }
              className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
            />
          </label>
        </div>
        <label className="grid gap-1 text-sm font-medium text-stone-700">
          Unit
          <input
            value={metric.unit ?? ""}
            onChange={(event) =>
              onChange({
                kind: "metric",
                value: {
                  ...metric,
                  unit: event.target.value.trim() || null,
                },
              })
            }
            className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
            placeholder="optional"
          />
        </label>
      </div>
    );
  }

  if (item.kind === "rubric") {
    const rubric = item.value;

    return (
      <div className="grid gap-3">
        <label className="grid gap-1 text-sm font-medium text-stone-700">
          Dimension
          <input
            value={rubric.dimension}
            onChange={(event) =>
              onChange({
                kind: "rubric",
                value: { ...rubric, dimension: event.target.value },
              })
            }
            className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
          />
        </label>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="grid gap-1 text-sm font-medium text-stone-700">
            Score
            <input
              type="number"
              value={rubric.score ?? ""}
              onChange={(event) =>
                onChange({
                  kind: "rubric",
                  value: {
                    ...rubric,
                    score: parseNullableNumber(event.target.value),
                  },
                })
              }
              className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
            />
          </label>
          <label className="grid gap-1 text-sm font-medium text-stone-700">
            Scale
            <input
              type="number"
              value={rubric.scale}
              onChange={(event) =>
                onChange({
                  kind: "rubric",
                  value: {
                    ...rubric,
                    scale: parseRequiredNumber(event.target.value, rubric.scale),
                  },
                })
              }
              className="h-10 rounded-md border border-stone-300 bg-white px-3 text-sm text-stone-900 outline-none transition-colors focus:border-emerald-500"
            />
          </label>
        </div>
        <label className="grid gap-1 text-sm font-medium text-stone-700">
          Description
          <textarea
            value={rubric.description ?? ""}
            onChange={(event) =>
              onChange({
                kind: "rubric",
                value: {
                  ...rubric,
                  description: event.target.value.trim() || null,
                },
              })
            }
            className="min-h-20 resize-y rounded-md border border-stone-300 bg-white p-3 text-sm leading-6 text-stone-900 outline-none transition-colors focus:border-emerald-500"
            placeholder="optional"
          />
        </label>
      </div>
    );
  }

  return (
    <label className="grid gap-1 text-sm font-medium text-stone-700">
      Item JSON
      <textarea
        value={item.value}
        onChange={(event) => onChange({ kind: "json", value: event.target.value })}
        className="min-h-32 resize-y rounded-md border border-stone-300 bg-white p-3 font-mono text-xs leading-6 text-stone-900 outline-none transition-colors focus:border-emerald-500"
      />
    </label>
  );
}

function LayerProgress({ currentLayer }: { currentLayer: FreezeLayer }) {
  const currentIndex = layerProgressIndex(currentLayer);

  return (
    <ol className="grid gap-2 sm:grid-cols-4" aria-label="Freeze layer progress">
      {LAYERS.map((layer, index) => {
        const isComplete = index < currentIndex;
        const isCurrent = index === currentIndex;

        return (
          <li
            key={layer}
            className={`rounded-md border px-3 py-2 text-sm font-semibold ${
              isCurrent
                ? "border-emerald-500 bg-emerald-50 text-emerald-800"
                : isComplete
                  ? "border-stone-300 bg-stone-100 text-stone-700"
                  : "border-stone-200 bg-white text-stone-400"
            }`}
          >
            {FREEZE_LAYER_LABEL[layer]}
          </li>
        );
      })}
    </ol>
  );
}

export function FreezeSession({ nodeId }: FreezeSessionProps) {
  const router = useRouter();
  const [nodeIntent, setNodeIntent] = useState("Loading node");
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [currentLayer, setCurrentLayer] = useState<FreezeLayer>("assertions");
  const [approvedItems, setApprovedItems] = useState<ApprovedFreezeItem[]>([]);
  const [pendingProposal, setPendingProposal] = useState<PendingProposal | null>(
    null,
  );
  const [layerCompletion, setLayerCompletion] = useState<LayerCompletion | null>(
    null,
  );
  const [messages, setMessages] = useState<SessionMessage[]>([]);
  const [userResponse, setUserResponse] = useState("");
  const [isLoading, setIsLoading] = useState(true);
  const [isSending, setIsSending] = useState(false);
  const [isCommitting, setIsCommitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mountedRef = useRef(false);

  const approvedCount = approvedItems.length;
  const canCommit = approvedCount > 0 && !isCommitting;

  const approvedByLayer = useMemo(() => {
    return LAYERS.reduce<Record<FreezeLayer, ApprovedFreezeItem[]>>(
      (result, layer) => ({
        ...result,
        [layer]: approvedItems.filter(
          (item) => normalizeFreezeLayer(item.layer) === layer,
        ),
      }),
      {
        assertions: [],
        metrics: [],
        rubric: [],
        complete: [],
      },
    );
  }, [approvedItems]);

  useEffect(() => {
    mountedRef.current = true;

    return () => {
      mountedRef.current = false;
    };
  }, []);

  const handleStreamEvent = useCallback((event: FreezeStreamEvent) => {
    if (isLayerCompleteEvent(event)) {
      const nextLayer = event.next_layer
        ? normalizeFreezeLayer(event.next_layer)
        : null;
      const completedLayer = normalizeFreezeLayer(event.layer);

      setPendingProposal(null);
      setLayerCompletion({
        layer: completedLayer,
        nextLayer,
      });
      setCurrentLayer(nextLayer ?? "complete");
      setMessages((current) => [
        ...current,
        {
          id: nextMessageId(),
          role: "agent",
          text: nextLayer
            ? `${FREEZE_LAYER_LABEL[completedLayer]} complete.`
            : "All freeze layers complete.",
        },
      ]);
      return;
    }

    const normalizedLayer = normalizeFreezeLayer(event.layer);
    setCurrentLayer(normalizedLayer);
    setLayerCompletion(null);
    setPendingProposal({
      proposal: event,
      editableItem: parseEditableFreezeItem(event.layer, event.item_json),
    });
    setMessages((current) => [
      ...current,
      {
        id: nextMessageId(),
        role: "agent",
        text: event.reasoning,
      },
    ]);
  }, []);

  const requestProposal = useCallback(async ({
    action,
    approvedItemJson,
    response,
    targetSessionId,
  }: {
    action: "start" | "approve" | "edit" | "reject" | "skip_layer";
    approvedItemJson?: string;
    response: string;
    targetSessionId: string;
  }): Promise<boolean> => {
    setIsSending(true);
    setError(null);

    try {
      const event = await respondFreezeSession(nodeId, {
        action,
        approved_item_json: approvedItemJson,
        session_id: targetSessionId,
        user_response: response,
      });

      if (!mountedRef.current) {
        return false;
      }

      handleStreamEvent(event);
      return true;
    } catch (requestError) {
      if (!mountedRef.current) {
        return false;
      }

      setError(
        requestError instanceof Error
          ? requestError.message
          : "Freeze session request failed",
      );
      return false;
    } finally {
      if (mountedRef.current) {
        setIsSending(false);
      }
    }
  }, [handleStreamEvent, nodeId]);

  useEffect(() => {
    let cancelled = false;

    async function initializeSession() {
      setIsLoading(true);
      setError(null);

      try {
        const [node, activeSession] = await Promise.all([
          getNode(nodeId),
          getActiveFreezeSession(nodeId),
        ]);

        if (cancelled) {
          return;
        }

        setNodeIntent(node.intent);
        const acceptance = parseNodeAcceptance(node.acceptance_json);

        if (!activeSession && acceptance?.type === "structured") {
          router.replace(`/nodes/${encodeURIComponent(nodeId)}?panel=${encodeURIComponent(nodeId)}`);
          return;
        }

        const session = activeSession ?? (await startFreezeSession(nodeId));
        const approved = activeSession
          ? parseApprovedFreezeItems(activeSession.approved_items_json)
          : [];
        const layer = activeSession
          ? normalizeFreezeLayer(activeSession.current_layer)
          : "assertions";

        if (cancelled) {
          return;
        }

        setSessionId(session.session_id);
        setApprovedItems(approved);
        setCurrentLayer(layer);
        setMessages([
          {
            id: nextMessageId(),
            role: "agent",
            text: activeSession
              ? "Session restored from saved approvals."
              : "Freeze session started.",
          },
        ]);

        await requestProposal({
          action: "start",
          response: "",
          targetSessionId: session.session_id,
        });
      } catch (loadError) {
        if (!cancelled) {
          setError(
            loadError instanceof Error
              ? loadError.message
              : "Freeze session failed to load",
          );
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    }

    void initializeSession();

    return () => {
      cancelled = true;
    };
  }, [nodeId, requestProposal, router]);

  const submitResponse = async (
    action: "approve" | "edit" | "reject" | "skip_layer" | "start",
    messageText: string,
    approvedItemJson?: string,
  ): Promise<boolean> => {
    if (!sessionId) {
      return false;
    }

    setMessages((current) => [
      ...current,
      {
        id: nextMessageId(),
        role: "user",
        text: messageText,
      },
    ]);
    setPendingProposal(null);
    setLayerCompletion(null);

    return requestProposal({
      action,
      approvedItemJson,
      response: messageText,
      targetSessionId: sessionId,
    });
  };

  const approveCurrent = async (action: "approve" | "edit") => {
    if (!pendingProposal) {
      return;
    }

    const approvedItemJson = serializeEditableFreezeItem(
      pendingProposal.editableItem,
    );
    const messageText =
      action === "edit"
        ? userResponse.trim() || "Edited and approved"
        : "Approved as-is";

    const sent = await submitResponse(action, messageText, approvedItemJson);

    if (sent) {
      setApprovedItems((current) => [
        ...current,
        {
          layer: normalizeFreezeLayer(pendingProposal.proposal.layer),
          item_json: approvedItemJson,
        },
      ]);
      setUserResponse("");
    }
  };

  const rejectCurrent = async () => {
    await submitResponse("reject", userResponse.trim() || "Rejected proposal");
    setUserResponse("");
  };

  const skipLayer = async () => {
    await submitResponse("skip_layer", "Skip this layer");
    setUserResponse("");
  };

  const moveToNextLayer = async () => {
    if (!sessionId) {
      return;
    }

    setLayerCompletion(null);
    await requestProposal({
      action: "start",
      response: "",
      targetSessionId: sessionId,
    });
  };

  const commitAcceptance = async () => {
    if (!sessionId || approvedItems.length === 0) {
      return;
    }

    setIsCommitting(true);
    setError(null);

    try {
      await commitFreezeSession(nodeId, sessionId);
      router.push(`/nodes/${encodeURIComponent(nodeId)}?panel=${encodeURIComponent(nodeId)}`);
    } catch (commitError) {
      setError(
        commitError instanceof Error
          ? commitError.message
          : "Commit acceptance failed",
      );
    } finally {
      setIsCommitting(false);
    }
  };

  return (
    <main className="min-h-screen bg-stone-50">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-5 py-6 sm:px-8 lg:px-10">
        <header className="rounded-lg border border-stone-200 bg-white p-5">
          <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
            <div className="max-w-3xl">
              <p className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
                Freeze Session
              </p>
              <h1 className="mt-2 text-3xl font-semibold tracking-tight text-stone-950">
                {nodeIntent}
              </h1>
            </div>
            <button
              type="button"
              onClick={() => router.push(`/nodes/${encodeURIComponent(nodeId)}?panel=${encodeURIComponent(nodeId)}`)}
              className="inline-flex h-11 items-center justify-center rounded-md border border-stone-300 px-4 text-sm font-semibold text-stone-700 transition-colors hover:border-stone-400 hover:text-stone-950"
            >
              Back to node
            </button>
          </div>

          <div className="mt-5">
            <LayerProgress currentLayer={currentLayer} />
          </div>
        </header>

        {error ? (
          <div className="rounded-lg border border-rose-200 bg-rose-50 p-4 text-sm text-rose-700">
            <div className="font-semibold">Freeze session needs attention</div>
            <p className="mt-1">{error}</p>
          </div>
        ) : null}

        <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_20rem]">
          <section className="min-h-[34rem] rounded-lg border border-stone-200 bg-white p-4">
            {isLoading ? (
              <div className="space-y-4">
                <div className="h-24 animate-pulse rounded-lg bg-stone-100" />
                <div className="ml-auto h-20 max-w-lg animate-pulse rounded-lg bg-stone-100" />
                <div className="h-56 animate-pulse rounded-lg bg-stone-100" />
              </div>
            ) : (
              <div className="space-y-4">
                {messages.map((message) => (
                  <div
                    key={message.id}
                    className={`flex ${
                      message.role === "user" ? "justify-end" : "justify-start"
                    }`}
                  >
                    <div
                      className={`max-w-[82%] rounded-lg border px-4 py-3 text-sm leading-6 ${
                        message.role === "user"
                          ? "border-emerald-200 bg-emerald-50 text-emerald-900"
                          : "border-stone-200 bg-stone-50 text-stone-700"
                      }`}
                    >
                      {message.text}
                    </div>
                  </div>
                ))}

                {pendingProposal ? (
                  <div className="flex justify-start">
                    <article className="w-full max-w-3xl rounded-lg border border-stone-200 bg-stone-50 p-4">
                      <div className="flex flex-wrap items-center justify-between gap-3">
                        <div>
                          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-stone-500">
                            {FREEZE_LAYER_LABEL[
                              normalizeFreezeLayer(pendingProposal.proposal.layer)
                            ]}
                          </p>
                          <h2 className="mt-1 text-lg font-semibold text-stone-950">
                            Proposed acceptance item
                          </h2>
                        </div>
                        <span className="rounded-md bg-white px-2 py-1 text-xs font-semibold text-stone-500 ring-1 ring-inset ring-stone-200">
                          Agent
                        </span>
                      </div>

                      <p className="mt-3 text-sm leading-6 text-stone-600">
                        {pendingProposal.proposal.reasoning}
                      </p>
                      <blockquote className="mt-3 rounded-md border-l-4 border-emerald-500 bg-white px-3 py-2 text-sm leading-6 text-stone-700">
                        {pendingProposal.proposal.source_quote}
                      </blockquote>

                      <div className="mt-4">
                        <EditableProposalFields
                          item={pendingProposal.editableItem}
                          onChange={(editableItem) =>
                            setPendingProposal((current) =>
                              current
                                ? {
                                    ...current,
                                    editableItem,
                                  }
                                : current,
                            )
                          }
                        />
                      </div>

                      <label className="mt-4 grid gap-1 text-sm font-medium text-stone-700">
                        Feedback
                        <textarea
                          value={userResponse}
                          onChange={(event) => setUserResponse(event.target.value)}
                          className="min-h-24 resize-y rounded-md border border-stone-300 bg-white p-3 text-sm leading-6 text-stone-900 outline-none transition-colors focus:border-emerald-500"
                          placeholder="make it more specific"
                        />
                      </label>

                      <div className="mt-4 flex flex-wrap gap-2">
                        <button
                          type="button"
                          onClick={() => {
                            void approveCurrent("approve");
                          }}
                          disabled={isSending}
                          className="inline-flex h-10 items-center justify-center rounded-md bg-emerald-700 px-4 text-sm font-semibold text-white transition-colors hover:bg-emerald-800 disabled:cursor-not-allowed disabled:bg-stone-300"
                        >
                          Approve as-is
                        </button>
                        <button
                          type="button"
                          onClick={() => {
                            void approveCurrent("edit");
                          }}
                          disabled={isSending}
                          className="inline-flex h-10 items-center justify-center rounded-md border border-stone-300 px-4 text-sm font-semibold text-stone-700 transition-colors hover:border-stone-400 hover:text-stone-950 disabled:cursor-not-allowed disabled:text-stone-300"
                        >
                          Edit then approve
                        </button>
                        <button
                          type="button"
                          onClick={() => {
                            void rejectCurrent();
                          }}
                          disabled={isSending}
                          className="inline-flex h-10 items-center justify-center rounded-md border border-stone-300 px-4 text-sm font-semibold text-stone-700 transition-colors hover:border-stone-400 hover:text-stone-950 disabled:cursor-not-allowed disabled:text-stone-300"
                        >
                          Send feedback
                        </button>
                        <button
                          type="button"
                          onClick={() => {
                            void skipLayer();
                          }}
                          disabled={isSending}
                          className="inline-flex h-10 items-center justify-center rounded-md px-4 text-sm font-semibold text-stone-500 transition-colors hover:bg-stone-100 hover:text-stone-950 disabled:cursor-not-allowed disabled:text-stone-300"
                        >
                          Skip this layer
                        </button>
                      </div>
                    </article>
                  </div>
                ) : null}

                {layerCompletion ? (
                  <div className="rounded-lg border border-emerald-200 bg-emerald-50 p-4 text-sm text-emerald-900">
                    <div className="font-semibold">
                      {FREEZE_LAYER_LABEL[layerCompletion.layer]} complete
                    </div>
                    <p className="mt-1 leading-6">
                      {layerCompletion.nextLayer
                        ? `${FREEZE_LAYER_LABEL[layerCompletion.nextLayer]} is ready.`
                        : "Review the approved items and commit the structured acceptance."}
                    </p>
                    {layerCompletion.nextLayer ? (
                      <button
                        type="button"
                        onClick={() => {
                          void moveToNextLayer();
                        }}
                        disabled={isSending}
                        className="mt-3 inline-flex h-10 items-center justify-center rounded-md bg-emerald-700 px-4 text-sm font-semibold text-white transition-colors hover:bg-emerald-800 disabled:cursor-not-allowed disabled:bg-stone-300"
                      >
                        Move to next layer
                      </button>
                    ) : null}
                  </div>
                ) : null}
              </div>
            )}
          </section>

          <aside className="rounded-lg border border-stone-200 bg-white p-4">
            <div className="flex items-center justify-between gap-3">
              <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
                Approved
              </h2>
              <span className="rounded-md bg-stone-100 px-2 py-1 text-sm font-semibold text-stone-700">
                {approvedCount}
              </span>
            </div>

            <div className="mt-4 space-y-4">
              {(["assertions", "metrics", "rubric"] as FreezeLayer[]).map(
                (layer) => (
                  <section key={layer}>
                    <h3 className="text-sm font-semibold text-stone-950">
                      {FREEZE_LAYER_LABEL[layer]}
                    </h3>
                    <div className="mt-2 space-y-2">
                      {approvedByLayer[layer].length === 0 ? (
                        <p className="text-sm text-stone-500">None yet</p>
                      ) : null}
                      {approvedByLayer[layer].map((item, index) => (
                        <div
                          key={`${layer}-${index}`}
                          className="rounded-md border border-stone-200 bg-stone-50 p-3 text-sm leading-6 text-stone-700"
                        >
                          {formatApprovedSummary(item)}
                        </div>
                      ))}
                    </div>
                  </section>
                ),
              )}
            </div>

            <button
              type="button"
              onClick={() => {
                void commitAcceptance();
              }}
              disabled={!canCommit}
              className="mt-5 inline-flex h-11 w-full items-center justify-center rounded-md bg-stone-950 px-4 text-sm font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:bg-stone-300"
            >
              {isCommitting ? "Committing" : "Commit acceptance"}
            </button>
          </aside>
        </div>
      </div>
    </main>
  );
}
