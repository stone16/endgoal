"use client";

import Link from "next/link";
import { useCallback, useMemo, useState } from "react";

import type { Node } from "@/bindings/Node";
import { ObjectiveTreeRow } from "@/features/nodes/components/objective-tree-row";
import { useObjectiveTree } from "@/features/nodes/hooks/use-objective-tree";
import { buildObjectiveBreadcrumbTrail } from "@/features/nodes/lib/objective-tree-data";
import { NodePanel } from "@/features/panel/node-panel";

function LoadingTree() {
  return (
    <div className="space-y-3">
      {Array.from({ length: 5 }).map((_, index) => (
        <div
          key={index}
          className="h-24 animate-pulse rounded-lg border border-stone-200 bg-stone-100"
          style={{ marginLeft: `${Math.min(index, 3) * 22}px` }}
        />
      ))}
    </div>
  );
}

function BreadcrumbTrail({
  ancestors,
  current,
}: {
  ancestors: Node[];
  current: Node | null;
}) {
  const trail = buildObjectiveBreadcrumbTrail(ancestors, current);

  return (
    <nav
      aria-label="Objective breadcrumb"
      className="flex flex-wrap items-center gap-2 text-sm text-stone-500"
    >
      <Link
        href="/"
        className="font-medium text-indigo-700 transition-colors hover:text-indigo-900"
      >
        ← Workspace overview
      </Link>
      {trail.map((node) => (
        <span key={node.id} className="inline-flex items-center gap-2">
          <span aria-hidden="true" className="text-stone-300">
            /
          </span>
          <span className="max-w-52 truncate text-stone-700">
            {node.intent}
          </span>
        </span>
      ))}
    </nav>
  );
}

export function ObjectiveTree({
  initialSelectedNodeId = null,
  rootId,
}: {
  initialSelectedNodeId?: string | null;
  rootId: string;
}) {
  const { ancestors, root, isLoading, error, refresh } = useObjectiveTree(rootId);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(
    initialSelectedNodeId,
  );
  const closePanel = useCallback(() => setSelectedNodeId(null), []);
  const selectedNode = useMemo(
    () => (selectedNodeId ? selectedNodeId : null),
    [selectedNodeId],
  );

  return (
    <>
      <main className="mx-auto flex w-full max-w-6xl flex-1 flex-col px-5 py-8 sm:px-8 lg:px-10">
        <section className="border-b border-stone-200 pb-7">
          <BreadcrumbTrail ancestors={ancestors} current={root?.node ?? null} />

          <div className="mt-5 flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
            <div className="max-w-3xl">
              <p className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
                Objective Tree
              </p>
              <h1 className="mt-2 text-3xl font-semibold tracking-tight text-stone-950">
                {root?.node.intent ?? "Loading objective"}
              </h1>
              {root?.state.next_step ? (
                <p className="mt-3 text-base leading-7 text-stone-600">
                  {root.state.next_step}
                </p>
              ) : null}
            </div>

            <button
              type="button"
              onClick={() => {
                void refresh();
              }}
              className="inline-flex h-11 items-center justify-center rounded-md border border-stone-300 px-4 text-sm font-semibold text-stone-700 transition-colors hover:border-stone-400 hover:text-stone-950"
            >
              Refresh
            </button>
          </div>
        </section>

        <section className="flex-1 py-8">
          {error ? (
            <div className="rounded-lg border border-rose-200 bg-rose-50 p-4 text-sm text-rose-700">
              <div className="font-semibold">Tree failed to load</div>
              <p className="mt-1">{error}</p>
            </div>
          ) : null}

          {isLoading && !root ? <LoadingTree /> : null}

          {!isLoading && !root && !error ? (
            <div className="rounded-lg border border-dashed border-stone-300 bg-stone-50 p-10 text-center">
              <h2 className="text-lg font-semibold text-stone-900">
                Objective not found
              </h2>
              <p className="mt-2 text-sm leading-6 text-stone-600">
                Return to the workspace overview and open an active root
                objective.
              </p>
            </div>
          ) : null}

          {root ? (
            <div className="space-y-3" role="tree" aria-label="Objective tree">
              <ObjectiveTreeRow
                item={root}
                depth={0}
                selectedNodeId={selectedNode}
                onSelect={setSelectedNodeId}
              />
            </div>
          ) : null}
        </section>
      </main>

      <NodePanel nodeId={selectedNode} onClose={closePanel} />
    </>
  );
}
