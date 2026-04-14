import type { Acceptance } from "@/bindings/Acceptance";
import type { Assertion } from "@/bindings/Assertion";
import type { AssertionStatus } from "@/bindings/AssertionStatus";
import type { Metric } from "@/bindings/Metric";
import type { Phase } from "@/bindings/Phase";
import type { RubricDimension } from "@/bindings/RubricDimension";
import type { Run } from "@/bindings/Run";
import type { RunDispatchRequest } from "@/lib/api";

export type TriggerRunGate = "direct" | "archetype_b" | "unavailable";

type RunOutputSummary = {
  findings?: unknown;
};

const ASSERTION_STATUS_VALUES = new Set<AssertionStatus>([
  "pass",
  "fail",
  "pending",
]);

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object"
    ? (value as Record<string, unknown>)
    : {};
}

function stringValue(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function nullableStringValue(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function numberValue(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function nullableNumberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function assertionStatus(value: unknown): AssertionStatus {
  return typeof value === "string" &&
    ASSERTION_STATUS_VALUES.has(value as AssertionStatus)
    ? (value as AssertionStatus)
    : "pending";
}

function normalizeAssertion(value: unknown): Assertion {
  const assertion = asRecord(value);

  return {
    id: stringValue(assertion.id),
    text: stringValue(assertion.text),
    check_fn: nullableStringValue(assertion.check_fn),
    status: assertionStatus(assertion.status),
  };
}

function normalizeMetric(value: unknown): Metric {
  const metric = asRecord(value);

  return {
    id: stringValue(metric.id),
    name: stringValue(metric.name),
    baseline: nullableNumberValue(metric.baseline),
    current: nullableNumberValue(metric.current),
    target: numberValue(metric.target, 1),
    unit: nullableStringValue(metric.unit),
  };
}

function normalizeRubric(value: unknown): RubricDimension {
  const rubric = asRecord(value);

  return {
    id: stringValue(rubric.id),
    dimension: stringValue(rubric.dimension),
    score: nullableNumberValue(rubric.score),
    scale: numberValue(rubric.scale, 10),
    description: nullableStringValue(rubric.description),
  };
}

export function parseNodeAcceptance(acceptanceJson: string): Acceptance | null {
  try {
    const parsed = asRecord(JSON.parse(acceptanceJson));

    if (parsed.type === "prose") {
      return {
        type: "prose",
        text: stringValue(parsed.text),
      };
    }

    if (parsed.type === "structured") {
      return {
        type: "structured",
        assertions: Array.isArray(parsed.assertions)
          ? parsed.assertions.map(normalizeAssertion)
          : [],
        metrics: Array.isArray(parsed.metrics)
          ? parsed.metrics.map(normalizeMetric)
          : [],
        rubric: Array.isArray(parsed.rubric)
          ? parsed.rubric.map(normalizeRubric)
          : [],
      };
    }

    return null;
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
