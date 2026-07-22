"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { ArrowLeft, ArrowRight } from "lucide-react";
import type { DocMeta } from "@/lib/docs";

interface DocsPaginationProps {
  prev: DocMeta | null;
  next: DocMeta | null;
  category: string;
}

export function DocsPagination({ prev, next, category }: DocsPaginationProps) {
  const t = useTranslations("docs");

  return (
    <div className="mt-16 flex items-stretch gap-4 border-t border-[var(--border-subtle)] pt-8">
      {prev ? (
        <Link
          href={`/docs/${category}/${prev.slug}`}
          className="glass-card-hover group flex flex-1 items-center gap-3 rounded-xl p-4 transition-all"
        >
          <ArrowLeft
            size={16}
            className="shrink-0 text-[rgb(var(--text-muted))] transition-transform group-hover:-translate-x-1"
          />
          <div className="flex-1 text-right">
            <p className="text-xs text-[rgb(var(--text-muted))]">{t("prevArticle")}</p>
            <p className="text-sm font-medium text-[rgb(var(--text-secondary))] group-hover:text-[rgb(var(--text-primary))]">
              {prev.title}
            </p>
          </div>
        </Link>
      ) : (
        <div className="flex-1" />
      )}

      {next ? (
        <Link
          href={`/docs/${category}/${next.slug}`}
          className="glass-card-hover group flex flex-1 items-center gap-3 rounded-xl p-4 transition-all"
        >
          <div className="flex-1">
            <p className="text-xs text-[rgb(var(--text-muted))]">{t("nextArticle")}</p>
            <p className="text-sm font-medium text-[rgb(var(--text-secondary))] group-hover:text-[rgb(var(--text-primary))]">
              {next.title}
            </p>
          </div>
          <ArrowRight
            size={16}
            className="shrink-0 text-[rgb(var(--text-muted))] transition-transform group-hover:translate-x-1"
          />
        </Link>
      ) : (
        <div className="flex-1" />
      )}
    </div>
  );
}
