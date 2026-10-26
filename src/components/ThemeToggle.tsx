import { useRef } from "react";
import { Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useTheme } from "@/components/theme-provider";
import type { ThemeOrigin } from "@/lib/theme-transition";

export function ThemeToggle() {
  const { resolvedTheme, setTheme } = useTheme();
  const press = useRef<ThemeOrigin>();
  const dark = resolvedTheme === "dark";
  const label = dark ? "切换到白天模式" : "切换到黑夜模式";

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      aria-label={label}
      title={label}
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
        setTheme(dark ? "light" : "dark", origin);
      }}
    >
      {dark ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
    </Button>
  );
}
