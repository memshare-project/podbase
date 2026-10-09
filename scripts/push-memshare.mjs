#!/usr/bin/env node
/**
 * 把 podbase 拉好的 catalog.json 推给 Memshare。
 * 一次请求会先清空分类、节目、单集，再插入这份数据。
 *
 *   MEMSHARE_API_URL=http://127.0.0.1:8080 \
 *   WORKER_API_KEY=... \
 *   node scripts/push-memshare.mjs [--catalog data/catalog.json] [--show <id>]
 */

import { readFile } from "node:fs/promises";
import path from "node:path";

const args = process.argv.slice(2);

function flag(name, fallback) {
  const index = args.indexOf(name);
  if (index === -1) return fallback;
  const value = args[index + 1];
  if (!value || value.startsWith("--")) {
    throw new Error(`${name} 需要一个参数`);
  }
  return value;
}

const catalogPath = path.resolve(flag("--catalog", "data/catalog.json"));
const onlyShow = flag("--show", "");
const baseURL = (process.env.MEMSHARE_API_URL || "http://127.0.0.1:8080").replace(/\/$/, "");
const apiKey = process.env.WORKER_API_KEY || "";

if (!apiKey) {
  console.error("需要环境变量 WORKER_API_KEY");
  process.exit(1);
}

const catalog = JSON.parse(await readFile(catalogPath, "utf8"));
const shows = Array.isArray(catalog.shows) ? catalog.shows : [];
const selected = onlyShow ? shows.filter((show) => show.id === onlyShow) : shows;

if (onlyShow && selected.length === 0) {
  console.error(`catalog 里没有节目 ${onlyShow}`);
  process.exit(1);
}

const categories = Array.isArray(catalog.categories) ? catalog.categories : [];
const showsToSend = [];
let fail = 0;

for (const show of selected) {
  if (show.error) {
    fail += 1;
    console.error(`✗ ${show.id}: 拉取失败，不写入（${show.error}）`);
    continue;
  }
  showsToSend.push(show);
}

if (showsToSend.length === 0) {
  console.error("没有可写入的节目，未改动远端数据");
  process.exit(1);
}

console.log(`替换远端目录：分类 ${categories.length}，节目 ${showsToSend.length}`);

const res = await fetch(`${baseURL}/internal/podcasts/sync`, {
  method: "POST",
  headers: {
    "content-type": "application/json",
    "x-worker-key": apiKey,
  },
  body: JSON.stringify({ categories, shows: showsToSend }),
});

const text = await res.text();
let body = null;
try {
  body = JSON.parse(text);
} catch {
  body = null;
}

if (!res.ok && !Array.isArray(body?.results)) {
  console.error(body?.error || text || `HTTP ${res.status}`);
  process.exit(1);
}

let ok = 0;
for (const result of body?.results ?? []) {
  if (result.ok) {
    ok += 1;
    console.log(`✓ ${result.showId} (${result.title}): inserted=${result.upserted}`);
  } else {
    fail += 1;
    console.error(`✗ ${result.showId}: ${result.error}`);
  }
}

console.log(`Done. ok=${ok} fail=${fail}`);
if (fail > 0) process.exit(1);
