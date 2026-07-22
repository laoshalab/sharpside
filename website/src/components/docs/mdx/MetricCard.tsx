interface MetricItem {
  value: string;
  label: string;
}

interface MetricCardProps {
  items: MetricItem[];
}

export function MetricCard({ items }: MetricCardProps) {
  if (!items || !Array.isArray(items)) return null;
  return (
    <div className="my-6 grid grid-cols-2 gap-3 sm:grid-cols-4">
      {items.map((item, i) => (
        <div
          key={i}
          className="glass-card flex flex-col items-center justify-center rounded-xl p-4 text-center"
        >
          <span className="gradient-text text-2xl font-bold sm:text-3xl">{item.value}</span>
          <span className="mt-1 text-xs text-[rgb(var(--text-muted))]">{item.label}</span>
        </div>
      ))}
    </div>
  );
}
