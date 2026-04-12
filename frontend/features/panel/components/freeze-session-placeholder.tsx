type FreezeSessionPlaceholderProps = {
  isOpen: boolean;
  nodeIntent: string;
  onClose: () => void;
};

export function FreezeSessionPlaceholder({
  isOpen,
  nodeIntent,
  onClose,
}: FreezeSessionPlaceholderProps) {
  if (!isOpen) {
    return null;
  }

  return (
    <aside
      data-node-panel-overlay="true"
      role="dialog"
      aria-label="Freeze session"
      className="fixed bottom-0 right-0 top-0 z-50 flex w-full max-w-xl flex-col border-l border-stone-200 bg-white shadow-[-12px_0_32px_rgba(28,25,23,0.12)] sm:w-[30rem]"
    >
      <div className="flex items-center justify-between gap-4 border-b border-stone-200 px-5 py-4">
        <div className="text-sm font-semibold uppercase tracking-[0.08em] text-stone-500">
          Freeze Session
        </div>
        <button
          type="button"
          onClick={onClose}
          className="inline-flex h-9 w-9 items-center justify-center rounded-md border border-stone-300 text-lg leading-none text-stone-500 transition-colors hover:border-stone-400 hover:text-stone-950"
          aria-label="Close freeze session"
        >
          ×
        </button>
      </div>
      <div className="flex flex-1 items-center justify-center p-8">
        <div className="max-w-sm text-center">
          <div className="mx-auto h-10 w-10 animate-pulse rounded-full bg-indigo-100" />
          <h2 className="mt-4 text-xl font-semibold text-stone-950">
            Preparing freeze session
          </h2>
          <p className="mt-3 text-sm leading-6 text-stone-600">
            {nodeIntent}
          </p>
        </div>
      </div>
    </aside>
  );
}
