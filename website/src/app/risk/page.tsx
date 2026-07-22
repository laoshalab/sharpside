"use client";

import { useTranslations } from "next-intl";
import { MarketingPage } from "@/components/layout/MarketingPage";

export default function RiskPage() {
  const t = useTranslations("pages.risk");

  return (
    <MarketingPage
      eyebrow={t("eyebrow")}
      title={t("title")}
      description={t("description")}
      bullets={t.raw("bullets") as string[]}
      primaryHref="/docs/technical/operations/shadow-mode"
      primaryLabel={t("primary")}
      secondaryHref="/docs"
      secondaryLabel={t("secondary")}
    />
  );
}
