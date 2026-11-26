/** Times closer than this to the playhead count as where it already is. */
const SAME_TIME = 0.001;
/**
 * While playing, "previous attack" skips the attacks heard in the last quarter second, or
 * pressing it twice quickly would land on the same attack again.
 */
const PLAYING_GRACE = 0.25;

/** The first attack after `time`, if any. `times` must be in order. */
export function nextAttack(times: readonly number[], time: number): number | undefined {
  const index = firstAbove(times, time + SAME_TIME);
  return index < times.length ? times[index] : undefined;
}

/** The last attack before `time`, if any. `times` must be in order. */
export function previousAttack(
  times: readonly number[],
  time: number,
  playing = false,
): number | undefined {
  const index = firstAtOrAfter(times, time - (playing ? PLAYING_GRACE : SAME_TIME)) - 1;
  return index >= 0 ? times[index] : undefined;
}

/** Index of the first time at or after `limit` (`times.length` if none). */
export function firstAtOrAfter(times: readonly number[], limit: number): number {
  let low = 0;
  let high = times.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if ((times[middle] ?? Number.POSITIVE_INFINITY) >= limit) high = middle;
    else low = middle + 1;
  }
  return low;
}

/** Index of the first time strictly after `limit` (`times.length` if none). */
function firstAbove(times: readonly number[], limit: number): number {
  let low = 0;
  let high = times.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if ((times[middle] ?? Number.POSITIVE_INFINITY) > limit) high = middle;
    else low = middle + 1;
  }
  return low;
}
