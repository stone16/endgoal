import type { AncestorSummary } from "@/bindings/AncestorSummary";
import type { Node } from "@/bindings/Node";
import type { NodeState } from "@/bindings/NodeState";
import type { Run } from "@/bindings/Run";

const DEFAULT_API_URL = "http://localhost:3001";

export class ApiError extends Error {
  status: number;
  body: string;

  constructor(status: number, body: string) {
    super(`API request failed with status ${status}`);
    this.name = "ApiError";
    this.status = status;
    this.body = body;
  }
}

function getConfiguredApiUrl(): string {
  const configuredUrl = process.env.NEXT_PUBLIC_API_URL?.trim();

  if (!configuredUrl) {
    return DEFAULT_API_URL;
  }

  return configuredUrl.endsWith("/")
    ? configuredUrl.slice(0, -1)
    : configuredUrl;
}

function buildUrl(path: string): string {
  return new URL(path, `${getConfiguredApiUrl()}/`).toString();
}

async function requestJson<T>(path: string): Promise<T> {
  const response = await fetch(buildUrl(path), {
    cache: "no-store",
    headers: {
      Accept: "application/json",
    },
  });

  if (!response.ok) {
    throw new ApiError(response.status, await response.text());
  }

  return (await response.json()) as T;
}

export function getApiBaseUrl(): string {
  return getConfiguredApiUrl();
}

export function getFrontendWebSocketUrl(): string {
  const url = new URL(getConfiguredApiUrl());
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.pathname = "/ws/frontend";
  url.search = "";
  url.hash = "";
  return url.toString();
}

export function getNodes(): Promise<Node[]> {
  return requestJson<Node[]>("/api/nodes");
}

export function getNode(id: string): Promise<Node> {
  return requestJson<Node>(`/api/nodes/${encodeURIComponent(id)}`);
}

export function getNodeState(id: string, rollupDepth = 1): Promise<NodeState> {
  return requestJson<NodeState>(
    `/api/nodes/${encodeURIComponent(id)}/state?rollup_depth=${rollupDepth}`,
  );
}

export function getNodeAncestors(id: string): Promise<Node[]> {
  return requestJson<Node[]>(
    `/api/nodes/${encodeURIComponent(id)}/ancestors`,
  );
}

export function getNodeChildren(id: string): Promise<Node[]> {
  return requestJson<Node[]>(
    `/api/nodes/${encodeURIComponent(id)}/children`,
  );
}

export function getRuns(nodeId: string): Promise<Run[]> {
  return requestJson<Run[]>(
    `/api/nodes/${encodeURIComponent(nodeId)}/runs`,
  );
}

export function getRun(id: string): Promise<Run> {
  return requestJson<Run>(`/api/runs/${encodeURIComponent(id)}`);
}

export type NodeAncestorsResponse = AncestorSummary[];
