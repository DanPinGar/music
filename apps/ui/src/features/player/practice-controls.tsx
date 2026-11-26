import { DrumIcon, RepeatIcon } from "lucide-react";
import type { MouseEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useStore } from "zustand";
import { useCommandRegistry } from "@/commands/commands-context";
import { Shortcut } from "@/commands/shortcut";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTitle, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { barBeat, tempoAt } from "./beat-grid";
import { DEFAULT_TRAINER, type SpeedTrainer } from "./loop";
import { formatTimecode } from "./navigation";
import type { PlayerRuntime } from "./player-runtime";

const CHIP =
  "h-7 gap-1.5 rounded-md px-2 font-mono text-xs tabular-nums text-muted-foreground hover:text-foreground";
const PRE_ROLLS = [0, 1, 2, 4] as const;
const SPEEDS = [0.5, 0.6, 0.7, 0.75, 0.8, 0.9, 1] as const;
const STEPS = [0.02, 0.05, 0.1] as const;

/** Runs a command from a button and gives the focus back to the player. */
function useRun() {
  const registry = useCommandRegistry();
  return (command: string) => (event: MouseEvent<HTMLButtonElement>) => {
    event.currentTarget.blur();
    registry.execute(command);
  };
}

function Choice<T extends number>({
  values,
  value,
  format,
  onChange,
  label,
}: {
  values: readonly T[];
  value: T;
  format: (value: T) => string;
  onChange: (value: T) => void;
  label: string;
}) {
  return (
    <fieldset className="flex items-center justify-between gap-2">
      <legend className="sr-only">{label}</legend>
      <span className="text-xs text-muted-foreground">{label}</span>
      <div className="flex gap-0.5">
        {values.map((option) => (
          <Button
            key={option}
            size="xs"
            variant={option === value ? "secondary" : "ghost"}
            onClick={() => onChange(option)}
            aria-pressed={option === value}
          >
            {format(option)}
          </Button>
        ))}
      </div>
    </fieldset>
  );
}

function CommandRow({
  command,
  binding,
  children,
}: {
  command: string;
  binding?: string;
  children: ReactNode;
}) {
  const run = useRun();
  return (
    <Button
      variant="ghost"
      size="sm"
      className="h-auto min-h-7 w-full justify-between py-1 text-left whitespace-normal"
      onClick={run(command)}
    >
      {children}
      {binding && <Shortcut binding={binding} />}
    </Button>
  );
}

/** Bar and beat of the playhead and the tempo there, from the tapped beat grid. */
export function GridControl({ runtime, time }: { runtime: PlayerRuntime; time: number }) {
  const { t } = useTranslation();
  const grid = useStore(runtime.session, (state) => state.grid);
  let summary = t("grid.none");
  if (grid) {
    const { bar, beat } = barBeat(grid, time);
    summary = `${t("grid.position", { bar, beat })} · ♩${Math.round(tempoAt(grid, time))}`;
  }
  return (
    <Popover>
      <PopoverTrigger
        render={<Button variant="ghost" className={CHIP} data-testid="grid-position" />}
      >
        <DrumIcon />
        {summary}
      </PopoverTrigger>
      <PopoverContent className="w-80" align="start" side="top" finalFocus={false}>
        <PopoverTitle>{t("grid.title")}</PopoverTitle>
        <p className="text-xs text-muted-foreground">{t("grid.help")}</p>
        <CommandRow command="grid.tap" binding="T">
          {t("commands.tapTempo")}
        </CommandRow>
        {grid && (
          <>
            <Choice
              label={t("grid.beatsPerBar")}
              values={[2, 3, 4, 5, 6, 7] as const}
              value={grid.beatsPerBar as 2 | 3 | 4 | 5 | 6 | 7}
              format={String}
              onChange={(beatsPerBar) =>
                runtime.session.setState({ grid: { ...grid, beatsPerBar } })
              }
            />
            <CommandRow command="grid.downbeatHere">{t("commands.downbeatHere")}</CommandRow>
            <CommandRow command="grid.clear">{t("commands.clearGrid")}</CommandRow>
          </>
        )}
      </PopoverContent>
    </Popover>
  );
}

/** The loop and how it is practised: A-B, pre-roll, count-in and speed trainer. */
export function LoopControl({ runtime }: { runtime: PlayerRuntime }) {
  const { t } = useTranslation();
  const registry = useCommandRegistry();
  const loop = useStore(runtime.session, (state) => state.loop);
  const practice = useStore(runtime.session, (state) => state.practice);
  const repetitions = useStore(runtime.session, (state) => state.repetitions);
  const countingIn = useStore(runtime.session, (state) => state.countingIn);
  const setPractice = (changes: Partial<typeof practice>) =>
    runtime.session.setState({ practice: { ...practice, ...changes } });
  const setTrainer = (changes: Partial<SpeedTrainer>) => {
    const trainer = { ...(practice.trainer ?? DEFAULT_TRAINER), ...changes };
    if (trainer.to < trainer.from) trainer.to = trainer.from;
    setPractice({ trainer });
  };

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            variant="ghost"
            className={cn(CHIP, loop?.enabled && "bg-sky-500/15 text-sky-300")}
            data-testid="loop-state"
          />
        }
      >
        <RepeatIcon />
        {countingIn
          ? t("loop.countingIn")
          : loop
            ? `${formatTimecode(loop.start, true)}–${formatTimecode(loop.end, true)}${loop.enabled ? ` ×${repetitions + 1}` : ""}`
            : t("loop.none")}
      </PopoverTrigger>
      <PopoverContent className="w-[28rem]" align="start" side="top" finalFocus={false}>
        <PopoverTitle>{t("loop.title")}</PopoverTitle>
        <p className="text-xs text-muted-foreground">{t("loop.help")}</p>
        <div className="grid grid-cols-2 gap-1">
          <CommandRow command="loop.setStart" binding="A">
            {t("commands.loopStart")}
          </CommandRow>
          <CommandRow command="loop.setEnd" binding="B">
            {t("commands.loopEnd")}
          </CommandRow>
          <CommandRow command="loop.toggle" binding="R">
            {loop?.enabled ? t("loop.turnOff") : t("loop.turnOn")}
          </CommandRow>
          <CommandRow command="loop.clear" binding="Shift+R">
            {t("commands.clearLoop")}
          </CommandRow>
          <CommandRow command="loop.save">{t("commands.saveLoop")}</CommandRow>
          <CommandRow command="loop.toSection">{t("commands.loopToSection")}</CommandRow>
        </div>
        <Choice
          label={t("loop.preRoll")}
          values={PRE_ROLLS}
          value={
            (PRE_ROLLS.find((beats) => beats === practice.preRollBeats) ??
              0) as (typeof PRE_ROLLS)[number]
          }
          format={(beats) => (beats === 0 ? t("loop.off") : t("loop.beats", { count: beats }))}
          onChange={(preRollBeats) => setPractice({ preRollBeats })}
        />
        <label className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
          {t("loop.countIn")}
          <input
            type="checkbox"
            className="size-4 accent-primary"
            checked={practice.countIn}
            onChange={(event) => setPractice({ countIn: event.currentTarget.checked })}
          />
        </label>
        <label className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
          {t("loop.trainer")}
          <input
            type="checkbox"
            className="size-4 accent-primary"
            checked={practice.trainer !== null}
            data-testid="trainer-toggle"
            onChange={() => registry.execute("loop.toggleTrainer")}
          />
        </label>
        {practice.trainer && (
          <div className="flex flex-col gap-1.5 rounded-md bg-muted/40 p-2">
            <Choice
              label={t("loop.trainerFrom")}
              values={SPEEDS}
              value={closest(SPEEDS, practice.trainer.from)}
              format={percent}
              onChange={(from) => setTrainer({ from })}
            />
            <Choice
              label={t("loop.trainerStep")}
              values={STEPS}
              value={closest(STEPS, practice.trainer.step)}
              format={(step) => `+${Math.round(step * 100)}`}
              onChange={(step) => setTrainer({ step })}
            />
            <Choice
              label={t("loop.trainerTo")}
              values={SPEEDS}
              value={closest(SPEEDS, practice.trainer.to)}
              format={percent}
              onChange={(to) => setTrainer({ to })}
            />
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}

function percent(speed: number): string {
  return `${Math.round(speed * 100)}%`;
}

function closest<T extends number>(values: readonly T[], value: number): T {
  return values.reduce((best, option) =>
    Math.abs(option - value) < Math.abs(best - value) ? option : best,
  );
}
