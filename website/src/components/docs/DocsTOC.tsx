"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/utils";
import type { Heading } from "@/lib/docs";

interface DocsTOCProps {
  headings: Heading[];
}

export function DocsTOC({ headings }: DocsTOCProps) {
  const t = useTranslations("docs");
  const [activeId, setActiveId] = useState<string>("");

  useEffect(() => {
    if (headings.length === 0) return;

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setActiveId(entry.target.id);
          }
        }
      },
      { rootMargin: "-80px 0px -60% 0px", threshold: 0.1 }
    );

    const elements = headings
      .map((h) => document.getElementById(h.id))
      .filter(Boolean) as HTMLElement[];

    elements.forEach((el) => observer.observe(el));
    return () => observer.disconnect();
  }, [headings]);

  if (headings.length === 0) return null;

  return (
    <aside className="hidden w-52 shrink-0 xl:block">
      <div className="sticky top-20 max-h-[calc(100vh-6rem)] overflow-y-auto pr-4">
        <p className="mb-3 text-xs font-bold uppercase tracking-wider text-[rgb(var(--text-muted))]">
          {t("onThisPage")}
        </p>
        <nav className="space-y-1 border-l border-[var(--border-subtle)]">
          {headings.map((heading) => (
            <a
              key={heading.id}
              href={`#${heading.id}`}
              onClick={(e) => {
                e.preventDefault();
                document.getElementById(heading.id)?.scrollIntoView({ behavior: "smooth" });
              }}
              className={cn(
                "block border-l-2 py-1 text-[13px] leading-snug transition-colors",
                heading.depth === 2 ? "pl-4" : "pl-7",
                activeId === heading.id
                  ? "border-[rgb(var(--accent))] text-[rgb(var(--accent))] font-medium"
                  : "border-transparent text-[rgb(var(--text-muted))] hover:text-[rgb(var(--text-secondary))] hover:border-[var(--glass-border)]"
              )}
            >
              {heading.text}
            </a>
          ))}
        </nav>
      </div>
    </aside>
  );
}
