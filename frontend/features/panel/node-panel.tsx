"use client";

import { useEffect, useRef, useState } from "react";

import { PhaseBadge } from "@/features/nodes/components/phase-badge";
import { ProgressBar } from "@/features/nodes/components/progress-bar";
import { AcceptanceSection } from "@/features/panel/components/acceptance-section";
import { ArchetypeBGateModal } from "@/features/panel/components/archetype-b-gate-modal";
import { PanelActions } from "@/features/panel/components/panel-actions";
import { PanelToast } from "@/features/panel/components/panel-toast";
import { RunsList } from "@/features/panel/components/runs-list";
import { useNodePanelData } from "@/features/panel/hooks/use-node-panel-data";
import { useRunTrigger } from "@/features/panel/hooks/use-run-trigger";
import { RunDetailOverlay } from "@/features/runs/run-detail-overlay";

type NodePanelProps = {
  nodeId: string | null;
  onClose: () => void;
};

type SelectedRun = {
  nodeId: string;
  runId: string;
};

export function NodePanel({ nodeId, onClose }: NodePanelProps) {
  const panelRef = useRef<HTMLElement | null>(null);
  const [selectedRun, setSelectedRun] = useState<SelectedRun | null>(null);
  const { node, state, acceptance, runs, isLoading, error, refresh } =
    useNodePanelData(nodeId);
  const runTrigger = useRunTrigger({
    acceptance,
    node,
    refresh,
  });

  const selectedRunId =
    selectedRun && selectedRun.nodeId === nodeId ? selectedRun.runId : null;

  useEffect(() => {
    if (!nodeId) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        if (selectedRunId) {
          return;
        }

        onClose();
      }
    }

    function handlePointerDown(event: PointerEvent) {
      const target = event.target;

      if (!(target instanceof Element)) {
        return;
      }

      if (panelRef.current?.contains(target)) {
        return;
      }

      if (target.closest('[data-node-panel-trigger="true"]')) {
        return;
      }

      if (target.closest('[data-node-panel-overlay="true"]')) {
        return;
      }

      onClose();
    }

    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("pointerdown", handlePointerDown);

    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("pointerdown", handlePointerDown);
    };
  }, [nodeId, onClose, selectedRunId]);

  const selectedRunModel =
    selectedRunId ? runs.find((run) => run.id === selectedRunId) ?? null : null;

  if (!nodeId) {
    return null;
  }

  return (
    <>
      <aside
        ref={panelRef}
        role="dialog"
        aria-label="Node panel"
        className="fixed bottom-0 right-0 top-0 z-40 flex w-full max-w-xl flex-col border-l border-stone-200 bg-white shadow-[-12px_0_32px_rgba(28,25,23,0.12)] sm:w-[30rem]"
      >
        <div className="flex items-center justify-between gap-4 border-b border-stone-200 px-5 py-4">
          <div className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
            Node
          </div>
          <button
            type="button"
            onClick={onClose}
            className="inline-flex h-9 w-9 items-center justify-center rounded-md border border-stone-300 text-lg leading-none text-stone-500 transition-colors hover:border-stone-400 hover:text-stone-950"
            aria-label="Close node panel"
          >
            ×
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-6">
          {error ? (
            <div className="rounded-lg border border-rose-200 bg-rose-50 p-4 text-sm text-rose-700">
              <div className="font-semibold">Node failed to load</div>
              <p className="mt-1">{error}</p>
            </div>
          ) : null}

          {isLoading && !node ? (
            <div className="space-y-4">
              <div className="h-8 w-28 animate-pulse rounded-md bg-stone-100" />
              <div className="h-16 animate-pulse rounded-lg bg-stone-100" />
              <div className="h-40 animate-pulse rounded-lg bg-stone-100" />
            </div>
          ) : null}

          {node && state ? (
            <div className="space-y-6">
              <section>
                <PhaseBadge phase={node.phase} />
                <h1 className="mt-3 text-2xl font-semibold leading-8 text-stone-950">
                  {node.intent}
                </h1>
              </section>

              <section className="rounded-lg border border-stone-200 bg-stone-50 p-4">
                <div className="mb-2 flex items-center justify-between text-sm text-stone-600">
                  <span>Progress</span>
                  <span className="font-medium text-stone-900">
                    {Math.round(state.progress)}%
                  </span>
                </div>
                <ProgressBar value={state.progress} />
                <div className="mt-4 text-sm">
                  <div className="font-semibold text-stone-900">Next Step</div>
                  <p className="mt-2 leading-6 text-stone-600">
                    {state.next_step}
                  </p>
                </div>
              </section>

              <AcceptanceSection acceptance={acceptance} />
              <RunsList
                runs={runs}
                onSelectRun={(runId) => {
                  setSelectedRun({ nodeId: node.id, runId });
                }}
              />
              <PanelActions
                isTriggerRunBusy={runTrigger.isDispatching}
                onTriggerRun={() => {
                  void runTrigger.triggerRun();
                }}
                triggerRunDisabled={!node}
              />
            </div>
          ) : null}
        </div>
      </aside>

      <ArchetypeBGateModal
        isDispatching={runTrigger.isDispatching}
        isOpen={runTrigger.isGateOpen}
        nodeIntent={node?.intent ?? ""}
        onCancel={runTrigger.cancelGate}
        onFreezeNow={runTrigger.freezeNow}
        onProceedAsExploration={() => {
          void runTrigger.proceedAsExploration();
        }}
      />
      <PanelToast
        onDismiss={runTrigger.dismissToast}
        toast={runTrigger.toast}
      />
      <RunDetailOverlay
        initialRun={selectedRunModel}
        runId={selectedRunId}
        onClose={() => setSelectedRun(null)}
      />
    </>
  );
}
