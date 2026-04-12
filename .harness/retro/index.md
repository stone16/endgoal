# Harness Retro Index

## Frequency Table

| Pattern | Count | Last Seen | Status | Notes |
|---|---:|---|---|---|
| coverage-gate-drift | 1 | 2026-04-13 | Proposed | Task-specific 95% coverage gate was not consistently enforced during checkpoint evaluation. |
| local-artifact-evidence | 1 | 2026-04-13 | Monitoring | PR evidence included local harness artifact paths, but command summaries were present. |
| full-verify-formatting-churn | 1 | 2026-04-13 | Monitoring | Late coverage fix also included workspace formatting normalization. |

## Pending Proposals

- coverage-gate-drift: Apply task-specific coverage thresholds during checkpoint evaluation and treat unmeasured required coverage as a hard gate.

## Retro Entries

| Date | Task | PR | Summary |
|---|---|---|---|
| 2026-04-13 | endgoal-kr-001 | https://github.com/stone16/endgoal/pull/1 | 13 checkpoints passed first try; review loop consensus reached; full verify passed with one frontend coverage-provider warning. |

## Filed Issues

- coverage-gate-drift: https://github.com/stone16/endgoal/issues/2
