interface CompareRow {
  dim: string;
  before: string;
  after: string;
}

interface CompareTableProps {
  before?: string;
  after?: string;
  rows: CompareRow[];
}

export function CompareTable({
  before = "传统方案",
  after = "Sharpside",
  rows,
}: CompareTableProps) {
  if (!rows || !Array.isArray(rows)) return null;
  return (
    <div className="my-6 overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-[var(--glass-border)]">
            <th className="px-4 py-3 text-left font-semibold text-[rgb(var(--text-secondary))]">
              &nbsp;
            </th>
            <th className="px-4 py-3 text-left font-semibold text-rose-400/80">{before}</th>
            <th className="px-4 py-3 text-left font-semibold text-emerald-400">{after}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => (
            <tr
              key={i}
              className="border-b border-[var(--border-subtle)] transition-colors hover:bg-[var(--overlay-subtle)]"
            >
              <td className="px-4 py-3 font-medium">{row.dim}</td>
              <td className="px-4 py-3 text-[rgb(var(--text-muted))]">{row.before}</td>
              <td className="px-4 py-3 text-emerald-400/90">{row.after}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
