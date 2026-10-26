import { Monitor, Moon, Sun } from "lucide-react";
import { useRef } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useTranslation } from "react-i18next";
import { useTheme } from "@/components/theme-provider";
import type { ThemeOrigin } from "@/lib/theme-transition";

export function ThemeSettings() {
  const { t } = useTranslation();
  const { theme, setTheme } = useTheme();

  return (
    <section className="space-y-2">
      <header className="space-y-1">
        <h3 className="text-sm font-medium">{t("settings.theme")}</h3>
        <p className="text-xs text-muted-foreground">
          {t("settings.themeHint")}
        </p>
      </header>
      <div className="inline-flex gap-1 rounded-md border border-border-default bg-background p-1">
        <ThemeButton
          active={theme === "light"}
          onClick={(origin) => setTheme("light", origin)}
          icon={Sun}
        >
          {t("settings.themeLight")}
        </ThemeButton>
        <ThemeButton
          active={theme === "dark"}
          onClick={(origin) => setTheme("dark", origin)}
          icon={Moon}
        >
          {t("settings.themeDark")}
        </ThemeButton>
        <ThemeButton
          active={theme === "system"}
          onClick={(origin) => setTheme("system", origin)}
          icon={Monitor}
        >
          {t("settings.themeSystem")}
        </ThemeButton>
      </div>
    </section>
  );
}

interface ThemeButtonProps {
  active: boolean;
  onClick: (origin: ThemeOrigin) => void;
  icon: React.ComponentType<{ className?: string }>;
  children: React.ReactNode;
}

function ThemeButton({
  active,
  onClick,
  icon: Icon,
  children,
}: ThemeButtonProps) {
  const press = useRef<ThemeOrigin>();
  return (
    <Button
      type="button"
      onPointerDown={(event) => {
        if (event.button === 0)
          press.current = { x: event.clientX, y: event.clientY };
      }}
      onPointerCancel={() => {
        press.current = undefined;
      }}
      onClick={(event) => {
        const rect = event.currentTarget.getBoundingClientRect();
        const origin =
          event.detail === 0
            ? { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
            : (press.current ?? { x: event.clientX, y: event.clientY });
        press.current = undefined;
        onClick(origin);
      }}
      size="sm"
      variant={active ? "default" : "ghost"}
      className={cn(
        "min-w-[96px] gap-1.5",
        active
          ? "shadow-sm"
          : "text-muted-foreground hover:text-foreground hover:bg-muted",
      )}
    >
      <Icon className="h-3.5 w-3.5" />
      {children}
    </Button>
  );
}
