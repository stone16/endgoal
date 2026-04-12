type ProgressBarProps = {
  value: number;
  tone?: "indigo" | "emerald" | "stone";
};

const TONE_CLASS: Record<NonNullable<ProgressBarProps["tone"]>, string> = {
  indigo: "bg-indigo-500",
  emerald: "bg-emerald-500",
  stone: "bg-stone-400",
};

export function ProgressBar({
  value,
  tone = "indigo",
}: ProgressBarProps) {
  const clampedValue = Number.isFinite(value)
    ? Math.min(Math.max(value, 0), 100)
    : 0;

  return (
    <div className="h-2 w-full overflow-hidden rounded-sm bg-stone-200">
      <div
        className={`h-full rounded-sm transition-[width] duration-300 ${TONE_CLASS[tone]}`}
        style={{ width: `${clampedValue}%` }}
      />
    </div>
  );
}
