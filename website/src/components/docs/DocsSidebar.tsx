"use client";

import { useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import {
  ChevronDown,
  ChevronRight,
  BookOpen,
  Wrench,
  FileText,
  Rocket,
  Shield,
  Users,
  Layers,
  Database,
  Server,
  GitBranch,
  Bot,
  Eye,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { SidebarSection, SidebarItem } from "@/lib/docs";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const iconMap: Record<string, React.ComponentType<any>> = {
  "book-open": BookOpen,
  wrench: Wrench,
  "file-text": FileText,
  rocket: Rocket,
  shield: Shield,
  users: Users,
  layers: Layers,
  database: Database,
  server: Server,
  "git-branch": GitBranch,
  bot: Bot,
  eye: Eye,
};

function getIcon(name?: string) {
  if (!name) return FileText;
  return iconMap[name] || FileText;
}

function SidebarLink({ item, depth = 0 }: { item: SidebarItem; depth?: number }) {
  const pathname = usePathname();
  const t = useTranslations("docs");
  const isActive = pathname === item.href;
  const [expanded, setExpanded] = useState(
    item.children?.some((c) => pathname === c.href || pathname.startsWith(c.href + "/")) ||
      false
  );
  const Icon = getIcon(item.icon);

  const knownSections = ["getting-started", "architecture", "operations"] as const;
  const displayTitle = knownSections.includes(item.title as (typeof knownSections)[number])
    ? t(`sections.${item.title}` as "sections.getting-started")
    : item.title;

  if (item.children && item.children.length > 0) {
    return (
      <div>
        <button
          onClick={() => setExpanded(!expanded)}
          className={cn(
            "flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors",
            "text-[rgb(var(--text-secondary))] hover:bg-[var(--overlay-subtle)] hover:text-[rgb(var(--text-primary))]"
          )}
          style={{ paddingLeft: `${12 + depth * 12}px` }}
        >
          {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          <span className="font-medium">{displayTitle}</span>
        </button>
        {expanded && (
          <div className="mt-0.5">
            {item.children.map((child) => (
              <SidebarLink key={child.href} item={child} depth={depth + 1} />
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <Link
      href={item.href}
      className={cn(
        "flex items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors",
        isActive
          ? "bg-[rgb(var(--accent))]/10 text-[rgb(var(--accent))] font-medium"
          : "text-[rgb(var(--text-secondary))] hover:bg-[var(--overlay-subtle)] hover:text-[rgb(var(--text-primary))]"
      )}
      style={{ paddingLeft: `${12 + depth * 12}px` }}
    >
      <Icon
        size={14}
        className={isActive ? "text-[rgb(var(--accent))]" : "text-[rgb(var(--text-muted))]"}
      />
      <span>{item.title}</span>
    </Link>
  );
}

interface DocsSidebarProps {
  sections: SidebarSection[];
  mobileOpen?: boolean;
  onMobileClose?: () => void;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const sectionIcons: Record<string, React.ComponentType<any>> = {
  guide: BookOpen,
  technical: Wrench,
};

export function DocsSidebar({ sections, mobileOpen, onMobileClose }: DocsSidebarProps) {
  const t = useTranslations("docs");

  const sectionLabels: Record<string, string> = {
    guide: t("guide"),
    technical: t("technical"),
  };

  const sidebar = (
    <nav className="flex h-full flex-col overflow-y-auto pb-8">
      <div className="sticky top-0 z-10 flex items-center justify-between bg-[rgb(var(--bg-primary))]/80 px-4 py-4 backdrop-blur-sm md:hidden">
        <span className="text-sm font-bold">{t("title")}</span>
        <button onClick={onMobileClose} className="rounded-lg p-1 hover:bg-[var(--overlay-subtle)]">
          <X size={18} />
        </button>
      </div>
      <div className="space-y-6 px-2 pt-4">
        {sections.map((section) => {
          const SIcon = sectionIcons[section.category] || FileText;
          return (
            <div key={section.category}>
              <div className="mb-2 flex items-center gap-2 px-3">
                <SIcon size={14} className="text-[rgb(var(--text-muted))]" />
                <span className="text-xs font-bold uppercase tracking-wider text-[rgb(var(--text-muted))]">
                  {sectionLabels[section.category] || section.title}
                </span>
              </div>
              <div className="space-y-0.5">
                {section.items.map((item) => (
                  <SidebarLink key={item.href} item={item} />
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </nav>
  );

  return (
    <>
      <aside className="hidden w-64 shrink-0 border-r border-[var(--glass-border)] md:block">
        <div className="sticky top-16 h-[calc(100vh-4rem)] overflow-y-auto">{sidebar}</div>
      </aside>

      {mobileOpen && (
        <div className="fixed inset-0 z-50 md:hidden">
          <div className="absolute inset-0 bg-black/50" onClick={onMobileClose} />
          <aside className="absolute inset-y-0 left-0 w-72 bg-[rgb(var(--bg-primary))] shadow-2xl">
            {sidebar}
          </aside>
        </div>
      )}
    </>
  );
}
