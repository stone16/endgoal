type ArchetypeBGateModalProps = {
  isDispatching: boolean;
  isOpen: boolean;
  nodeIntent: string;
  onCancel: () => void;
  onFreezeNow: () => void;
  onProceedAsExploration: () => void;
};

export function ArchetypeBGateModal({
  isDispatching,
  isOpen,
  nodeIntent,
  onCancel,
  onFreezeNow,
  onProceedAsExploration,
}: ArchetypeBGateModalProps) {
  if (!isOpen) {
    return null;
  }

  return (
    <div
      data-node-panel-overlay="true"
      className="fixed inset-0 z-50 overflow-y-auto bg-stone-950/35 px-4 py-6"
      role="dialog"
      aria-modal="true"
      aria-labelledby="archetype-b-title"
    >
      <div className="flex min-h-full items-center justify-center">
        <div className="w-full max-w-md rounded-lg border border-stone-200 bg-white p-5 shadow-xl">
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-stone-500">
            {nodeIntent}
          </p>
          <h2
            id="archetype-b-title"
            className="mt-3 text-xl font-semibold text-stone-950"
          >
            Freeze acceptance before a normal run
          </h2>
          <p className="mt-3 text-sm leading-6 text-stone-600">
            Exploration can proceed now, or freeze the acceptance criteria
            first.
          </p>

          <div className="mt-5 grid gap-3">
            <button
              type="button"
              onClick={onFreezeNow}
              className="inline-flex h-11 items-center justify-center rounded-md bg-indigo-600 px-4 text-sm font-semibold text-white transition-colors hover:bg-indigo-700"
            >
              Freeze now
            </button>
            <button
              type="button"
              onClick={onProceedAsExploration}
              disabled={isDispatching}
              className="inline-flex h-11 items-center justify-center rounded-md border border-stone-300 px-4 text-sm font-semibold text-stone-700 transition-colors hover:border-stone-400 hover:text-stone-950"
            >
              {isDispatching ? "Dispatching" : "Proceed as exploration"}
            </button>
            <button
              type="button"
              onClick={onCancel}
              className="inline-flex h-11 items-center justify-center rounded-md px-4 text-sm font-semibold text-stone-500 transition-colors hover:bg-stone-100 hover:text-stone-950"
            >
              Cancel
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
