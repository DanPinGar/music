import { useTranslation } from "react-i18next";
import { SegnoMark } from "@/components/segno-mark";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

/** The study player. Phase 1A fills it with the video, timeline and navigation layer. */
export function PlayerPage() {
  const { t } = useTranslation();
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
