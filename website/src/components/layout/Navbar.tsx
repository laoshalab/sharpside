"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { Menu, Moon, Sun, X } from "lucide-react";
import { useTheme } from "@/components/ui/ThemeProvider";
import { LanguageSwitcher } from "@/components/shared/LanguageSwitcher";
import { cn } from "@/lib/utils";

const APP_HREF = process.env.NEXT_PUBLIC_APP_URL || "http://localhost:8080";

export function Navbar() {
  const pathname = usePathname();
  const { theme, toggle } = useTheme();
  const [mobileOpen, setMobileOpen] = useState(false);
  const [scrolled, setScrolled] = useState(false);
  const t = useTranslations("nav");

  const navLinks = [
    { href: "/copy", label: t("copy"), highlight: true },
    { href: "/channels", label: t("channels") },
    { href: "/venues", label: t("venues") },
    { href: "/risk", label: t("risk") },
    { href: "/tech", label: t("tech") },
    { href: "/docs", label: t("docs") },
  ];

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 20);
    onScroll();
    window.addEventListener("scroll", onScroll);
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    setMobileOpen(false);
  }, [pathname]);

  const isActive = (href: string) =>
    pathname === href || pathname.startsWith(href + "/");

  const linkClass = (href: string, highlight?: boolean) =>
    cn(
      "rounded-lg px-3 py-2 text-sm transition-colors hover:bg-[var(--overlay-subtle)] hover:text-[rgb(var(--text-primary))]",
      isActive(href)
        ? "bg-[rgb(var(--accent))]/10 font-medium text-[rgb(var(--accent))]"
        : highlight
          ? "font-medium text-[rgb(var(--accent))]"
          : "text-[rgb(var(--text-secondary))]"
    );

  return (
    <header
      className={cn(
        "fixed inset-x-0 top-0 z-50 w-full transition-all duration-300",
        scrolled || mobileOpen
          ? "border-b border-[var(--glass-border)] bg-[rgb(var(--bg-primary))]/80 backdrop-blur-xl"
          : "bg-transparent"
      )}
    >
      <nav className="mx-auto flex h-16 max-w-7xl items-center justify-between gap-3 px-4 sm:px-6 lg:px-8">
        <Link href="/" className="flex shrink-0 items-center gap-2.5">
          <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br from-[#00c2a8] to-[#ffb020] text-sm font-bold text-[#041018]">
            S
          </span>
          <span className="font-display text-lg font-bold tracking-tight">Sharpside</span>
        </Link>

        <div className="hidden items-center gap-0.5 md:flex">
          {navLinks.map((link) => (
            <Link
              key={link.href}
              href={link.href}
              className={linkClass(link.href, link.highlight)}
            >
              {link.label}
            </Link>
          ))}
        </div>

        <div className="hidden items-center gap-2 md:flex">
          <LanguageSwitcher />
          <button
            onClick={toggle}
            aria-label={t("toggleTheme")}
            className="rounded-lg p-2 text-[rgb(var(--text-secondary))] hover:bg-[var(--overlay-subtle)] hover:text-[rgb(var(--text-primary))]"
          >
            {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
          </button>
          <a
            href={APP_HREF}
            className="rounded-lg bg-[rgb(var(--accent))] px-4 py-2 text-sm font-semibold text-[#041018] transition hover:brightness-110"
          >
            {t("launchApp")}
          </a>
        </div>

        <div className="flex items-center gap-1 md:hidden">
          <button
            onClick={toggle}
            aria-label={t("toggleTheme")}
            className="rounded-lg p-2 text-[rgb(var(--text-secondary))] hover:bg-[var(--overlay-subtle)]"
          >
            {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
          </button>
          <button
            onClick={() => setMobileOpen((v) => !v)}
            aria-label={t("openMenu")}
            className="rounded-lg p-2 text-[rgb(var(--text-secondary))] hover:bg-[var(--overlay-subtle)]"
          >
            {mobileOpen ? <X size={20} /> : <Menu size={20} />}
          </button>
        </div>
      </nav>

      {mobileOpen && (
        <div className="border-t border-[var(--glass-border)] bg-[rgb(var(--bg-primary))]/95 backdrop-blur-xl md:hidden">
          <div className="space-y-1 px-4 py-4">
            {navLinks.map((link) => (
              <Link
                key={link.href}
                href={link.href}
                className={cn("block", linkClass(link.href, link.highlight), "py-2.5")}
              >
                {link.label}
              </Link>
            ))}
            <div className="flex items-center gap-3 border-t border-[var(--glass-border)] pt-4">
              <LanguageSwitcher />
            </div>
            <a
              href={APP_HREF}
              className="mt-2 block rounded-lg bg-[rgb(var(--accent))] px-3 py-2.5 text-center text-sm font-semibold text-[#041018]"
            >
              {t("launchApp")}
            </a>
          </div>
        </div>
      )}
    </header>
  );
}
