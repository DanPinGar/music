import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils";
import { linkTime, remarkTimes } from "./remark-times";

const PLUGINS = [remarkGfm, remarkTimes];

/**
 * A comment's Markdown. Times written in it ("1:23.5") jump there when clicked. Other links
 * are shown but not followed: the app's window must never navigate away. Raw HTML is never
 * rendered (react-markdown escapes it).
 */
export function CommentBody({
  body,
  onTime,
  className,
}: {
  body: string;
  onTime: (time: number) => void;
  className?: string;
}) {
  const components: Components = {
    a: ({ href, children }) => {
      const time = linkTime(href);
      if (time === null) {
        return (
          <span className="underline decoration-dotted" title={href}>
            {children}
          </span>
        );
      }
      return (
        <button
          type="button"
          className="rounded px-0.5 font-mono text-violet-300 underline decoration-violet-400/50 hover:bg-violet-500/15"
          onClick={(event) => {
            event.currentTarget.blur();
            onTime(time);
          }}
          data-testid="comment-time"
        >
          {children}
        </button>
      );
    },
  };
  return (
    <div
      className={cn(
        "text-sm leading-relaxed break-words",
        "[&_blockquote]:border-l-2 [&_blockquote]:pl-2 [&_blockquote]:text-muted-foreground",
        "[&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:text-xs",
        "[&_h1]:font-semibold [&_h2]:font-semibold [&_h3]:font-semibold",
        "[&_ol]:list-decimal [&_ol]:pl-5 [&_ul]:list-disc [&_ul]:pl-5",
        "[&_p+p]:mt-1.5 [&_strong]:font-semibold",
        className,
      )}
    >
      <Markdown remarkPlugins={PLUGINS} components={components}>
        {body}
      </Markdown>
    </div>
  );
}
