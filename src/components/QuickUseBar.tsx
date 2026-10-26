import { cloneElement, isValidElement, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import type { AppId } from "@/lib/api";
import { APP_IDS, APP_ICON_MAP } from "@/config/appConfig";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface QuickUseBarProps {
  activeApp: AppId;
  onSelect: (app: AppId) => void;
  disabled?: boolean;
  /** 收进「更多平台」折叠区的扩展平台，默认收起。 */
  moreApps?: AppId[];
}

/** 只展示已支持自动接入的工具。 */
export function QuickUseBar({
  activeApp,
  onSelect,
  disabled = false,
  moreApps,
}: QuickUseBarProps) {
  const { t } = useTranslation();
  const [moreOpen, setMoreOpen] = useState(false);

  const renderAppButton = (app: AppId) => {
    const config = APP_ICON_MAP[app];
    const isActive = activeApp === app;
    return (
      <Button
        key={app}
        type="button"
        variant={isActive ? "outline" : "ghost"}
        className={cn(
          "h-20 min-w-0 justify-start gap-3 rounded-xl border border-border/60 bg-background/40 px-4 text-sm",
          isActive &&
            "border-primary/50 bg-primary/10 text-primary ring-1 ring-primary/20 hover:border-primary/60 hover:bg-primary/15 hover:text-primary",
        )}
        disabled={disabled}
        role="tab"
        aria-selected={isActive}
        onClick={() => onSelect(app)}
        title={t("quickUse.useApp", {
          defaultValue: "选择 {{app}}",
          app: config.label,
        })}
        aria-label={t("quickUse.useApp", {
          defaultValue: "选择 {{app}}",
          app: config.label,
        })}
      >
        <span
          aria-hidden="true"
          className="flex h-9 w-9 shrink-0 items-center justify-center"
        >
          {isValidElement<{ size?: number }>(config.icon)
            ? cloneElement(config.icon, { size: 36 })
            : config.icon}
        </span>
        <span className="min-w-0 whitespace-normal text-left leading-snug">
          {config.label}
        </span>
      </Button>
    );
  };

  return (
    <div>
      <div
        role="tablist"
        aria-label="选择工具"
        className="grid grid-cols-1 gap-3 min-[420px]:grid-cols-2"
      >
        {APP_IDS.map(renderAppButton)}
      </div>
      {moreApps && moreApps.length > 0 && (
        <div className="mt-3">
          <button
            type="button"
            className="flex w-full items-center justify-center gap-1.5 rounded-lg py-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
            aria-expanded={moreOpen}
            disabled={disabled}
            onClick={() => setMoreOpen((open) => !open)}
          >
            <ChevronDown
              className={cn(
                "h-3.5 w-3.5 transition-transform",
                moreOpen && "rotate-180",
              )}
            />
            更多平台
          </button>
          {moreOpen && (
            <div
              role="tablist"
              aria-label="更多平台"
              className="mt-2 grid grid-cols-1 gap-3 min-[420px]:grid-cols-2"
            >
              {moreApps.map(renderAppButton)}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
