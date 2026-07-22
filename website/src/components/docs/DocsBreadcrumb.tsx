"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { ChevronRight } from "lucide-react";

interface DocsBreadcrumbProps {
  category: string;
  section?: string;
  title: string;
}

export function DocsBreadcrumb({ category, section, title }: DocsBreadcrumbProps) {
  const t = useTranslations("docs");

  const categoryLabels: Record<string, string> = {
    guide: t("guide"),
    technical: t("technical"),
  };

  const sectionLabels: Record<string, string> = {
    "getting-started": t("sections.getting-started"),
    architecture: t("sections.architecture"),
    operations: t("sections.operations"),
  };

  return (
    <nav className="mb-6 flex flex-wrap items-center gap-1 text-sm text-[rgb(var(--text-muted))]">
      <Link href="/docs" className="transition-colors hover:text-[rgb(var(--text-secondary))]">
        {t("title")}
      </Link>
      <ChevronRight size={14} />
      <Link
        href={`/docs/${category === "guide" ? "guide/overview" : "technical/getting-started/setup"}`}
        className="transition-colors hover:text-[rgb(var(--text-secondary))]"
      >
        {categoryLabels[category] || category}
      </Link>
      {section && (
        <>
          <ChevronRight size={14} />
          <span>{sectionLabels[section] || section.replace(/-/g, " ")}</span>
        </>
      )}
      <ChevronRight size={14} />
      <span className="font-medium text-[rgb(var(--text-primary))]">{title}</span>
    </nav>
  );
}
