"use client";

import type { Acceptance } from "@/bindings/Acceptance";
import type { Node } from "@/bindings/Node";
import type { NodeState } from "@/bindings/NodeState";
import type { Run } from "@/bindings/Run";
import { useRealtimeSubscription } from "@/features/realtime/provider";
import { getNode, getNodeState, getRuns } from "@/lib/api";
import { useCallback, useEffect, useRef, useState } from "react";

import {
  parseNodeAcceptance,
  sortRunsNewestFirst,
} from "../lib/node-panel-data";

type NodePanelData = {
  node: Node | null;
  state: NodeState | null;
  acceptance: Acceptance | null;
  runs: Run[];
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
};

async function loadNodePanelData(nodeId: string) {
  const [node, state, runs] = await Promise.all([
    getNode(nodeId),
    getNodeState(nodeId, 1),
    getRuns(nodeId),
  ]);

  return {
    node,
    state,
    acceptance: parseNodeAcceptance(node.acceptance_json),
    runs: sortRunsNewestFirst(runs),
  };
}

export function useNodePanelData(nodeId: string | null): NodePanelData {
  const [node, setNode] = useState<Node | null>(null);
  const [state, setState] = useState<NodeState | null>(null);
  const [acceptance, setAcceptance] = useState<Acceptance | null>(null);
  const [runs, setRuns] = useState<Run[]>([]);
  const [isLoading, setIsLoading] = useState(Boolean(nodeId));
  const [error, setError] = useState<string | null>(null);
  const requestSequenceRef = useRef(0);

  const refresh = useCallback(async () => {
    const requestSequence = requestSequenceRef.current + 1;
    requestSequenceRef.current = requestSequence;

    if (!nodeId) {
      setNode(null);
      setState(null);
      setAcceptance(null);
      setRuns([]);
      setIsLoading(false);
      setError(null);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const nextData = await loadNodePanelData(nodeId);

      if (requestSequenceRef.current !== requestSequence) {
        return;
      }

      setNode(nextData.node);
      setState(nextData.state);
      setAcceptance(nextData.acceptance);
      setRuns(nextData.runs);
    } catch (refreshError) {
      if (requestSequenceRef.current !== requestSequence) {
        return;
      }

      setError(
        refreshError instanceof Error
          ? refreshError.message
          : "Failed to load node panel",
      );
    } finally {
      if (requestSequenceRef.current === requestSequence) {
        setIsLoading(false);
      }
    }
  }, [nodeId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useRealtimeSubscription((message) => {
    if (!nodeId) {
      return;
    }

    if (
      (message.type === "node:updated" && message.id === nodeId) ||
      (message.type === "run:updated" &&
        runs.some((run) => run.id === message.id))
    ) {
      void refresh();
    }
  });

  return {
    node,
    state,
    acceptance,
    runs,
    isLoading,
    error,
    refresh,
  };
}
