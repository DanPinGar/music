import { Navigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { useSystemInfo } from "@/api/use-system-info";
import { SegnoMark } from "@/components/segno-mark";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { lastVideoId } from "./last-video";

/** The "Player" tab: reopens the last video studied, or explains how to open one. */
export function PlayerPage() {
  const { t } = useTranslation();
  const library = useSystemInfo().data?.library_id;
  const last = library ? lastVideoId(library) : null;
  if (last) return <Navigate to="/videos/$videoId" params={{ videoId: last }} replace />;

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-6 p-6">
      <h1 className="font-heading text-2xl font-semibold">{t("player.title")}</h1>
      <Card className="items-center py-12 text-center">
        <SegnoMark className="size-12 text-muted-foreground" />
        <CardHeader className="w-full max-w-md">
          <CardTitle>{t("player.title")}</CardTitle>
          <CardDescription>{t("player.empty")}</CardDescription>
        </CardHeader>
      </Card>
    </div>
  );
}
