import type { Run } from "@/bindings/Run";
import { getRunFindingsSnippet } from "@/features/panel/lib/node-panel-data";

type RunsListProps = {
  runs: Run[];
};

function formatTimestamp(isoTimestamp: string): string {
  const timestamp = new Date(isoTimestamp);

  if (Number.isNaN(timestamp.getTime())) {
    return "time unknown";
  }

  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(timestamp);
}

function runStatusClass(status: string): string {
  switch (status) {
    case "completed":
      return "bg-emerald-50 text-emerald-700 ring-emerald-200";
    case "failed":
      return "bg-rose-50 text-rose-700 ring-rose-200";
    case "running":
      return "bg-indigo-50 text-indigo-700 ring-indigo-200";
    default:
      return "bg-stone-100 text-stone-600 ring-stone-200";
  }
}

export function RunsList({ runs }: RunsListProps) {
  return (
    <section className="rounded-lg border border-stone-200 p-4">
      <div className="flex items-center justify-between gap-4">
        <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
          Runs
        </h2>
        <span className="text-sm text-stone-500">{runs.length}</span>
      </div>

      <div className="mt-3 space-y-3">
        {runs.length === 0 ? (
          <p className="text-sm leading-6 text-stone-600">No runs yet</p>
        ) : null}

        {runs.map((run) => (
          <button
            key={run.id}
            type="button"
            className="w-full rounded-md border border-stone-200 bg-stone-50 p-3 text-left transition-colors hover:border-stone-300 hover:bg-white"
          >
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="text-sm font-semibold text-stone-900">
                {run.type}
              </div>
              <span
                className={`inline-flex rounded-md px-2 py-1 text-xs font-semibold ring-1 ring-inset ${runStatusClass(
                  run.status,
                )}`}
              >
                {run.status}
              </span>
            </div>
            <p className="mt-1 text-xs text-stone-500">
              {formatTimestamp(run.created_at)}
            </p>
            <p className="mt-3 line-clamp-2 text-sm leading-6 text-stone-600">
              {getRunFindingsSnippet(run)}
            </p>
          </button>
        ))}
      </div>
    </section>
  );
}
