"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { getAppHref } from "@/lib/appUrl";

const APP_HREF = getAppHref();

export function Footer() {
  const t = useTranslations("footer");

  const columns = [
    {
      title: t("product"),
      links: [
        { href: "/copy", label: t("copy") },
        { href: "/channels", label: t("channels") },
        { href: "/venues", label: t("venues") },
        { href: "/risk", label: t("risk") },
      ],
    },
    {
      title: t("developers"),
      links: [
        { href: "/tech", label: t("tech") },
        { href: "/docs", label: t("docs") },
        { href: "/docs/technical/getting-started/setup", label: t("setup") },
        { href: "/docs/technical/architecture/overview", label: t("architecture") },
      ],
    },
  ];

  return (
    <footer className="border-t border-[var(--glass-border)] bg-[rgb(var(--bg-footer))]">
      <div className="mx-auto grid max-w-7xl gap-10 px-4 py-12 sm:grid-cols-[1.2fr_1fr_1fr] sm:px-6 lg:px-8">
        <div>
          <p className="font-display text-sm font-bold">Sharpside</p>
          <p className="mt-2 max-w-xs text-xs leading-relaxed text-[rgb(var(--text-muted))]">
            {t("tagline")}
          </p>
          {APP_HREF ? (
            <a
              href={APP_HREF}
              className="mt-4 inline-block text-sm font-medium text-[rgb(var(--accent))] hover:underline"
            >
              {t("launchApp")}
            </a>
          ) : (
            <Link
              href="/docs"
              className="mt-4 inline-block text-sm font-medium text-[rgb(var(--accent))] hover:underline"
            >
              {t("docs")}
            </Link>
          )}
        </div>
        {columns.map((col) => (
          <div key={col.title}>
            <p className="mb-3 text-xs font-bold uppercase tracking-wider text-[rgb(var(--text-muted))]">
              {col.title}
            </p>
            <ul className="space-y-2">
              {col.links.map((link) => (
                <li key={link.href}>
                  <Link
                    href={link.href}
                    className="text-sm text-[rgb(var(--text-secondary))] hover:text-[rgb(var(--accent))]"
                  >
                    {link.label}
                  </Link>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </footer>
  );
}
