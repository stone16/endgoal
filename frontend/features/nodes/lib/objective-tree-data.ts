import type { Node } from "@/bindings/Node";
import type { NodeState } from "@/bindings/NodeState";

export type ObjectiveTreeItem = {
  node: Node;
  state: NodeState;
  children: ObjectiveTreeItem[];
};

export type ObjectiveTreeRow = {
  node: Node;
  state: NodeState;
  depth: number;
};

export function flattenObjectiveTree(
  item: ObjectiveTreeItem,
  depth = 0,
): ObjectiveTreeRow[] {
  return [
    {
      node: item.node,
      state: item.state,
      depth,
    },
    ...item.children.flatMap((child) => flattenObjectiveTree(child, depth + 1)),
  ];
}

export function buildObjectiveBreadcrumbTrail(
  ancestors: Node[],
  current: Node | null,
): Node[] {
  return current ? [...ancestors, current] : ancestors;
}
