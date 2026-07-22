"use client";

import { useTranslations } from "next-intl";
import { MarketingPage } from "@/components/layout/MarketingPage";

export default function ChannelsPage() {
  const t = useTranslations("pages.channels");

  return (
    <MarketingPage
      eyebrow={t("eyebrow")}
      title={t("title")}
      description={t("description")}
      bullets={t.raw("bullets") as string[]}
      primaryHref="/docs/guide/dual-channels"
      primaryLabel={t("primary")}
      secondaryHref="/docs/technical/operations/tg-bot"
      secondaryLabel={t("secondary")}
    />
  );
}
