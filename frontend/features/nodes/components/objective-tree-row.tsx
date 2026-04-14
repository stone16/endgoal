import type { ObjectiveTreeItem } from "@/features/nodes/lib/objective-tree-data";

import { PhaseBadge, phaseAccentClass } from "./phase-badge";
import { ProgressBar } from "./progress-bar";

type ObjectiveTreeRowProps = {
  item: ObjectiveTreeItem;
  depth: number;
  selectedNodeId: string | null;
  onSelect: (nodeId: string) => void;
};

function FlagList({ item }: { item: ObjectiveTreeItem }) {
  const blockerCount = item.state.rollup_blockers.length;
  const flags = [
    blockerCount > 0
      ? `${blockerCount} blocker${blockerCount === 1 ? "" : "s"}`
      : null,
    item.node.phase === "in_review" ? "Review" : null,
  ].filter(Boolean);

  if (flags.length === 0) {
    return <span className="text-stone-400">No active flags</span>;
  }

  return (
    <div className="flex flex-wrap gap-2">
      {flags.map((flag) => (
        <span
          key={flag}
          className="inline-flex rounded-md bg-amber-50 px-2 py-1 text-xs font-semibold text-amber-700 ring-1 ring-inset ring-amber-200"
        >
          {flag}
        </span>
      ))}
    </div>
  );
}

export function ObjectiveTreeRow({
  item,
  depth,
  selectedNodeId,
  onSelect,
}: ObjectiveTreeRowProps) {
  const isSelected = selectedNodeId === item.node.id;

  return (
    <div className="relative" style={{ marginLeft: `${depth * 22}px` }}>
      {depth > 0 ? (
        <span
          aria-hidden="true"
          className="absolute -left-3 bottom-0 top-0 w-px bg-stone-200"
        />
      ) : null}

      <button
        type="button"
        data-node-panel-trigger="true"
        aria-current={isSelected ? "true" : undefined}
        onClick={() => onSelect(item.node.id)}
        className={`node-card flex w-full flex-col rounded-lg border border-l-[3px] bg-white p-4 text-left shadow-[0_1px_2px_rgba(28,25,23,0.06)] transition-colors hover:border-stone-300 hover:bg-stone-50 ${phaseAccentClass(item.node.phase)} ${
          isSelected ? "ring-2 ring-indigo-200" : ""
        }`}
      >
        <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-3">
              <PhaseBadge phase={item.node.phase} />
              <span className="text-xs font-medium uppercase tracking-[0.08em] text-stone-500">
                Level {depth}
              </span>
            </div>
            <h2 className="mt-3 text-base font-semibold leading-6 text-stone-950">
              {item.node.intent}
            </h2>
          </div>

          <div className="w-full shrink-0 lg:w-64">
            <div className="mb-2 flex items-center justify-between text-sm text-stone-600">
              <span>Progress</span>
              <span className="font-medium text-stone-900">
                {Math.round(item.state.progress)}%
              </span>
            </div>
            <ProgressBar value={item.state.progress} />
          </div>
        </div>

        <div className="mt-4 flex flex-col gap-3 border-t border-stone-100 pt-4 text-sm text-stone-500 sm:flex-row sm:items-center sm:justify-between">
          <FlagList item={item} />
          <span className="line-clamp-2 text-stone-600">
            {item.state.next_step}
          </span>
        </div>
      </button>

      {item.children.length > 0 ? (
        <div className="mt-3 space-y-3">
          {item.children.map((child) => (
            <ObjectiveTreeRow
              key={child.node.id}
              item={child}
              depth={depth + 1}
              selectedNodeId={selectedNodeId}
              onSelect={onSelect}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}
