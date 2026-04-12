"use client";

import type { Node } from "@/bindings/Node";
import { useRealtimeSubscription } from "@/features/realtime/provider";
import {
  getNode,
  getNodeAncestors,
  getNodeChildren,
  getNodeState,
} from "@/lib/api";
import { useCallback, useEffect, useRef, useState } from "react";

import type { ObjectiveTreeItem } from "../lib/objective-tree-data";

const OBJECTIVE_TREE_MAX_DEPTH = 3;

type ObjectiveTreeState = {
  ancestors: Node[];
  root: ObjectiveTreeItem | null;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
};

async function loadSubtree(
  node: Node,
  depthRemaining = OBJECTIVE_TREE_MAX_DEPTH,
): Promise<ObjectiveTreeItem> {
  const [state, children] = await Promise.all([
    getNodeState(node.id, depthRemaining),
    depthRemaining > 0 ? getNodeChildren(node.id) : Promise.resolve([]),
  ]);

  const childItems = await Promise.all(
    children.map((child) => loadSubtree(child, depthRemaining - 1)),
  );

  return {
    node,
    state,
    children: childItems,
  };
}

async function loadObjectiveTree(rootId: string) {
  const [rootNode, ancestors] = await Promise.all([
    getNode(rootId),
    getNodeAncestors(rootId),
  ]);
  const root = await loadSubtree(rootNode);

  return {
    ancestors,
    root,
  };
}

export function useObjectiveTree(rootId: string): ObjectiveTreeState {
  const [ancestors, setAncestors] = useState<Node[]>([]);
  const [root, setRoot] = useState<ObjectiveTreeItem | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const requestSequenceRef = useRef(0);

  const refresh = useCallback(async () => {
    const requestSequence = requestSequenceRef.current + 1;
    requestSequenceRef.current = requestSequence;

    setIsLoading(true);
    setError(null);

    try {
      const nextTree = await loadObjectiveTree(rootId);

      if (requestSequenceRef.current !== requestSequence) {
        return;
      }

      setAncestors(nextTree.ancestors);
      setRoot(nextTree.root);
    } catch (refreshError) {
      if (requestSequenceRef.current !== requestSequence) {
        return;
      }

      setError(
        refreshError instanceof Error
          ? refreshError.message
          : "Failed to load objective tree",
      );
    } finally {
      if (requestSequenceRef.current === requestSequence) {
        setIsLoading(false);
      }
    }
  }, [rootId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useRealtimeSubscription((message) => {
    if (message.type === "node:updated" || message.type === "run:updated") {
      void refresh();
    }
  });

  return {
    ancestors,
    root,
    isLoading,
    error,
    refresh,
  };
}
