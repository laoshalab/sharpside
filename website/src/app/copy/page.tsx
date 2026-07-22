"use client";

import { useTranslations } from "next-intl";
import { MarketingPage } from "@/components/layout/MarketingPage";

export default function CopyPage() {
  const t = useTranslations("pages.copy");

  return (
    <MarketingPage
      eyebrow={t("eyebrow")}
      title={t("title")}
      description={t("description")}
      bullets={t.raw("bullets") as string[]}
      primaryHref="/docs/guide/copy-trading"
      primaryLabel={t("primary")}
      secondaryHref="/docs"
      secondaryLabel={t("secondary")}
    />
  );
}
