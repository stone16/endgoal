#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ENDGOAL_SMOKE_PORT:-3321}"
TOKEN="${ENDGOAL_DAEMON_TOKEN:-smoke-token}"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/endgoal-smoke.XXXXXX")"
DB_PATH="$TMP_DIR/smoke.db"
DATABASE_URL="sqlite://$DB_PATH?mode=rwc"
BACKEND_LOG="$TMP_DIR/backend.log"
DAEMON_LOG="$TMP_DIR/daemon.log"
BACKEND_PID=""
DAEMON_PID=""

cleanup() {
  if [[ -n "$DAEMON_PID" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
  fi

  if [[ -n "$BACKEND_PID" ]]; then
    kill "$BACKEND_PID" >/dev/null 2>&1 || true
  fi

  rm -rf "$TMP_DIR"
}

fail() {
  echo "smoke: $*" >&2
  echo "smoke: backend log at $BACKEND_LOG" >&2
  echo "smoke: daemon log at $DAEMON_LOG" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

api_url() {
  printf 'http://127.0.0.1:%s%s' "$PORT" "$1"
}

curl_json() {
  curl -fsS "$@"
}

json_field() {
  jq -r "$1"
}

wait_for_health() {
  for _ in $(seq 1 100); do
    if curl -fsS "$(api_url /api/health)" >/dev/null 2>&1; then
      return 0
    fi

    sleep 0.1
  done

  fail "backend did not become healthy within 10s"
}

wait_for_daemon() {
  for _ in $(seq 1 50); do
    if grep -q "daemon connected" "$DAEMON_LOG" 2>/dev/null; then
      return 0
    fi

    sleep 0.1
  done

  fail "daemon did not connect within 5s"
}

extract_sse_data() {
  sed -n 's/^data: //p' | head -n 1
}

trap cleanup EXIT

require_cmd cargo
require_cmd curl
require_cmd jq
require_cmd node
require_cmd sqlite3

cd "$ROOT_DIR"

ENDGOAL_BACKEND_PORT="$PORT" \
ENDGOAL_DAEMON_TOKEN="$TOKEN" \
ENDGOAL_LLM_STUB=true \
DATABASE_URL="$DATABASE_URL" \
  cargo run -p endgoal-backend >"$BACKEND_LOG" 2>&1 &
BACKEND_PID="$!"

wait_for_health

ENDGOAL_BACKEND_PORT="$PORT" ENDGOAL_DAEMON_TOKEN="$TOKEN" node >"$DAEMON_LOG" 2>&1 <<'NODE' &
const port = process.env.ENDGOAL_BACKEND_PORT;
const token = process.env.ENDGOAL_DAEMON_TOKEN;

if (typeof WebSocket !== "function") {
  console.error("Node.js WebSocket global is unavailable");
  process.exit(1);
}

const socket = new WebSocket(`ws://127.0.0.1:${port}/ws/daemon`, {
  headers: {
    Authorization: `Bearer ${token}`,
  },
});

socket.addEventListener("open", () => {
  console.log("daemon connected");
});

socket.addEventListener("message", (event) => {
  const dispatch = JSON.parse(event.data);
  const output = dispatch.input?.intent ?? "smoke_test_output";

  socket.send(JSON.stringify({
    kind: "event",
    run_id: dispatch.run_id,
    seq: 0,
    event_type: "stdout",
    data_text: output,
  }));

  setTimeout(() => {
    socket.send(JSON.stringify({
      kind: "event",
      run_id: dispatch.run_id,
      seq: 1,
      event_type: "system",
      data_text: "mock daemon completed",
    }));
    socket.send(JSON.stringify({
      kind: "terminal",
      run_id: dispatch.run_id,
      status: "completed",
      error: null,
    }));
  }, 500);
});

socket.addEventListener("error", (error) => {
  console.error("daemon error", error.message ?? error.type ?? error);
});

setInterval(() => {}, 1000);
NODE
DAEMON_PID="$!"

wait_for_daemon

create_node_body="$(jq -n '{
  intent: "smoke_test_output",
  acceptance_json: "{\"type\":\"prose\",\"text\":\"make this measurable\"}"
}')"
node_json="$(curl_json -X POST "$(api_url /api/nodes)" \
  -H 'Content-Type: application/json' \
  --data "$create_node_body")"
node_id="$(printf '%s' "$node_json" | json_field '.id')"
[[ "$node_id" != "null" && -n "$node_id" ]] || fail "node id missing"

freeze_start_json="$(curl_json -X POST "$(api_url "/api/nodes/$node_id/freeze/start")")"
session_id="$(printf '%s' "$freeze_start_json" | json_field '.session_id')"
[[ "$session_id" != "null" && -n "$session_id" ]] || fail "freeze session id missing"

proposal_sse="$(curl_json -X POST "$(api_url "/api/nodes/$node_id/freeze/respond")" \
  -H 'Content-Type: application/json' \
  --data "$(jq -n --arg session_id "$session_id" '{
    session_id: $session_id,
    user_response: "",
    action: "start"
  }')")"
proposal_json="$(printf '%s\n' "$proposal_sse" | extract_sse_data)"
item_json="$(printf '%s' "$proposal_json" | json_field '.item_json')"
[[ "$item_json" != "null" && -n "$item_json" ]] || fail "proposal item_json missing"

curl_json -X POST "$(api_url "/api/nodes/$node_id/freeze/respond")" \
  -H 'Content-Type: application/json' \
  --data "$(jq -n \
    --arg session_id "$session_id" \
    --arg item_json "$item_json" \
    '{
      session_id: $session_id,
      user_response: "approved for smoke test",
      action: "approve",
      approved_item_json: $item_json
    }')" >/dev/null

committed_node="$(curl_json -X POST "$(api_url "/api/nodes/$node_id/freeze/commit")" \
  -H 'Content-Type: application/json' \
  --data "$(jq -n --arg session_id "$session_id" '{ session_id: $session_id }')")"
phase="$(printf '%s' "$committed_node" | json_field '.phase')"
acceptance_type="$(printf '%s' "$committed_node" | jq -r '.acceptance_json | fromjson | .type')"
[[ "$phase" == "active" ]] || fail "expected active phase after freeze commit, got $phase"
[[ "$acceptance_type" == "structured" ]] || fail "expected structured acceptance, got $acceptance_type"

dispatch_json="$(curl_json -X POST "$(api_url "/api/nodes/$node_id/runs")" \
  -H 'Content-Type: application/json' \
  --data '{"type":"research_iteration","runtime":"echo"}')"
run_id="$(printf '%s' "$dispatch_json" | json_field '.id')"
dispatch_status="$(printf '%s' "$dispatch_json" | json_field '.status')"
[[ "$dispatch_status" == "dispatched" ]] || fail "expected dispatched status, got $dispatch_status"

seen_running=0
final_status=""
for _ in $(seq 1 100); do
  run_json="$(curl_json "$(api_url "/api/runs/$run_id")")"
  run_status="$(printf '%s' "$run_json" | json_field '.status')"

  if [[ "$run_status" == "running" ]]; then
    seen_running=1
  fi

  if [[ "$run_status" == "completed" ]]; then
    final_status="$run_status"
    break
  fi

  sleep 0.1
done
[[ "$seen_running" == "1" ]] || fail "run status never reached running"
[[ "$final_status" == "completed" ]] || fail "run did not complete within 10s"

event_count="$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM run_events WHERE run_id = '$run_id' AND data_text LIKE '%smoke_test_output%';")"
[[ "$event_count" -ge 1 ]] || fail "expected smoke_test_output run event row"

curl_json -X PATCH "$(api_url "/api/runs/$run_id/output")" \
  -H 'Content-Type: application/json' \
  --data '{
    "assertion_results": { "a1": "pass" },
    "metric_values": { "m1": 80 },
    "rubric_scores": { "r1": 8 },
    "confidence": 0.8,
    "findings": "smoke test pass",
    "concerns": [],
    "needs_human_review": false
  }' >/dev/null

review_node="$(curl_json -X POST "$(api_url "/api/nodes/$node_id/review")")"
review_phase="$(printf '%s' "$review_node" | json_field '.phase')"
[[ "$review_phase" == "in_review" ]] || fail "expected in_review phase, got $review_phase"

approved_node="$(curl_json -X POST "$(api_url "/api/nodes/$node_id/approve")")"
approved_phase="$(printf '%s' "$approved_node" | json_field '.phase')"
[[ "$approved_phase" == "complete" ]] || fail "expected complete phase, got $approved_phase"

run_snapshot="$(curl_json "$(api_url "/api/runs/$run_id")" | json_field '.input_snapshot_json')"
[[ "$run_snapshot" != "null" && -n "$run_snapshot" ]] || fail "input_snapshot_json missing"

state_json="$(curl_json "$(api_url "/api/nodes/$node_id/state")")"
progress="$(printf '%s' "$state_json" | jq -r '.progress')"
awk -v progress="$progress" 'BEGIN { exit !(progress > 0) }' \
  || fail "expected state progress > 0, got $progress"

echo "smoke: ok"
