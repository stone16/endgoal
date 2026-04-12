"use client";

import type { Acceptance } from "@/bindings/Acceptance";
import type { Node } from "@/bindings/Node";
import { dispatchRun } from "@/lib/api";
import { useCallback, useState } from "react";

import {
  buildRunDispatchRequest,
  getTriggerRunGate,
} from "../lib/node-panel-data";

type ToastState = {
  message: string;
  tone: "success" | "error";
} | null;

type UseRunTriggerInput = {
  acceptance: Acceptance | null;
  node: Node | null;
  refresh: () => Promise<void>;
};

type UseRunTriggerState = {
  cancelGate: () => void;
  closeFreezePlaceholder: () => void;
  dismissToast: () => void;
  freezeNow: () => void;
  isDispatching: boolean;
  isFreezePlaceholderOpen: boolean;
  isGateOpen: boolean;
  proceedAsExploration: () => Promise<void>;
  toast: ToastState;
  triggerRun: () => Promise<void>;
};

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return "Run dispatch failed";
}

export function useRunTrigger({
  acceptance,
  node,
  refresh,
}: UseRunTriggerInput): UseRunTriggerState {
  const [isGateOpen, setIsGateOpen] = useState(false);
  const [isFreezePlaceholderOpen, setIsFreezePlaceholderOpen] = useState(false);
  const [isDispatching, setIsDispatching] = useState(false);
  const [toast, setToast] = useState<ToastState>(null);

  const runDispatch = useCallback(
    async (
      runType: "research_iteration" | "exploration",
      successMessage: string,
    ) => {
      if (!node) {
        return;
      }

      setIsDispatching(true);
      setToast(null);

      try {
        await dispatchRun(node.id, buildRunDispatchRequest(runType));
        setToast({
          message: successMessage,
          tone: "success",
        });
        await refresh();
      } catch (dispatchError) {
        setToast({
          message: errorMessage(dispatchError),
          tone: "error",
        });
      } finally {
        setIsDispatching(false);
      }
    },
    [node, refresh],
  );

  const triggerRun = useCallback(async () => {
    if (!node) {
      return;
    }

    const gate = getTriggerRunGate(node.phase, acceptance);

    if (gate === "direct") {
      await runDispatch("research_iteration", "Run dispatched");
      return;
    }

    if (gate === "archetype_b") {
      setIsGateOpen(true);
      return;
    }

    setToast({
      message: "Node must be active before dispatch",
      tone: "error",
    });
  }, [acceptance, node, runDispatch]);

  const cancelGate = useCallback(() => {
    setIsGateOpen(false);
  }, []);

  const proceedAsExploration = useCallback(async () => {
    setIsGateOpen(false);
    await runDispatch("exploration", "Run dispatched as exploration");
  }, [runDispatch]);

  const freezeNow = useCallback(() => {
    setIsGateOpen(false);
    setIsFreezePlaceholderOpen(true);
  }, []);

  const closeFreezePlaceholder = useCallback(() => {
    setIsFreezePlaceholderOpen(false);
  }, []);

  const dismissToast = useCallback(() => {
    setToast(null);
  }, []);

  return {
    cancelGate,
    closeFreezePlaceholder,
    dismissToast,
    freezeNow,
    isDispatching,
    isFreezePlaceholderOpen,
    isGateOpen,
    proceedAsExploration,
    toast,
    triggerRun,
  };
}
