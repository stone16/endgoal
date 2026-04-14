import type { Acceptance } from "@/bindings/Acceptance";
import type { AssertionStatus } from "@/bindings/AssertionStatus";
import { ProgressBar } from "@/features/nodes/components/progress-bar";
import {
  metricProgress,
  rubricProgress,
} from "@/features/panel/lib/node-panel-data";

type AcceptanceSectionProps = {
  acceptance: Acceptance | null;
};

const ASSERTION_BADGE_CLASS: Record<AssertionStatus, string> = {
  pass: "bg-emerald-50 text-emerald-700 ring-emerald-200",
  fail: "bg-rose-50 text-rose-700 ring-rose-200",
  pending: "bg-stone-100 text-stone-600 ring-stone-200",
};

function formatNumber(value: number | null): string {
  if (value === null) {
    return "unset";
  }

  return Number.isInteger(value) ? value.toString() : value.toFixed(2);
}

function formatMetricValue(value: number | null, unit: string | null): string {
  const formatted = formatNumber(value);
  return unit && value !== null ? `${formatted}${unit}` : formatted;
}

export function AcceptanceSection({ acceptance }: AcceptanceSectionProps) {
  if (!acceptance) {
    return (
      <section className="rounded-lg border border-stone-200 p-4">
        <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
          Acceptance
        </h2>
        <p className="mt-3 text-sm leading-6 text-stone-600">Not set</p>
      </section>
    );
  }

  if (acceptance.type === "prose") {
    return (
      <section className="rounded-lg border border-stone-200 p-4">
        <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
          Acceptance
        </h2>
        <textarea
          readOnly
          value={acceptance.text}
          className="mt-3 min-h-32 w-full resize-none rounded-md border border-stone-200 bg-stone-50 p-3 text-sm leading-6 text-stone-700"
        />
      </section>
    );
  }

  return (
    <section className="space-y-5 rounded-lg border border-stone-200 p-4">
      <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
        Acceptance
      </h2>

      <div>
        <h3 className="text-sm font-semibold text-stone-950">Assertions</h3>
        <div className="mt-3 space-y-3">
          {acceptance.assertions.length === 0 ? (
            <p className="text-sm text-stone-500">No assertions</p>
          ) : null}
          {acceptance.assertions.map((assertion) => (
            <div
              key={assertion.id}
              className="rounded-md border border-stone-200 bg-stone-50 p-3"
            >
              <div className="flex items-start gap-3">
                <span
                  className={`inline-flex rounded-md px-2 py-1 text-xs font-semibold ring-1 ring-inset ${
                    ASSERTION_BADGE_CLASS[assertion.status]
                  }`}
                >
                  {assertion.status}
                </span>
                <div className="min-w-0">
                  <p className="text-sm leading-6 text-stone-800">
                    {assertion.text}
                  </p>
                  {assertion.check_fn ? (
                    <p className="mt-1 text-xs text-stone-500">
                      Check: {assertion.check_fn}
                    </p>
                  ) : null}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div>
        <h3 className="text-sm font-semibold text-stone-950">Metrics</h3>
        <div className="mt-3 space-y-3">
          {acceptance.metrics.length === 0 ? (
            <p className="text-sm text-stone-500">No metrics</p>
          ) : null}
          {acceptance.metrics.map((metric) => {
            const progress = metricProgress(metric);

            return (
              <div
                key={metric.id}
                className="rounded-md border border-stone-200 bg-stone-50 p-3"
              >
                <div className="flex items-start justify-between gap-3 text-sm">
                  <div>
                    <div className="font-semibold text-stone-900">
                      {metric.name}
                    </div>
                    <div className="mt-1 text-stone-500">
                      Baseline {formatMetricValue(metric.baseline, metric.unit)}
                    </div>
                  </div>
                  <div className="text-right font-medium text-stone-900">
                    {formatMetricValue(metric.current, metric.unit)} /{" "}
                    {formatMetricValue(metric.target, metric.unit)}
                  </div>
                </div>
                <div className="mt-3">
                  <ProgressBar value={progress} tone="emerald" />
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <div>
        <h3 className="text-sm font-semibold text-stone-950">Rubric</h3>
        <div className="mt-3 space-y-3">
          {acceptance.rubric.length === 0 ? (
            <p className="text-sm text-stone-500">No rubric dimensions</p>
          ) : null}
          {acceptance.rubric.map((rubric) => (
            <div
              key={rubric.id}
              className="rounded-md border border-stone-200 bg-stone-50 p-3"
            >
              <div className="flex items-start justify-between gap-3 text-sm">
                <div>
                  <div className="font-semibold text-stone-900">
                    {rubric.dimension}
                  </div>
                  {rubric.description ? (
                    <p className="mt-1 leading-6 text-stone-600">
                      {rubric.description}
                    </p>
                  ) : null}
                </div>
                <div className="shrink-0 font-medium text-stone-900">
                  {formatNumber(rubric.score)} / {rubric.scale}
                </div>
              </div>
              <div className="mt-3">
                <ProgressBar value={rubricProgress(rubric)} tone="stone" />
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
