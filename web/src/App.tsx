import { useEffect, useMemo, useRef, useState } from "react";
import { catalogUrl, formatDate, formatDuration, LEVEL, PLATFORM } from "./format.ts";
import type { Catalog, Episode, Show } from "./types.ts";

type NowPlaying = { show: Show; episode: Episode };

function useHashId() {
  const read = () => location.hash.replace(/^#/, "");
  const [id, setId] = useState(read);
  useEffect(() => {
    const sync = () => setId(read());
    window.addEventListener("hashchange", sync);
    return () => window.removeEventListener("hashchange", sync);
  }, []);
  return id;
}

function openShow(id: string) {
  location.hash = id;
}

function closeShow() {
  history.pushState(null, "", location.pathname + location.search);
  window.dispatchEvent(new HashChangeEvent("hashchange"));
}

function Cover({ show }: { show: Show }) {
  const [failed, setFailed] = useState(false);
  return (
    <div className="cover" style={{ background: show.coverColor || "#c4b6a6" }}>
      {show.coverLabel}
      {show.coverImageUrl && !failed ? (
        <img
          alt=""
          referrerPolicy="no-referrer"
          src={show.coverImageUrl}
          onError={() => setFailed(true)}
        />
      ) : null}
    </div>
  );
}

function App() {
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [error, setError] = useState("");
  const [category, setCategory] = useState("all");
  const [query, setQuery] = useState("");
  const [scrolled, setScrolled] = useState(false);
  const [now, setNow] = useState<NowPlaying | null>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const showId = useHashId();

  useEffect(() => {
    const controller = new AbortController();
    fetch(catalogUrl(), { signal: controller.signal })
      .then((response) => {
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        return response.json() as Promise<Catalog>;
      })
      .then(setCatalog)
      .catch((err: unknown) => {
        if (err instanceof DOMException && err.name === "AbortError") return;
        setError("没有读到目录");
      });
    return () => controller.abort();
  }, []);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 4);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    document.body.classList.toggle("has-player", now !== null);
    const audio = audioRef.current;
    if (!now || !audio) return;
    audio.src = now.episode.audioUrl;
    void audio.play().catch(() => undefined);
  }, [now]);

  const shows = useMemo(() => {
    if (!catalog) return [];
    const needle = query.trim().toLowerCase();
    return catalog.shows.filter((show) => {
      if (category !== "all" && show.categoryId !== category) return false;
      if (!needle) return true;
      return [show.title, show.author, show.tagline, show.description]
        .join("\n")
        .toLowerCase()
        .includes(needle);
    });
  }, [catalog, category, query]);

  const active = catalog?.shows.find((show) => show.id === showId) ?? null;
  const episodeTotal = catalog?.shows.reduce((sum, show) => sum + show.episodes.length, 0) ?? 0;

  function play(show: Show, episode: Episode) {
    setNow({ show, episode });
  }

  const categories = catalog ? [{ id: "all", title: "全部" }, ...catalog.categories] : [];

  return (
    <>
      <header className={scrolled ? "scrolled" : undefined}>
        <div className="wrap">
          <div className="top">
            <div>
              <h1>日语播客</h1>
              <p className="meta">
                {catalog
                  ? `${catalog.shows.length} 个节目 · ${episodeTotal} 集 · 同步于 ${formatDate(catalog.syncedAt)}`
                  : error || "正在读取目录…"}
              </p>
            </div>
            <input
              className="search"
              type="search"
              placeholder="搜索节目"
              aria-label="搜索节目"
              value={query}
              onChange={(event) => {
                const next = event.target.value;
                setQuery(next);
                if (next.trim()) setCategory("all");
                if (location.hash) closeShow();
              }}
            />
          </div>
          <div className="filters">
            {categories.map((item) => {
              const count =
                item.id === "all"
                  ? catalog?.shows.length ?? 0
                  : catalog?.shows.filter((show) => show.categoryId === item.id).length ?? 0;
              return (
                <button
                  key={item.id}
                  className="chip"
                  type="button"
                  aria-pressed={category === item.id}
                  onClick={() => {
                    setCategory(item.id);
                    if (location.hash) closeShow();
                  }}
                >
                  {item.title}
                  <small>{count}</small>
                </button>
              );
            })}
          </div>
        </div>
      </header>
      <main>
        <div className="wrap">
          {error ? <p className="empty">{error}</p> : null}
          {catalog && active ? (
            <ShowDetail
              show={active}
              currentId={now?.episode.id}
              onBack={closeShow}
              onPlay={play}
            />
          ) : null}
          {catalog && !active ? (
            shows.length ? (
              <div className="grid">
                {shows.map((show) => (
                  <button key={show.id} className="card" type="button" onClick={() => openShow(show.id)}>
                    <Cover show={show} />
                    <div>
                      <h2>{show.title}</h2>
                      <p className="author">{show.author}</p>
                      <p className="tagline">{show.tagline}</p>
                      <p className="facts">
                        <span className={`pill ${show.level}`}>{LEVEL[show.level] ?? show.level}</span>
                        <span>{PLATFORM[show.platform] ?? show.platform}</span>
                        <span>{show.episodeCount} 集</span>
                      </p>
                    </div>
                  </button>
                ))}
              </div>
            ) : (
              <p className="empty">没有匹配的节目</p>
            )
          ) : null}
        </div>
      </main>
      <div className="player" hidden={now === null}>
        <p>
          <strong>{now?.episode.title}</strong>
          <span>{now?.show.title}</span>
        </p>
        <audio ref={audioRef} controls preload="none" />
      </div>
    </>
  );
}

function ShowDetail({
  show,
  currentId,
  onBack,
  onPlay,
}: {
  show: Show;
  currentId: string | undefined;
  onBack: () => void;
  onPlay: (show: Show, episode: Episode) => void;
}) {
  return (
    <>
      <button className="back" type="button" onClick={onBack}>
        ← 全部节目
      </button>
      <div className="detail-head">
        <Cover show={show} />
        <div className="show-copy">
          <h2>{show.title}</h2>
          <p className="author">
            {show.author} · {show.tagline}
          </p>
          <p className="facts">
            <span className={`pill ${show.level}`}>{LEVEL[show.level] ?? show.level}</span>
            <span>{PLATFORM[show.platform] ?? show.platform}</span>
          </p>
          {show.description ? <p>{show.description}</p> : null}
          {show.error ? <p>{show.error}</p> : null}
          <p>
            <a href={show.rssUrl} target="_blank" rel="noreferrer">
              RSS
            </a>
          </p>
        </div>
      </div>
      <div className="episodes">
        {show.episodes.map((episode) => {
          const when = [formatDate(episode.publishedAt), formatDuration(episode.durationSec)]
            .filter(Boolean)
            .join(" · ");
          return (
            <button
              key={episode.id}
              className={episode.id === currentId ? "episode current" : "episode"}
              type="button"
              onClick={() => onPlay(show, episode)}
            >
              <span className="num">{episode.number}</span>
              <span>
                <strong>{episode.title}</strong>
                {episode.description ? <p>{episode.description}</p> : null}
              </span>
              <span className="when">{when}</span>
            </button>
          );
        })}
      </div>
    </>
  );
}

export default App;
