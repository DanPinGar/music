import { assetUrl, type Video } from "@segno/api-client";
import { useApiConfig } from "@/api/api-context";
import { SegnoMark } from "@/components/segno-mark";

/** Where tile `index` sits in the sprite, as CSS background percentages. */
export function tilePosition(index: number, columns: number, rows: number): string {
  const column = index % columns;
  const row = Math.floor(index / columns);
  const x = columns > 1 ? (column / (columns - 1)) * 100 : 0;
  const y = rows > 1 ? (row / (rows - 1)) * 100 : 0;
  return `${x}% ${y}%`;
}

/**
 * One frame of the video from its thumbnail sprite: a tenth of the way in, past the black
 * intros most videos start with. A placeholder until the sprite exists.
 */
export function VideoThumbnail({ video }: { video: Video }) {
  const config = useApiConfig();
  const sprite = video.thumbnails;
  if (!sprite) {
    return (
      <div className="flex aspect-video w-full items-center justify-center bg-muted">
        <SegnoMark className="size-8 text-muted-foreground/50" />
      </div>
    );
  }
  const index = Math.min(sprite.count - 1, Math.floor(sprite.count / 10));
  return (
    <div
      className="w-full bg-black bg-no-repeat"
      style={{
        aspectRatio: `${sprite.tile_width} / ${sprite.tile_height}`,
        backgroundImage: `url("${assetUrl(config, video.id, "thumbnails")}")`,
        backgroundSize: `${sprite.columns * 100}% ${sprite.rows * 100}%`,
        backgroundPosition: tilePosition(index, sprite.columns, sprite.rows),
      }}
    />
  );
}
