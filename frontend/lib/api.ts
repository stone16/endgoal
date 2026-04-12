import type { AncestorSummary } from "@/bindings/AncestorSummary";
import type { FreezeProposal } from "@/bindings/FreezeProposal";
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

async function requestJson<T>(
  path: string,
  init: RequestInit = {},
): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set("Accept", "application/json");

  const response = await fetch(buildUrl(path), {
    ...init,
    cache: "no-store",
    headers,
  });

  if (!response.ok) {
    throw new ApiError(response.status, await response.text());
  }

  return (await response.json()) as T;
}

async function requestText(
  path: string,
  init: RequestInit = {},
): Promise<string> {
  const headers = new Headers(init.headers);
  headers.set("Accept", "text/event-stream");

  const response = await fetch(buildUrl(path), {
    ...init,
    cache: "no-store",
    headers,
  });

  if (!response.ok) {
    throw new ApiError(response.status, await response.text());
  }

  return response.text();
}

function parseSseJson<T>(text: string): T {
  const dataLines = text
    .split(/\r?\n/)
    .filter((line) => line.startsWith("data:"))
    .map((line) => line.slice("data:".length).trimStart());
  const payload = dataLines.join("\n").trim();

  if (!payload) {
    throw new Error("Freeze session stream returned no data");
  }

  return JSON.parse(payload) as T;
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

export type DispatchRunResponse = {
  id: string;
  status: string;
};

export type RunDispatchRequest = {
  type: "research_iteration" | "exploration";
  runtime: "echo";
};

export function dispatchRun(
  nodeId: string,
  request: RunDispatchRequest,
): Promise<DispatchRunResponse> {
  return requestJson<DispatchRunResponse>(
    `/api/nodes/${encodeURIComponent(nodeId)}/runs`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(request),
    },
  );
}

export function getRun(id: string): Promise<Run> {
  return requestJson<Run>(`/api/runs/${encodeURIComponent(id)}`);
}

export function getRunEventStreamUrl(id: string): string {
  return buildUrl(`/api/runs/${encodeURIComponent(id)}/stream`);
}

export function approveNode(id: string): Promise<Node> {
  return requestJson<Node>(`/api/nodes/${encodeURIComponent(id)}/approve`, {
    method: "POST",
  });
}

export type RejectNodeRequest = {
  tighter_policy?: Record<string, unknown>;
};

export function rejectNode(
  id: string,
  request?: RejectNodeRequest,
): Promise<Node> {
  return requestJson<Node>(`/api/nodes/${encodeURIComponent(id)}/reject`, {
    method: "POST",
    ...(request
      ? {
          headers: {
            "Content-Type": "application/json",
          },
          body: JSON.stringify(request),
        }
      : {}),
  });
}

export type NodeAncestorsResponse = AncestorSummary[];

export type FreezeActiveSession = {
  session_id: string;
  approved_items_json: string;
  current_layer: string;
};

export type FreezeStartResponse = {
  session_id: string;
};

export type FreezeRespondAction =
  | "start"
  | "approve"
  | "edit"
  | "reject"
  | "skip_layer";

export type FreezeRespondRequest = {
  session_id: string;
  user_response: string;
  action: FreezeRespondAction;
  approved_item_json?: string;
};

export type FreezeLayerCompleteEvent = {
  event_type: "layer_complete";
  layer: string;
  next_layer: string | null;
};

export type FreezeStreamEvent = FreezeProposal | FreezeLayerCompleteEvent;

export function getActiveFreezeSession(
  nodeId: string,
): Promise<FreezeActiveSession | null> {
  return requestJson<FreezeActiveSession | null>(
    `/api/nodes/${encodeURIComponent(nodeId)}/freeze/active`,
  );
}

export function startFreezeSession(
  nodeId: string,
): Promise<FreezeStartResponse> {
  return requestJson<FreezeStartResponse>(
    `/api/nodes/${encodeURIComponent(nodeId)}/freeze/start`,
    {
      method: "POST",
    },
  );
}

export async function respondFreezeSession(
  nodeId: string,
  request: FreezeRespondRequest,
): Promise<FreezeStreamEvent> {
  const text = await requestText(
    `/api/nodes/${encodeURIComponent(nodeId)}/freeze/respond`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(request),
    },
  );

  return parseSseJson<FreezeStreamEvent>(text);
}

export function commitFreezeSession(
  nodeId: string,
  sessionId: string,
): Promise<Node> {
  return requestJson<Node>(
    `/api/nodes/${encodeURIComponent(nodeId)}/freeze/commit`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ session_id: sessionId }),
    },
  );
}
