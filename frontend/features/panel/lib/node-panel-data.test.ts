import { describe, expect, it } from "vitest";

import {
  buildRunDispatchRequest,
  getTriggerRunGate,
  getRunFindingsSnippet,
  parseNodeAcceptance,
  sortRunsNewestFirst,
} from "./node-panel-data";

import type { Run } from "@/bindings/Run";

const baseRun: Run = {
  id: "run-1",
  node_id: "node-1",
  type: "research_iteration",
  status: "completed",
  runtime: "echo",
  input_snapshot_json: null,
  output_json: null,
  scratchpad_path: null,
  started_at: null,
  ended_at: null,
  created_at: "2026-04-12T08:00:00Z",
};

describe("node panel data helpers", () => {
  it("parses structured acceptance into assertion, metric, and rubric groups", () => {
    const acceptance = parseNodeAcceptance(
      JSON.stringify({
        type: "structured",
        assertions: [
          {
            id: "a1",
            text: "artifact is reproducible",
            check_fn: "manual",
            status: "pass",
          },
        ],
        metrics: [
          {
            id: "m1",
            name: "accuracy",
            baseline: 0,
            current: 60,
            target: 100,
            unit: "%",
          },
        ],
        rubric: [
          {
            id: "r1",
            dimension: "clarity",
            score: 8,
            scale: 10,
            description: "reviewer can audit it",
          },
        ],
      }),
    );

    expect(acceptance?.type).toBe("structured");

    if (acceptance?.type !== "structured") {
      throw new Error("expected structured acceptance");
    }

    expect(acceptance.assertions[0]?.status).toBe("pass");
    expect(acceptance.metrics[0]?.current).toBe(60);
    expect(acceptance.rubric[0]?.dimension).toBe("clarity");
  });

  it("sorts runs with the newest run first", () => {
    const oldest: Run = {
      ...baseRun,
      id: "oldest",
      created_at: "2026-04-12T08:00:00Z",
    };
    const newest: Run = {
      ...baseRun,
      id: "newest",
      created_at: "2026-04-12T10:00:00Z",
    };

    expect(sortRunsNewestFirst([oldest, newest]).map((run) => run.id)).toEqual([
      "newest",
      "oldest",
    ]);
  });

  it("extracts a findings snippet from run output_json", () => {
    const snippet = getRunFindingsSnippet({
      ...baseRun,
      output_json: JSON.stringify({
        findings:
          "This run found a concrete acceptance gap that should be reviewed before completion.",
      }),
    });

    expect(snippet).toBe(
      "This run found a concrete acceptance gap that should be reviewed before completion.",
    );
  });

  it("routes structured active nodes to direct run dispatch", () => {
    const acceptance = parseNodeAcceptance(
      JSON.stringify({
        type: "structured",
        assertions: [],
        metrics: [],
        rubric: [],
      }),
    );

    expect(getTriggerRunGate("active", acceptance)).toBe("direct");
  });

  it("routes prose acceptance to the Archetype B gate", () => {
    const acceptance = parseNodeAcceptance(
      JSON.stringify({
        type: "prose",
        text: "still needs freezing",
      }),
    );

    expect(getTriggerRunGate("draft", acceptance)).toBe("archetype_b");
  });

  it("builds the exploration dispatch payload with the frontend runtime", () => {
    expect(buildRunDispatchRequest("exploration")).toEqual({
      type: "exploration",
      runtime: "echo",
    });
  });
});
