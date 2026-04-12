import type { Phase } from "@/bindings/Phase";

const PHASE_CONFIG: Record<
  Phase,
  { label: string; className: string }
> = {
  draft: {
    label: "Draft",
    className: "bg-stone-100 text-stone-700 ring-stone-200",
  },
  active: {
    label: "Active",
    className: "bg-indigo-50 text-indigo-700 ring-indigo-200",
  },
  in_review: {
    label: "In Review",
    className: "bg-amber-50 text-amber-700 ring-amber-200",
  },
  complete: {
    label: "Complete",
    className: "bg-emerald-50 text-emerald-700 ring-emerald-200",
  },
  archived: {
    label: "Archived",
    className: "bg-stone-100 text-stone-500 ring-stone-200",
  },
};

export function phaseAccentClass(phase: Phase): string {
  switch (phase) {
    case "draft":
      return "border-stone-300";
    case "active":
      return "border-indigo-500";
    case "in_review":
      return "border-amber-500";
    case "complete":
      return "border-emerald-500";
    case "archived":
      return "border-stone-200";
  }
}

export function PhaseBadge({ phase }: { phase: Phase }) {
  const { label, className } = PHASE_CONFIG[phase];

  return (
    <span
      className={`inline-flex items-center rounded-md px-2.5 py-1 text-xs font-semibold ring-1 ring-inset ${className}`}
    >
      {label}
    </span>
  );
}
