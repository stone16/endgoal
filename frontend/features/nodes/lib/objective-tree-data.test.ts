import { describe, expect, it } from "vitest";

import {
  buildObjectiveBreadcrumbTrail,
  flattenObjectiveTree,
} from "./objective-tree-data";

import type { Node } from "@/bindings/Node";
import type { NodeState } from "@/bindings/NodeState";

const nodeState: NodeState = {
  state: "active",
  progress: 60,
  confidence: 0.8,
  next_step: "dispatch the next run",
  effective_policy: {
    tokens_max: null,
    iterations_max: null,
    wallclock_max_s: null,
    allowed_tools: null,
    review_required: null,
  },
  rollup_blockers: [],
};

function node(id: string, parent_id: string | null): Node {
  return {
    id,
    parent_id,
    intent: `Node ${id}`,
    phase: "active",
    acceptance_json: '{"type":"prose","text":"ship it"}',
    local_policy_json: null,
    canonical_artifact_text: null,
    canonical_updated_by_run_id: null,
    next_step_cache: null,
    next_step_cache_for_run_id: null,
    created_at: "2026-04-12T08:00:00Z",
    updated_at: "2026-04-12T08:00:00Z",
  };
}

describe("objective tree data helpers", () => {
  it("flattens a nested objective tree while preserving depth", () => {
    const rows = flattenObjectiveTree({
      node: node("root", null),
      state: nodeState,
      children: [
        {
          node: node("child", "root"),
          state: nodeState,
          children: [
            {
              node: node("leaf", "child"),
              state: nodeState,
              children: [],
            },
          ],
        },
      ],
    });

    expect(rows.map((row) => [row.node.id, row.depth])).toEqual([
      ["root", 0],
      ["child", 1],
      ["leaf", 2],
    ]);
  });

  it("builds a root-to-current breadcrumb trail", () => {
    const trail = buildObjectiveBreadcrumbTrail(
      [node("root", null), node("parent", "root")],
      node("current", "parent"),
    );

    expect(trail.map((item) => item.id)).toEqual(["root", "parent", "current"]);
  });
});
