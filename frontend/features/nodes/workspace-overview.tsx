"use client";

import { WorkspaceOverviewCard } from "@/features/nodes/components/workspace-overview-card";
import { useWorkspaceOverview } from "@/features/nodes/hooks/use-workspace-overview";

function LoadingState() {
  return (
    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
      {Array.from({ length: 3 }).map((_, index) => (
        <div
          key={index}
          className="h-72 animate-pulse rounded-lg border border-stone-200 bg-stone-100"
        />
      ))}
    </div>
  );
}

export function WorkspaceOverview() {
  const { items, isLoading, error, refresh } = useWorkspaceOverview();

  return (
    <main className="mx-auto flex w-full max-w-6xl flex-1 flex-col px-6 py-10 sm:px-8 lg:px-10">
      <section className="border-b border-stone-200 pb-8">
        <p className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
          Workspace Overview
        </p>
        <div className="mt-3 flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
          <div className="max-w-3xl">
            <h1 className="text-3xl font-semibold tracking-tight text-stone-950">
              Goals that need attention now
            </h1>
            <p className="mt-3 text-base leading-7 text-stone-600">
              Top-level objectives only. Live node updates invalidate this view
              automatically, and each card keeps the state layer&apos;s next step
              visible without drilling into the tree.
            </p>
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
            <div className="font-semibold">Workspace overview failed to load</div>
            <p className="mt-1">{error}</p>
          </div>
        ) : null}

        {isLoading && items.length === 0 ? <LoadingState /> : null}

        {!isLoading && items.length === 0 ? (
          <div className="rounded-lg border border-dashed border-stone-300 bg-stone-50 p-10 text-center">
            <h2 className="text-lg font-semibold text-stone-900">
              No top-level nodes yet
            </h2>
            <p className="mt-2 text-sm leading-6 text-stone-600">
              Create a root objective through the backend API and it will appear
              here automatically when the node update event lands.
            </p>
          </div>
        ) : null}

        {items.length > 0 ? (
          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
            {items.map((item) => (
              <WorkspaceOverviewCard key={item.node.id} item={item} />
            ))}
          </div>
        ) : null}
      </section>
    </main>
  );
}
