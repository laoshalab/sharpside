import { getLocale } from "next-intl/server";
import { getSidebarData } from "@/lib/docs";
import { DocsShell } from "@/components/docs/DocsShell";

export default async function DocsLayout({ children }: { children: React.ReactNode }) {
  const locale = await getLocale();
  const sidebarData = getSidebarData(locale);
  return <DocsShell sidebarData={sidebarData}>{children}</DocsShell>;
}
