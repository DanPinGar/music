import { expectData, type Video } from "@segno/api-client";
import { useQuery } from "@tanstack/react-query";
import { DownloadIcon, FilePlusIcon, SearchIcon, XIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/api/api-context";
import {
  dialogOpen,
  isTypingTarget,
  useCommandRegistry,
  useCommands,
} from "@/commands/commands-context";
import type { Command } from "@/commands/registry";
import { Shortcut } from "@/commands/shortcut";
import { SegnoMark } from "@/components/segno-mark";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import {
  allTags,
  DEFAULT_ORDER,
  filterVideos,
  LIBRARY_ORDERS,
  type LibraryOrder,
  sortVideos,
  youtubeId,
} from "./library-filters";
import { useImportVideos } from "./use-import-videos";
import { useLibraryActions } from "./use-library-actions";
import { VideoRow } from "./video-row";

const ORDER_KEY = "segno.library.order";

function storedOrder(): LibraryOrder {
  try {
    const stored = localStorage.getItem(ORDER_KEY);
    return LIBRARY_ORDERS.find((order) => order === stored) ?? DEFAULT_ORDER;
  } catch {
    return DEFAULT_ORDER;
  }
}

function storeOrder(order: LibraryOrder): void {
  try {
    localStorage.setItem(ORDER_KEY, order);
  } catch {
    // Storage unavailable: the library opens in the default order.
  }
}

/** Start screen: the videos downloaded or imported so far. */
export function LibraryPage() {
  const { t } = useTranslation();
  const api = useApi();
  const registry = useCommandRegistry();
  const { pickAndImport } = useImportVideos();
  const { download } = useLibraryActions();
  const videos = useQuery({
    queryKey: ["videos"],
    queryFn: async () => expectData(await api.GET("/videos")),
    // Fresh on every visit: studying moves each video's position, and the core adds videos.
    staleTime: 0,
    // Follow downloads and preparations while any is running.
    refetchInterval: (query) =>
      query.state.data?.some((video) => video.preparation || video.download) ? 1_000 : false,
  });
  const [query, setQuery] = useState("");
  const [tag, setTag] = useState<string | null>(null);
  const [order, setOrder] = useState(storedOrder);
  const search = useRef<HTMLInputElement>(null);

  // A YouTube link pasted anywhere in the library (outside text fields) is downloaded.
  useEffect(() => {
    function onPaste(event: ClipboardEvent) {
      if (isTypingTarget(event.target) || dialogOpen()) return;
      const text = event.clipboardData?.getData("text/plain") ?? "";
      if (youtubeId(text) === null) return;
      event.preventDefault();
      void download(text.trim());
    }
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [download]);

  const commands = useMemo<Command[]>(
    () => [
      {
        id: "library.search",
        title: "commands.searchLibrary",
        group: "general",
        keys: ["Ctrl+F", "/"],
        run: () => search.current?.focus(),
      },
    ],
    [],
  );
  useCommands(commands);

  const all = videos.data ?? [];
  const tags = useMemo(() => allTags(all), [all]);
  const shown = useMemo(
    () => sortVideos(filterVideos(all, query, tag), order),
    [all, query, tag, order],
  );
  const filtering = query.trim() !== "" || tag !== null;
  const clearFilters = () => {
    setQuery("");
    setTag(null);
  };

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-6 p-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="font-heading text-2xl font-semibold">{t("library.title")}</h1>
          <p className="text-sm text-muted-foreground">{t("library.subtitle")}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={() => void pickAndImport()}>
            <FilePlusIcon />
            {t("library.import")}
            <Shortcut binding="Ctrl+O" className="ml-1" />
          </Button>
          <Button onClick={() => registry.execute("library.download")} data-testid="download">
            <DownloadIcon />
            {t("library.download")}
            <Shortcut binding="Ctrl+D" className="ml-1" />
          </Button>
        </div>
      </div>

      {videos.isPending && <p className="text-sm text-muted-foreground">{t("common.loading")}</p>}

      {videos.isError && (
        <Card>
          <CardHeader>
            <CardTitle>{t("library.error")}</CardTitle>
            <CardDescription>{videos.error.message}</CardDescription>
          </CardHeader>
          <CardContent>
            <Button variant="outline" onClick={() => void videos.refetch()}>
              {t("common.retry")}
            </Button>
          </CardContent>
        </Card>
      )}

      {videos.isSuccess && all.length === 0 && <EmptyLibrary />}
      {videos.isSuccess && all.length > 0 && (
        <>
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center gap-2">
              <div className="relative min-w-60 flex-1">
                <SearchIcon className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  ref={search}
                  type="search"
                  value={query}
                  onChange={(event) => setQuery(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape") {
                      if (query !== "") setQuery("");
                      else event.currentTarget.blur();
                    }
                  }}
                  placeholder={t("library.search")}
                  aria-label={t("library.search")}
                  className="pl-8"
                  data-testid="library-search"
                />
              </div>
              <OrderMenu
                order={order}
                onChange={(next) => {
                  setOrder(next);
                  storeOrder(next);
                }}
              />
              {filtering && (
                <span className="text-xs text-muted-foreground">
                  {t("library.count", { shown: shown.length, total: all.length })}
                </span>
              )}
            </div>
            {tags.length > 0 && <TagFilter tags={tags} active={tag} onChange={setTag} />}
          </div>
          {shown.length > 0 ? (
            <VideoList videos={shown} />
          ) : (
            <div className="flex flex-col items-center gap-3 py-12 text-sm text-muted-foreground">
              {t("library.noMatches")}
              <Button variant="outline" size="sm" onClick={clearFilters}>
                <XIcon />
                {t("library.clearFilters")}
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

function EmptyLibrary() {
  const { t } = useTranslation();
  return (
    <Card className="items-center py-12 text-center">
      <SegnoMark className="size-12 text-muted-foreground" />
      <CardHeader className="w-full max-w-md">
        <CardTitle>{t("library.emptyTitle")}</CardTitle>
        <CardDescription>
          {t("library.emptyBody")} <Shortcut binding="Ctrl+O" className="align-middle" />;{" "}
          {t("library.emptyDownload")} <Shortcut binding="Ctrl+D" className="align-middle" />
        </CardDescription>
      </CardHeader>
    </Card>
  );
}

function OrderMenu({
  order,
  onChange,
}: {
  order: LibraryOrder;
  onChange: (order: LibraryOrder) => void;
}) {
  const { t } = useTranslation();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button variant="outline" aria-label={t("library.sort")} data-testid="sort" />}
      >
        {t(`library.orders.${order}`)}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-auto">
        <DropdownMenuRadioGroup
          value={order}
          onValueChange={(value) => onChange(value as LibraryOrder)}
        >
          {LIBRARY_ORDERS.map((option) => (
            <DropdownMenuRadioItem key={option} value={option}>
              {t(`library.orders.${option}`)}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function TagFilter({
  tags,
  active,
  onChange,
}: {
  tags: string[];
  active: string | null;
  onChange: (tag: string | null) => void;
}) {
  const { t } = useTranslation();
  const chip = (selected: boolean) =>
    cn(
      "rounded-full border px-2.5 py-0.5 text-xs transition-colors hover:bg-muted",
      selected && "border-primary bg-primary/10 text-foreground",
    );
  return (
    <div className="flex flex-wrap gap-1.5" data-testid="tag-filter">
      <button type="button" className={chip(active === null)} onClick={() => onChange(null)}>
        {t("library.allTags")}
      </button>
      {tags.map((tag) => {
        const selected = active?.toLowerCase() === tag.toLowerCase();
        return (
          <button
            key={tag}
            type="button"
            className={chip(selected)}
            aria-pressed={selected}
            onClick={() => onChange(selected ? null : tag)}
          >
            {tag}
          </button>
        );
      })}
    </div>
  );
}

/** The videos one under another, in the order chosen. */
function VideoList({ videos }: { videos: Video[] }) {
  return (
    <ul className="flex flex-col divide-y overflow-hidden rounded-xl bg-card ring-1 ring-foreground/10">
      {videos.map((video) => (
        <li key={video.id}>
          <VideoRow video={video} />
        </li>
      ))}
    </ul>
  );
}
