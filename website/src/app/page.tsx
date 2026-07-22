"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { ArrowRight, BookOpen, Layers, Shield, Zap } from "lucide-react";

const pillarKeys = [
  { key: "reach", icon: Layers },
  { key: "discover", icon: BookOpen },
  { key: "follow", icon: Zap },
  { key: "operate", icon: Shield },
] as const;

export default function HomePage() {
  const t = useTranslations("home");

  return (
    <div className="pt-16">
      <section className="relative overflow-hidden px-4 pb-20 pt-16 sm:px-6 sm:pt-24">
        <div className="mx-auto max-w-4xl text-center">
          <p className="mb-4 text-sm font-semibold uppercase tracking-[0.2em] text-[rgb(var(--accent))]">
            Sharpside
          </p>
          <h1 className="font-display text-4xl font-bold tracking-tight sm:text-5xl lg:text-6xl">
            {t("heading")}
            <span className="gradient-text"> {t("headingAccent")}</span>
          </h1>
          <p className="mx-auto mt-5 max-w-2xl text-lg text-[rgb(var(--text-muted))]">
            {t("subtitle")}
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Link
              href="/docs"
              className="inline-flex items-center gap-2 rounded-xl bg-[rgb(var(--accent))] px-5 py-2.5 text-sm font-semibold text-[#041018] transition hover:brightness-110"
            >
              {t("ctaPrimary")}
              <ArrowRight size={16} />
            </Link>
            <Link
              href="/docs/technical/architecture/overview"
              className="inline-flex items-center gap-2 rounded-xl border border-[var(--glass-border)] px-5 py-2.5 text-sm font-medium text-[rgb(var(--text-secondary))] transition hover:border-[var(--glass-hover-border)] hover:text-[rgb(var(--text-primary))]"
            >
              {t("ctaSecondary")}
            </Link>
          </div>
        </div>
      </section>

      <section className="border-t border-[var(--border-subtle)] px-4 py-16 sm:px-6">
        <div className="mx-auto grid max-w-5xl gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {pillarKeys.map((p) => (
            <div key={p.key} className="glass-card rounded-2xl p-5">
              <p.icon size={20} className="mb-3 text-[rgb(var(--accent))]" />
              <h2 className="font-display text-lg font-semibold">
                {t(`pillars.${p.key}.title`)}
              </h2>
              <p className="mt-2 text-sm leading-relaxed text-[rgb(var(--text-muted))]">
                {t(`pillars.${p.key}.desc`)}
              </p>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
