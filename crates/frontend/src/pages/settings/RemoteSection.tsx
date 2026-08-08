import { Cloud, Server } from "../../components/icons";

import { useLocale } from "../../i18n";
import {
  clearStoredEndpoint,
  isDesktopShell,
  peekStoredEndpoint,
  resetEndpoint,
} from "../../lib/backend/endpoint";
import type { SettingsSectionProps } from "./types";

/**
 * 远程连接状态：显示当前连接的后端、一键切回本地。
 * 安全性来自 Bearer token + 密码强度，不靠 UI toggle。
 */
export function RemoteSection(_props: SettingsSectionProps) {
  const { t } = useLocale();
  const storedEndpoint = peekStoredEndpoint();
  const canReturnToLocal = storedEndpoint && isDesktopShell();

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-sky-500/10">
          <Cloud className="h-[18px] w-[18px] text-sky-500" />
        </div>
        <div>
          <h3 className="text-sm font-semibold">{t("settings.remoteTitle")}</h3>
          <p className="text-xs text-muted-foreground">{t("settings.remoteDesc")}</p>
        </div>
      </div>

      <div className="rounded-xl border border-border/60 bg-card p-5">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2 text-sm font-medium text-foreground">
              <Server className="h-4 w-4 text-muted-foreground" />
              {t("settings.backendServerTitle")}
            </div>
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {storedEndpoint
                ? `${storedEndpoint.host}:${storedEndpoint.port}`
                : t("settings.backendServerEmbedded")}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {canReturnToLocal ? (
              <button
                type="button"
                className="rounded-md border border-sky-500/30 bg-sky-500/10 px-3 py-1.5 text-xs text-sky-600 hover:bg-sky-500/20 dark:text-sky-400"
                onClick={() => {
                  clearStoredEndpoint();
                  resetEndpoint();
                  const url = new URL(window.location.href);
                  url.search = "";
                  window.location.assign(url);
                }}
              >
                {t("settings.backendServerReturnLocal")}
              </button>
            ) : null}
            <button
              type="button"
              className="rounded-md border border-border px-3 py-1.5 text-xs text-foreground hover:bg-muted"
              onClick={() => {
                const url = new URL(window.location.href);
                url.searchParams.set("connect", "1");
                window.location.assign(url);
              }}
            >
              {t("settings.backendServerChange")}
            </button>
          </div>
        </div>
        {storedEndpoint ? (
          <p className="mt-2 text-[11px] text-sky-600/80 dark:text-sky-400/80">
            {t("settings.connectionModeRemote")}
          </p>
        ) : null}
      </div>
    </div>
  );
}
