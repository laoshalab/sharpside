"use client";

import { useTranslations } from "next-intl";
import { MarketingPage } from "@/components/layout/MarketingPage";

export default function TechPage() {
  const t = useTranslations("pages.tech");

  return (
    <MarketingPage
      eyebrow={t("eyebrow")}
      title={t("title")}
      description={t("description")}
      bullets={t.raw("bullets") as string[]}
      primaryHref="/docs/technical/getting-started/setup"
      primaryLabel={t("primary")}
      secondaryHref="/docs/technical/architecture/overview"
      secondaryLabel={t("secondary")}
    />
  );
}
