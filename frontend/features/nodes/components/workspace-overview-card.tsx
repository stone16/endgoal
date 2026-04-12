import Link from "next/link";

import type { Phase } from "@/bindings/Phase";
import type { WorkspaceOverviewItem } from "@/features/nodes/hooks/use-workspace-overview";

import { PhaseBadge, phaseAccentClass } from "./phase-badge";
import { ProgressBar } from "./progress-bar";

function formatRelativeTimestamp(isoTimestamp: string): string {
  const timestamp = new Date(isoTimestamp).getTime();
  const now = Date.now();

  if (Number.isNaN(timestamp)) {
    return "time unknown";
  }

  const diffMs = timestamp - now;
  const diffMinutes = Math.round(diffMs / (1000 * 60));
  const formatter = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

  if (Math.abs(diffMinutes) < 60) {
    return formatter.format(diffMinutes, "minute");
  }

  const diffHours = Math.round(diffMinutes / 60);

  if (Math.abs(diffHours) < 24) {
    return formatter.format(diffHours, "hour");
  }

  const diffDays = Math.round(diffHours / 24);
  return formatter.format(diffDays, "day");
}

function confidenceTone(phase: Phase): "indigo" | "emerald" | "stone" {
  if (phase === "complete") {
    return "emerald";
  }

  if (phase === "archived" || phase === "draft") {
    return "stone";
  }

  return "indigo";
}

export function WorkspaceOverviewCard({
  item,
}: {
  item: WorkspaceOverviewItem;
}) {
  const blockerCount = item.state.rollup_blockers.length;
  const confidencePercent = Math.round(item.state.confidence * 100);

  return (
    <article
      className={`flex h-full flex-col rounded-lg border border-stone-200 bg-white p-5 shadow-[0_1px_2px_rgba(28,25,23,0.06)] ${phaseAccentClass(item.node.phase)} border-l-4`}
    >
      <div className="flex items-start justify-between gap-4">
        <PhaseBadge phase={item.node.phase} />
        <p className="text-sm text-stone-500">
          Updated {formatRelativeTimestamp(item.node.updated_at)}
        </p>
      </div>

      <div className="mt-4 flex-1">
        <h2 className="text-lg font-semibold text-stone-950">
          {item.node.intent}
        </h2>

        <div className="mt-5 space-y-3">
          <div>
            <div className="mb-2 flex items-center justify-between text-sm text-stone-600">
              <span>Progress</span>
              <span className="font-medium text-stone-900">
                {Math.round(item.state.progress)}%
              </span>
            </div>
            <ProgressBar value={item.state.progress} />
          </div>

          <div>
            <div className="mb-2 flex items-center justify-between text-sm text-stone-600">
              <span>Confidence</span>
              <span className="font-medium text-stone-900">
                {confidencePercent}%
              </span>
            </div>
            <ProgressBar
              value={confidencePercent}
              tone={confidenceTone(item.node.phase)}
            />
          </div>
        </div>

        <div className="mt-5 rounded-md bg-stone-50 p-3">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-stone-500">
            Next Attention
          </p>
          <p
            className="mt-2 text-sm leading-6 text-stone-700"
            style={{
              display: "-webkit-box",
              WebkitLineClamp: 2,
              WebkitBoxOrient: "vertical",
              overflow: "hidden",
            }}
          >
            {item.state.next_step}
          </p>
        </div>
      </div>

      <footer className="mt-5 flex items-center justify-between gap-4 border-t border-stone-100 pt-4">
        <div className="flex items-center gap-2 text-sm text-stone-500">
          {blockerCount > 0 ? (
            <span className="inline-flex items-center gap-1 rounded-md bg-rose-50 px-2 py-1 font-medium text-rose-700">
              <span aria-hidden="true">!</span>
              {blockerCount} blocker{blockerCount === 1 ? "" : "s"}
            </span>
          ) : null}
          {item.node.phase === "in_review" ? (
            <span className="inline-flex items-center gap-1 rounded-md bg-amber-50 px-2 py-1 font-medium text-amber-700">
              <span aria-hidden="true">?</span>
              Review
            </span>
          ) : null}
          {blockerCount === 0 && item.node.phase !== "in_review" ? (
            <span className="text-stone-400">No active flags</span>
          ) : null}
        </div>

        <Link
          href={`/nodes/${item.node.id}`}
          className="text-sm font-semibold text-indigo-700 transition-colors hover:text-indigo-900"
        >
          Open tree →
        </Link>
      </footer>
    </article>
  );
}
