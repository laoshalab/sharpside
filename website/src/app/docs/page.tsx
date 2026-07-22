"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { BookOpen, Wrench, ArrowRight } from "lucide-react";

export default function DocsHomePage() {
  const t = useTranslations("docs");

  const categories = [
    {
      key: "guide",
      icon: BookOpen,
      title: t("guide"),
      desc: t("guideDesc"),
      cta: t("guideCta"),
      href: "/docs/guide/overview",
      gradient: "from-teal-500/10 to-emerald-500/10",
      iconBg: "bg-teal-500/10",
      iconColor: "text-teal-400",
      borderHover: "hover:border-teal-500/30",
    },
    {
      key: "technical",
      icon: Wrench,
      title: t("technical"),
      desc: t("technicalDesc"),
      cta: t("technicalCta"),
      href: "/docs/technical/getting-started/setup",
      gradient: "from-amber-500/10 to-orange-500/10",
      iconBg: "bg-amber-500/10",
      iconColor: "text-amber-400",
      borderHover: "hover:border-amber-500/30",
    },
  ];

  return (
    <div className="px-4 py-12 sm:px-8 lg:px-12">
      <div className="mb-12 text-center">
        <div className="mb-4 inline-flex items-center gap-2 rounded-full bg-[var(--overlay-subtle)] px-4 py-1.5 text-sm text-[rgb(var(--text-muted))]">
          <BookOpen size={14} />
          {t("badge")}
        </div>
        <h1 className="font-display text-3xl font-bold sm:text-4xl">{t("title")}</h1>
        <p className="mx-auto mt-4 max-w-xl text-[rgb(var(--text-muted))]">{t("subtitle")}</p>
      </div>

      <div className="mx-auto grid max-w-4xl gap-6 md:grid-cols-2">
        {categories.map((cat) => (
          <Link
            key={cat.key}
            href={cat.href}
            className={`glass-card-hover group relative overflow-hidden rounded-2xl p-8 transition-all ${cat.borderHover}`}
          >
            <div
              className={`absolute inset-0 bg-gradient-to-br ${cat.gradient} opacity-0 transition-opacity group-hover:opacity-100`}
            />
            <div className="relative">
              <div className={`mb-5 inline-flex rounded-xl p-3 ${cat.iconBg}`}>
                <cat.icon size={24} className={cat.iconColor} />
              </div>
              <h2 className="mb-2 text-xl font-bold">{cat.title}</h2>
              <p className="mb-6 text-sm leading-relaxed text-[rgb(var(--text-muted))]">
                {cat.desc}
              </p>
              <span className="inline-flex items-center gap-1.5 text-sm font-medium text-[rgb(var(--text-secondary))] transition-colors group-hover:text-[rgb(var(--text-primary))]">
                {cat.cta}
                <ArrowRight
                  size={14}
                  className="transition-transform group-hover:translate-x-1"
                />
              </span>
            </div>
          </Link>
        ))}
      </div>
    </div>
  );
}
