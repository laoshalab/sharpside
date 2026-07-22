import Link from "next/link";

interface MarketingPageProps {
  eyebrow: string;
  title: string;
  description: string;
  bullets: string[];
  primaryHref: string;
  primaryLabel: string;
  secondaryHref?: string;
  secondaryLabel?: string;
}

export function MarketingPage({
  eyebrow,
  title,
  description,
  bullets,
  primaryHref,
  primaryLabel,
  secondaryHref,
  secondaryLabel,
}: MarketingPageProps) {
  return (
    <div className="pt-16">
      <section className="px-4 pb-20 pt-16 sm:px-6 sm:pt-24">
        <div className="mx-auto max-w-3xl">
          <p className="mb-4 text-sm font-semibold uppercase tracking-[0.2em] text-[rgb(var(--accent))]">
            {eyebrow}
          </p>
          <h1 className="font-display text-4xl font-bold tracking-tight sm:text-5xl">
            {title}
          </h1>
          <p className="mt-5 text-lg text-[rgb(var(--text-muted))]">{description}</p>
          <ul className="mt-8 space-y-3 text-[rgb(var(--text-secondary))]">
            {bullets.map((item) => (
              <li key={item} className="flex gap-3">
                <span className="mt-2 h-1.5 w-1.5 shrink-0 rounded-full bg-[rgb(var(--accent))]" />
                <span>{item}</span>
              </li>
            ))}
          </ul>
          <div className="mt-10 flex flex-wrap gap-3">
            <Link
              href={primaryHref}
              className="inline-flex rounded-xl bg-[rgb(var(--accent))] px-5 py-2.5 text-sm font-semibold text-[#041018] transition hover:brightness-110"
            >
              {primaryLabel}
            </Link>
            {secondaryHref && secondaryLabel && (
              <Link
                href={secondaryHref}
                className="inline-flex rounded-xl border border-[var(--glass-border)] px-5 py-2.5 text-sm font-medium text-[rgb(var(--text-secondary))] transition hover:border-[var(--glass-hover-border)] hover:text-[rgb(var(--text-primary))]"
              >
                {secondaryLabel}
              </Link>
            )}
          </div>
        </div>
      </section>
    </div>
  );
}
