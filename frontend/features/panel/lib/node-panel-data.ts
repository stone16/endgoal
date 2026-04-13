import type { Acceptance } from "@/bindings/Acceptance";
import type { Metric } from "@/bindings/Metric";
import type { Phase } from "@/bindings/Phase";
import type { RubricDimension } from "@/bindings/RubricDimension";
import type { Run } from "@/bindings/Run";
import type { RunDispatchRequest } from "@/lib/api";

export type TriggerRunGate = "direct" | "archetype_b" | "unavailable";

type RunOutputSummary = {
  findings?: unknown;
};

export function parseNodeAcceptance(acceptanceJson: string): Acceptance | null {
  try {
    const parsed = JSON.parse(acceptanceJson) as Partial<Acceptance>;

    if (parsed.type !== "prose" && parsed.type !== "structured") {
      return null;
    }

    return parsed as Acceptance;
  } catch {
    return null;
  }
}

export function sortRunsNewestFirst(runs: Run[]): Run[] {
  return [...runs].sort(
    (left, right) =>
      new Date(right.created_at).getTime() -
      new Date(left.created_at).getTime(),
  );
}

export function getTriggerRunGate(
  phase: Phase,
  acceptance: Acceptance | null,
): TriggerRunGate {
  if (acceptance?.type === "prose") {
    return "archetype_b";
  }

  if (acceptance?.type === "structured" && phase === "active") {
    return "direct";
  }

  return "unavailable";
}

export function buildRunDispatchRequest(
  type: RunDispatchRequest["type"],
): RunDispatchRequest {
  return {
    type,
    runtime: "echo",
  };
}

export function getRunFindingsSnippet(run: Run): string {
  if (!run.output_json) {
    return "No findings yet";
  }

  try {
    const output = JSON.parse(run.output_json) as RunOutputSummary;

    if (typeof output.findings === "string" && output.findings.trim()) {
      return output.findings.trim();
    }
  } catch {
    return "Findings unavailable";
  }

  return "No findings yet";
}

export function metricProgress(metric: Metric): number {
  if (metric.current === null || metric.target === 0) {
    return 0;
  }

  return Math.min(Math.max((metric.current / metric.target) * 100, 0), 100);
}

export function rubricProgress(rubric: RubricDimension): number {
  if (rubric.score === null || rubric.scale === 0) {
    return 0;
  }

  return Math.min(Math.max((rubric.score / rubric.scale) * 100, 0), 100);
}
