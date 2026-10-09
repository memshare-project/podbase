export type Level = "beginner" | "advanced" | "expert";

export type Category = {
  id: string;
  title: string;
  subtitle: string;
  sortOrder: number;
};

export type Episode = {
  id: string;
  showId: string;
  number: number;
  guid: string;
  title: string;
  description: string;
  audioUrl: string;
  imageUrl: string | null;
  durationSec: number;
  publishedAt: string | null;
};

export type Show = {
  id: string;
  categoryId: string;
  rssUrl: string;
  platform: string;
  title: string;
  author: string;
  coverColor: string;
  coverLabel: string;
  coverImageUrl: string | null;
  level: Level;
  tagline: string;
  description: string;
  episodeCount: number;
  sortOrder: number;
  lastSyncedAt: string | null;
  episodes: Episode[];
  error?: string;
};

export type Catalog = {
  language: string;
  syncedAt: string;
  categories: Category[];
  shows: Show[];
};
