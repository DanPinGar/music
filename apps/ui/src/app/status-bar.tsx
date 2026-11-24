import { expectData } from "@segno/api-client";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useApi } from "@/api/api-context";

/** Bottom bar: connection to the core, its version and where the library lives. */
export function StatusBar() {
  const { t } = useTranslation();
  const api = useApi();
  const system = useQuery({
    queryKey: ["system"],
    queryFn: async () => expectData(await api.GET("/system")),
  });

  return (
    <footer className="flex h-7 shrink-0 items-center gap-3 border-t px-4 text-xs text-muted-foreground">
      {system.isPending && <span>{t("status.connecting")}</span>}
      {system.isError && (
        <span className="flex items-center gap-2 text-destructive">
          <span className="size-1.5 rounded-full bg-destructive" />
          {t("status.disconnected")}
        </span>
      )}
      {system.isSuccess && (
        <>
          <span className="flex items-center gap-2">
            <span className="size-1.5 rounded-full bg-emerald-500" />
            {t("status.connected", { version: system.data.version })}
          </span>
          {system.data.library_path && (
            <span className="ml-auto truncate" title={system.data.library_path}>
              {t("status.library", { path: system.data.library_path })}
            </span>
          )}
        </>
      )}
    </footer>
  );
}
