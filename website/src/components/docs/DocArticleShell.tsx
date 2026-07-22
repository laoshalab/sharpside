"use client";

import { Calendar, Clock } from "lucide-react";
import { useTranslations } from "next-intl";
import { DocsTOC } from "@/components/docs/DocsTOC";
import { DocsBreadcrumb } from "@/components/docs/DocsBreadcrumb";
import { DocsPagination } from "@/components/docs/DocsPagination";
import type { Doc, DocMeta } from "@/lib/docs";

interface DocArticleShellProps {
  doc: Doc;
  category: string;
  prev: DocMeta | null;
  next: DocMeta | null;
  children: React.ReactNode;
}

export function DocArticleShell({
  doc,
  category,
  prev,
  next,
  children,
}: DocArticleShellProps) {
  const t = useTranslations("docs");
  const section = doc.slug.includes("/") ? doc.slug.split("/")[0] : undefined;

  return (
    <div className="flex">
      <article className="min-w-0 flex-1 px-6 py-8 sm:px-10 lg:px-12">
        <DocsBreadcrumb category={category} section={section} title={doc.title} />

        <div className="mb-8">
          <h1 className="font-display text-3xl font-bold sm:text-4xl">{doc.title}</h1>
          {doc.description && (
            <p className="mt-3 text-lg text-[rgb(var(--text-muted))]">{doc.description}</p>
          )}
          <div className="mt-4 flex flex-wrap items-center gap-4 text-xs text-[rgb(var(--text-muted))]">
            {doc.readingTime > 0 && (
              <span className="flex items-center gap-1">
                <Clock size={12} />
                {doc.readingTime} {t("minutes")}
              </span>
            )}
            {doc.lastUpdated && (
              <span className="flex items-center gap-1">
                <Calendar size={12} />
                {doc.lastUpdated}
              </span>
            )}
            {doc.tags.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {doc.tags.map((tag) => (
                  <span
                    key={tag}
                    className="rounded-full bg-[var(--overlay-subtle)] px-2 py-0.5 text-[10px]"
                  >
                    {tag}
                  </span>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="docs-prose">{children}</div>

        <DocsPagination prev={prev} next={next} category={category} />
      </article>

      <DocsTOC headings={doc.headings} />
    </div>
  );
}
