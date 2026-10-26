import { useEffect, useState } from "react";
import { Download, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { settingsApi } from "@/lib/api";
import { checkAppUpdate, type AppUpdateInfo } from "@/lib/api/update";
import { extractErrorMessage } from "@/utils/errorUtils";

const DISMISSED_VERSION_KEY = "xinghuo:update:dismissed-version";

export function AppUpdateDialog() {
  const { t } = useTranslation();
  const [update, setUpdate] = useState<AppUpdateInfo | null>(null);
  const [open, setOpen] = useState(false);
  const [opening, setOpening] = useState(false);

  useEffect(() => {
    let active = true;
    const timer = window.setTimeout(async () => {
      try {
        const result = await checkAppUpdate();
        if (
          active &&
          result.available &&
          localStorage.getItem(DISMISSED_VERSION_KEY) !== result.latestVersion
        ) {
          setUpdate(result);
          setOpen(true);
        }
      } catch (error) {
        console.warn("[AppUpdateDialog] Background update check failed", error);
      }
    }, 1500);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, []);

  const dismiss = () => {
    if (update) {
      localStorage.setItem(DISMISSED_VERSION_KEY, update.latestVersion);
    }
    setOpen(false);
  };

  const download = async () => {
    if (!update || opening) return;
    setOpening(true);
    try {
      await settingsApi.openExternal(update.downloadUrl);
      setOpen(false);
    } catch (error) {
      toast.error(t("settings.appUpdateOpenFailed"), {
        description: extractErrorMessage(error),
        closeButton: true,
      });
    } finally {
      setOpening(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !next && dismiss()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("settings.appUpdateAvailable")}</DialogTitle>
          <DialogDescription>
            {update?.title || t("settings.appUpdateHint")}
            {update ? ` · ${update.source}` : ""}
          </DialogDescription>
        </DialogHeader>
        {update && (
          <div className="space-y-4 px-6">
            <div className="flex items-center justify-center gap-3 rounded-lg bg-muted/50 px-4 py-3 text-sm">
              <span>v{update.currentVersion}</span>
              <span aria-hidden="true">→</span>
              <span className="font-semibold text-primary">
                v{update.latestVersion}
              </span>
            </div>
            {update.notes && (
              <pre className="max-h-56 overflow-y-auto whitespace-pre-wrap rounded-lg border bg-background p-3 font-sans text-xs leading-relaxed text-muted-foreground">
                {update.notes}
              </pre>
            )}
          </div>
        )}
        <DialogFooter className="flex-row justify-end gap-2">
          <Button type="button" variant="ghost" onClick={dismiss}>
            {t("settings.appUpdateDismiss")}
          </Button>
          <Button type="button" onClick={download} disabled={opening}>
            {opening ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Download className="h-4 w-4" />
            )}
            {t("settings.appUpdateDownload")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
