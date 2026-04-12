import type { Phase } from "@/bindings/Phase";

const ACTIONS = ["Edit Intent", "Trigger Run", "Add Note", "Archive"];

type PanelActionsProps = {
  phase: Phase;
  isTriggerRunBusy: boolean;
  isReviewActionBusy: boolean;
  onApproveReview: () => void;
  onRejectReview: () => void;
  onReviewReasonChange: (value: string) => void;
  onTriggerRun: () => void;
  reviewActionError: string | null;
  reviewReason: string;
  triggerRunDisabled: boolean;
};

export function PanelActions({
  phase,
  isTriggerRunBusy,
  isReviewActionBusy,
  onApproveReview,
  onRejectReview,
  onReviewReasonChange,
  onTriggerRun,
  reviewActionError,
  reviewReason,
  triggerRunDisabled,
}: PanelActionsProps) {
  if (phase === "in_review") {
    return (
      <section className="rounded-lg border border-stone-200 p-4">
        <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
          Review
        </h2>
        <label className="mt-3 grid gap-2 text-sm font-medium text-stone-700">
          Reason / tighter constraint
          <textarea
            id="review-reason"
            name="review-reason"
            value={reviewReason}
            onChange={(event) => onReviewReasonChange(event.target.value)}
            placeholder='Optional plain reason or JSON, e.g. {"tokens_max":50000}'
            className="min-h-24 resize-y rounded-md border border-stone-300 bg-white p-3 text-sm leading-6 text-stone-900 outline-none transition-colors focus:border-emerald-500"
          />
        </label>
        {reviewActionError ? (
          <p className="mt-3 text-sm text-rose-700">{reviewActionError}</p>
        ) : null}
        <div className="mt-3 grid gap-2 sm:grid-cols-2">
          <button
            type="button"
            onClick={onApproveReview}
            disabled={isReviewActionBusy}
            className="inline-flex h-10 items-center justify-center rounded-md border border-emerald-700 bg-emerald-700 px-3 text-sm font-semibold text-white transition-colors hover:bg-emerald-800 disabled:cursor-not-allowed disabled:opacity-70"
          >
            {isReviewActionBusy ? "Approving" : "Approve synthesis"}
          </button>
          <button
            type="button"
            onClick={onRejectReview}
            disabled={isReviewActionBusy}
            className="inline-flex h-10 items-center justify-center rounded-md border border-rose-700 bg-rose-700 px-3 text-sm font-semibold text-white transition-colors hover:bg-rose-800 disabled:cursor-not-allowed disabled:opacity-70"
          >
            {isReviewActionBusy ? "Rejecting" : "Reject — back to Active"}
          </button>
        </div>
      </section>
    );
  }

  return (
    <section className="rounded-lg border border-stone-200 p-4">
      <h2 className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
        Actions
      </h2>
      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        {ACTIONS.map((action) => (
          <button
            key={action}
            type="button"
            onClick={action === "Trigger Run" ? onTriggerRun : undefined}
            disabled={
              action === "Trigger Run"
                ? triggerRunDisabled || isTriggerRunBusy
                : false
            }
            className="inline-flex h-10 items-center justify-center rounded-md border border-stone-300 px-3 text-sm font-semibold text-stone-700 transition-colors hover:border-stone-400 hover:text-stone-950"
          >
            {action === "Trigger Run" && isTriggerRunBusy
              ? "Dispatching"
              : action}
          </button>
        ))}
      </div>
    </section>
  );
}
