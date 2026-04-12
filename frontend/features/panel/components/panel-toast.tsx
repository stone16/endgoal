type PanelToastProps = {
  onDismiss: () => void;
  toast: {
    message: string;
    tone: "success" | "error";
  } | null;
};

const TOAST_CLASS: Record<NonNullable<PanelToastProps["toast"]>["tone"], string> = {
  success: "border-emerald-200 bg-emerald-50 text-emerald-800",
  error: "border-rose-200 bg-rose-50 text-rose-800",
};

export function PanelToast({ onDismiss, toast }: PanelToastProps) {
  if (!toast) {
    return null;
  }

  return (
    <div
      data-node-panel-overlay="true"
      className={`fixed bottom-4 left-4 right-4 z-50 flex items-center justify-between gap-4 rounded-lg border px-4 py-3 text-sm shadow-lg sm:left-auto sm:w-96 ${TOAST_CLASS[toast.tone]}`}
      role="status"
    >
      <span>{toast.message}</span>
      <button
        type="button"
        onClick={onDismiss}
        className="rounded-md px-2 py-1 font-semibold hover:bg-white/60"
      >
        Dismiss
      </button>
    </div>
  );
}
