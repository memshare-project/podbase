export const PLATFORM: Record<string, string> = {
  anchor: "Anchor",
  megaphone: "Megaphone",
  wordpress: "WordPress",
  libsyn: "Libsyn",
  nhk: "NHK",
  rss: "RSS",
};

export const LEVEL: Record<string, string> = {
  beginner: "入门",
  advanced: "进阶",
};

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return `${date.getFullYear()}年${date.getMonth() + 1}月${date.getDate()}日`;
}

export function formatDuration(sec: number): string {
  if (!sec) return "";
  const hours = Math.floor(sec / 3600);
  const minutes = Math.floor((sec % 3600) / 60);
  const seconds = sec % 60;
  const mm = String(minutes).padStart(2, "0");
  const ss = String(seconds).padStart(2, "0");
  return hours ? `${hours}:${mm}:${ss}` : `${minutes}:${ss}`;
}

export function catalogUrl(): string {
  return `${import.meta.env.BASE_URL}catalog.json`;
}
