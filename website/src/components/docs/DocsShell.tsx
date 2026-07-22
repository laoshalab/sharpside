"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { Menu } from "lucide-react";
import { DocsSidebar } from "@/components/docs/DocsSidebar";
import type { SidebarSection } from "@/lib/docs";

interface DocsShellProps {
  sidebarData: SidebarSection[];
  children: React.ReactNode;
}

export function DocsShell({ sidebarData, children }: DocsShellProps) {
  const [mobileOpen, setMobileOpen] = useState(false);
  const t = useTranslations("docs");

  return (
    <div className="pt-16">
      <div className="sticky top-16 z-30 flex items-center border-b border-[var(--glass-border)] bg-[rgb(var(--bg-primary))]/80 px-4 py-2 backdrop-blur-xl md:hidden">
        <button
          onClick={() => setMobileOpen(true)}
          className="flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm text-[rgb(var(--text-secondary))] hover:bg-[var(--overlay-subtle)]"
        >
          <Menu size={16} />
          <span>{t("toc")}</span>
        </button>
      </div>

      <div className="flex min-h-[calc(100vh-4rem)]">
        <DocsSidebar
          sections={sidebarData}
          mobileOpen={mobileOpen}
          onMobileClose={() => setMobileOpen(false)}
        />
        <main className="min-w-0 flex-1">{children}</main>
      </div>
    </div>
  );
}
