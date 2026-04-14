import type { Assertion } from "@/bindings/Assertion";
import type { AssertionStatus } from "@/bindings/AssertionStatus";
import type { Metric } from "@/bindings/Metric";
import type { RubricDimension } from "@/bindings/RubricDimension";

export type FreezeLayer = "assertions" | "metrics" | "rubric" | "complete";

export type ApprovedFreezeItem = {
  layer: FreezeLayer | string;
  item_json: string;
};

export type EditableFreezeItem =
  | { kind: "assertion"; value: Assertion }
  | { kind: "metric"; value: Metric }
  | { kind: "rubric"; value: RubricDimension }
  | { kind: "json"; value: string };

export const FREEZE_LAYER_LABEL: Record<FreezeLayer, string> = {
  assertions: "Assertions",
  metrics: "Metrics",
  rubric: "Rubric",
  complete: "Done",
};

const ASSERTION_STATUS_VALUES: AssertionStatus[] = ["pass", "fail", "pending"];

function safeParseJson(value: string): unknown {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return null;
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object"
    ? (value as Record<string, unknown>)
    : {};
}

function stringValue(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function nullableStringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value : null;
}

function numberValue(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function nullableNumberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function assertionStatus(value: unknown): AssertionStatus {
  return typeof value === "string" &&
    ASSERTION_STATUS_VALUES.includes(value as AssertionStatus)
    ? (value as AssertionStatus)
    : "pending";
}

export function normalizeFreezeLayer(
  layer: string | null | undefined,
): FreezeLayer {
  if (layer === "assertion" || layer === "assertions") {
    return "assertions";
  }

  if (layer === "metric" || layer === "metrics") {
    return "metrics";
  }

  if (layer === "rubric") {
    return "rubric";
  }

  return "complete";
}

export function parseApprovedFreezeItems(
  approvedItemsJson: string | null | undefined,
): ApprovedFreezeItem[] {
  if (!approvedItemsJson) {
    return [];
  }

  const parsed = safeParseJson(approvedItemsJson);

  return Array.isArray(parsed)
    ? parsed
        .map((item) => asRecord(item))
        .filter(
          (item) =>
            typeof item.layer === "string" &&
            typeof item.item_json === "string",
        )
        .map((item) => ({
          layer: stringValue(item.layer),
          item_json: stringValue(item.item_json),
        }))
    : [];
}

export function approvedFreezeItemCount(
  approvedItemsJson: string | null | undefined,
): number {
  return parseApprovedFreezeItems(approvedItemsJson).length;
}

export function parseEditableFreezeItem(
  layer: string,
  itemJson: string,
): EditableFreezeItem {
  const parsed = asRecord(safeParseJson(itemJson));
  const normalizedLayer = normalizeFreezeLayer(layer);

  if (normalizedLayer === "assertions") {
    return {
      kind: "assertion",
      value: {
        id: stringValue(parsed.id, "a1"),
        text: stringValue(parsed.text),
        check_fn: nullableStringValue(parsed.check_fn),
        status: assertionStatus(parsed.status),
      },
    };
  }

  if (normalizedLayer === "metrics") {
    return {
      kind: "metric",
      value: {
        id: stringValue(parsed.id, "m1"),
        name: stringValue(parsed.name, "metric"),
        baseline: nullableNumberValue(parsed.baseline),
        current: nullableNumberValue(parsed.current),
        target: numberValue(parsed.target, 1),
        unit: nullableStringValue(parsed.unit),
      },
    };
  }

  if (normalizedLayer === "rubric") {
    return {
      kind: "rubric",
      value: {
        id: stringValue(parsed.id, "r1"),
        dimension: stringValue(parsed.dimension, "quality"),
        score: nullableNumberValue(parsed.score),
        scale: numberValue(parsed.scale, 10),
        description: nullableStringValue(parsed.description),
      },
    };
  }

  return {
    kind: "json",
    value: itemJson,
  };
}

export function serializeEditableFreezeItem(item: EditableFreezeItem): string {
  if (item.kind === "json") {
    return item.value;
  }

  return JSON.stringify(item.value);
}

export function layerProgressIndex(layer: FreezeLayer): number {
  if (layer === "assertions") {
    return 0;
  }

  if (layer === "metrics") {
    return 1;
  }

  if (layer === "rubric") {
    return 2;
  }

  return 3;
}
