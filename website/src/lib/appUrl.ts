/** App CTA target. Never fall back to localhost in production builds. */
export function getAppHref(): string | null {
  const configured = process.env.NEXT_PUBLIC_APP_URL?.trim();
  if (configured) return configured;
  if (process.env.NODE_ENV !== "production") return "http://localhost:8080";
  return null;
}
