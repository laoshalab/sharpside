"use client";

import { useTranslations } from "next-intl";
import { MarketingPage } from "@/components/layout/MarketingPage";

export default function VenuesPage() {
  const t = useTranslations("pages.venues");

  return (
    <MarketingPage
      eyebrow={t("eyebrow")}
      title={t("title")}
      description={t("description")}
      bullets={t.raw("bullets") as string[]}
      primaryHref="/docs/technical/architecture/venues"
      primaryLabel={t("primary")}
      secondaryHref="/docs/technical/architecture/overview"
      secondaryLabel={t("secondary")}
    />
  );
}
