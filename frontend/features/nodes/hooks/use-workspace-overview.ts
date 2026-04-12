"use client";

import type { Node } from "@/bindings/Node";
import type { NodeState } from "@/bindings/NodeState";
import { useRealtimeSubscription } from "@/features/realtime/provider";
import { getNodeState, getNodes } from "@/lib/api";
import { useCallback, useEffect, useRef, useState } from "react";

export type WorkspaceOverviewItem = {
  node: Node;
  state: NodeState;
};

type WorkspaceOverviewState = {
  items: WorkspaceOverviewItem[];
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
};

async function loadWorkspaceOverview(): Promise<WorkspaceOverviewItem[]> {
  const nodes = await getNodes();

  const states = await Promise.all(
    nodes.map(async (node) => ({
      node,
      state: await getNodeState(node.id),
    })),
  );

  return states.sort(
    (left, right) =>
      new Date(right.node.updated_at).getTime() -
      new Date(left.node.updated_at).getTime(),
  );
}

export function useWorkspaceOverview(): WorkspaceOverviewState {
  const [items, setItems] = useState<WorkspaceOverviewItem[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const requestSequenceRef = useRef(0);

  const refresh = useCallback(async () => {
    const requestSequence = requestSequenceRef.current + 1;
    requestSequenceRef.current = requestSequence;

    setIsLoading(true);
    setError(null);

    try {
      const nextItems = await loadWorkspaceOverview();

      if (requestSequenceRef.current === requestSequence) {
        setItems(nextItems);
      }
    } catch (refreshError) {
      if (requestSequenceRef.current !== requestSequence) {
        return;
      }

      if (refreshError instanceof Error) {
        setError(refreshError.message);
      } else {
        setError("Failed to load workspace overview");
      }
    } finally {
      if (requestSequenceRef.current === requestSequence) {
        setIsLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useRealtimeSubscription((message) => {
    if (message.type === "node:updated") {
      void refresh();
    }
  });

  return {
    items,
    isLoading,
    error,
    refresh,
  };
}
