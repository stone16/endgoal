const ACTIONS = ["Edit Intent", "Trigger Run", "Add Note", "Archive"];

type PanelActionsProps = {
  isTriggerRunBusy: boolean;
  onTriggerRun: () => void;
  triggerRunDisabled: boolean;
};

export function PanelActions({
  isTriggerRunBusy,
  onTriggerRun,
  triggerRunDisabled,
}: PanelActionsProps) {
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
