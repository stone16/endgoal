import { describe, expect, it } from "vitest";

import {
  approvedFreezeItemCount,
  normalizeFreezeLayer,
  parseApprovedFreezeItems,
  parseEditableFreezeItem,
  serializeEditableFreezeItem,
} from "./freeze-session-data";

describe("freeze session data helpers", () => {
  it("normalizes backend layer names", () => {
    expect(normalizeFreezeLayer("assertion")).toBe("assertions");
    expect(normalizeFreezeLayer("metrics")).toBe("metrics");
    expect(normalizeFreezeLayer("rubric")).toBe("rubric");
    expect(normalizeFreezeLayer("done")).toBe("complete");
  });

  it("parses approved freeze item rows from persisted JSON", () => {
    const approvedItems = parseApprovedFreezeItems(
      JSON.stringify([
        {
          layer: "assertions",
          item_json: "{\"id\":\"a1\",\"text\":\"specific\",\"status\":\"pending\"}",
        },
      ]),
    );

    expect(approvedItems).toHaveLength(1);
    expect(approvedItems[0]?.layer).toBe("assertions");
    expect(approvedFreezeItemCount(JSON.stringify(approvedItems))).toBe(1);
  });

  it("parses assertion proposal JSON into editable fields", () => {
    const item = parseEditableFreezeItem(
      "assertion",
      JSON.stringify({
        id: "a1",
        text: "artifact is reproducible",
        status: "pending",
      }),
    );

    expect(item.kind).toBe("assertion");

    if (item.kind !== "assertion") {
      throw new Error("expected assertion");
    }

    expect(item.value.text).toBe("artifact is reproducible");
    expect(serializeEditableFreezeItem(item)).toContain("artifact is reproducible");
  });

  it("falls back to safe defaults for metric proposal JSON", () => {
    const item = parseEditableFreezeItem("metric", "{}");

    expect(item.kind).toBe("metric");

    if (item.kind !== "metric") {
      throw new Error("expected metric");
    }

    expect(item.value.name).toBe("metric");
    expect(item.value.target).toBe(1);
  });
});
