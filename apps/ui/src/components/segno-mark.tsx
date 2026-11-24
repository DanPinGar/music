import type { SVGProps } from "react";

/**
 * The segno sign (𝄋), the app's mark: in sheet music it marks the point to jump back to.
 * Drawn on a 24×24 grid like the lucide icons, in the current text colour.
 */
export function SegnoMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      <path d="M16.5 6.8C15.6 5.4 14 4.6 12.2 4.6 9.7 4.6 7.8 6 7.8 8.1c0 2.3 2 3.1 4.4 3.9 2.5.8 4.3 1.7 4.3 4 0 2.1-1.9 3.5-4.4 3.5-1.9 0-3.5-.8-4.4-2.2" />
      <path d="M6.2 19.2 17.8 4.8" />
      <circle cx="6.6" cy="13.4" r="1" fill="currentColor" stroke="none" />
      <circle cx="17.4" cy="10.6" r="1" fill="currentColor" stroke="none" />
    </svg>
  );
}
