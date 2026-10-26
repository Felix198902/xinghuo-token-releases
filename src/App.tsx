import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  Settings,
  Plus,
  Download,
  Minus,
  Maximize2,
  X,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "sonner";
import { ActivationPanel } from "@/components/activation/ActivationPanel";
import { AppUpdateDialog } from "@/components/AppUpdateDialog";
import { SettingsPage } from "@/components/settings/SettingsPage";
import UnifiedMcpPanel, {
  type UnifiedMcpPanelHandle,
} from "@/components/mcp/UnifiedMcpPanel";
import UnifiedSkillsPanel, {
  type UnifiedSkillsPanelHandle,
} from "@/components/skills/UnifiedSkillsPanel";
import {
  SkillsPage,
  type SkillsPageHandle,
} from "@/components/skills/SkillsPage";
import PromptPanel, {
  type PromptPanelHandle,
} from "@/components/prompts/PromptPanel";
import { SessionManagerPage } from "@/components/sessions/SessionManagerPage";
import { Button } from "@/components/ui/button";
import { ThemeToggle } from "@/components/ThemeToggle";
import { APP_IDS, APP_ICON_MAP } from "@/config/appConfig";
import { BRAND } from "@/config/brand";
import { useSettingsQuery } from "@/lib/query";
import {
  useXinghuoActivationQuery,
  useXinghuoStatusPolling,
} from "@/lib/query/xinghuo";
import type { AppId } from "@/lib/api";
import { isLinux, isWindows } from "@/lib/platform";

type View =
  | "home"
  | "settings"
  | "mcp"
  | "skills"
  | "discovery"
  | "prompts"
  | "sessions";

/** 固定星火服务入口，不再挂载供应商列表、配置编辑器或导入弹窗。 */
function App() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const syncStatusChanged = async (
    payload:
      | { source?: string; status?: string; error?: string }
      | null
      | undefined,
  ) => {
    await queryClient.invalidateQueries({ queryKey: ["settings"] });
    if (payload?.source === "auto" && payload.status === "error")
      toast.error(`自动同步失败：${payload.error || t("common.unknown")}`);
  };
  useTauriEvent("webdav-sync-status-updated", syncStatusChanged);
  useTauriEvent("s3-sync-status-updated", syncStatusChanged);
  const [view, setView] = useState<View>("home");
  const [busy, setBusy] = useState(false);
  const [skillsUpdates, setSkillsUpdates] = useState({
    isChecking: false,
    hasSkills: false,
  });
  const [app, setApp] = useState<AppId>(() => {
    const saved = localStorage.getItem("xinghuo-token-last-app") as AppId;
    return APP_IDS.includes(saved) ? saved : "claude";
  });
  const { data: settings } = useSettingsQuery();
  const { data: activation } = useXinghuoActivationQuery();
  useXinghuoStatusPolling(Boolean(activation?.activated));
  const mcpRef = useRef<UnifiedMcpPanelHandle>(null);
  const skillsRef = useRef<UnifiedSkillsPanelHandle>(null);
  const discoveryRef = useRef<SkillsPageHandle>(null);
  const promptRef = useRef<PromptPanelHandle>(null);
  const windowControls =
    isWindows() || (isLinux() && settings?.useAppWindowControls);
  const windowAction = async (
    action: "minimize" | "toggleMaximize" | "close",
  ) => {
    try {
      await getCurrentWindow()[action]();
    } catch {
      toast.error("窗口操作失败，请重试");
    }
  };
  const back = () => setView(view === "discovery" ? "skills" : "home");
  const titles: Record<View, string> = {
    home: BRAND.name,
    settings: t("settings.title"),
    mcp: t("mcp.title"),
    skills: t("skills.title"),
    discovery: t("skills.title"),
    prompts: t("prompts.title", { appName: APP_ICON_MAP[app].label }),
    sessions: t("sessionManager.title"),
  };

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-background text-foreground">
      <div
        data-tauri-drag-region
        className="flex h-8 shrink-0 items-center justify-end px-2"
      >
        {windowControls &&
          (["minimize", "toggleMaximize", "close"] as const).map(
            (action, index) => (
              <Button
                key={action}
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                aria-label={["最小化", "最大化", "关闭"][index]}
                onClick={() => void windowAction(action)}
              >
                {index === 0 ? (
                  <Minus className="h-4 w-4" />
                ) : index === 1 ? (
                  <Maximize2 className="h-4 w-4" />
                ) : (
                  <X className="h-4 w-4" />
                )}
              </Button>
            ),
          )}
      </div>
      <header className="flex h-14 shrink-0 items-center justify-between gap-3 border-b px-6">
        <div className="flex min-w-0 items-center gap-2">
          {view !== "home" && (
            <Button
              variant="ghost"
              size="icon"
              disabled={busy}
              aria-label={t("common.back")}
              onClick={back}
            >
              <ArrowLeft className="h-4 w-4" />
            </Button>
          )}
          <h1 className="truncate text-lg font-semibold">{titles[view]}</h1>
        </div>
        <div className="flex items-center gap-2">
          <ThemeToggle />
          {view === "home" ? (
            <Button
              variant="ghost"
              size="icon"
              aria-label={t("common.settings")}
              onClick={() => setView("settings")}
            >
              <Settings className="h-4 w-4" />
            </Button>
          ) : null}
          {view === "mcp" && (
            <>
              <Button
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={() => mcpRef.current?.openImport()}
              >
                <Download className="mr-1 h-4 w-4" />
                {t("common.import")}
              </Button>
              <Button
                size="sm"
                disabled={busy}
                onClick={() => mcpRef.current?.openAdd()}
              >
                <Plus className="mr-1 h-4 w-4" />
                {t("common.add")}
              </Button>
            </>
          )}
          {view === "skills" && (
            <>
              <Button
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={() => skillsRef.current?.openImport()}
              >
                {t("common.import")}
              </Button>
              <Button
                size="sm"
                disabled={busy}
                onClick={() => skillsRef.current?.openDiscovery()}
              >
                {t("skills.discover")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={
                  busy || skillsUpdates.isChecking || !skillsUpdates.hasSkills
                }
                onClick={() => skillsRef.current?.checkUpdates()}
              >
                {t("skills.checkUpdates")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={() => skillsRef.current?.openInstallFromZip()}
              >
                {t("skills.installFromZip")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={() => skillsRef.current?.openRestoreFromBackup()}
              >
                {t("skills.restoreFromBackup")}
              </Button>
            </>
          )}
          {view === "discovery" && (
            <Button
              size="sm"
              disabled={busy}
              onClick={() => discoveryRef.current?.openRepoManager()}
            >
              {t("skills.repoManager")}
            </Button>
          )}
          {view === "prompts" && (
            <Button
              size="sm"
              disabled={busy}
              onClick={() => promptRef.current?.openAdd()}
            >
              <Plus className="mr-1 h-4 w-4" />
              {t("common.add")}
            </Button>
          )}
        </div>
      </header>
      <main className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        {view === "home" && (
          <>
            <ActivationPanel />
            <div className="flex shrink-0 flex-wrap justify-center gap-2 px-6 pb-5">
              {(["sessions", "skills", "mcp", "prompts"] as const).map(
                (item) => (
                  <Button
                    key={item}
                    variant="ghost"
                    size="sm"
                    onClick={() => setView(item)}
                  >
                    {titles[item]}
                  </Button>
                ),
              )}
            </div>
          </>
        )}
        {view === "settings" && (
          <SettingsPage open onOpenChange={() => setView("home")} />
        )}
        {(view === "sessions" || view === "prompts") && (
          <div className="flex gap-2 px-6 py-3">
            {APP_IDS.map((id) => (
              <Button
                key={id}
                size="sm"
                variant={id === app ? "secondary" : "ghost"}
                disabled={busy}
                onClick={() => {
                  setApp(id);
                  localStorage.setItem("xinghuo-token-last-app", id);
                }}
              >
                {APP_ICON_MAP[id].label}
              </Button>
            ))}
          </div>
        )}
        {view === "sessions" && <SessionManagerPage key={app} appId={app} />}
        {view === "prompts" && (
          <PromptPanel
            ref={promptRef}
            open
            appId={app}
            onOpenChange={back}
            onNavigationBlockedChange={setBusy}
          />
        )}
        {view === "mcp" && (
          <UnifiedMcpPanel
            ref={mcpRef}
            onOpenChange={back}
            onInteractionBlockedChange={setBusy}
          />
        )}
        {view === "skills" && (
          <UnifiedSkillsPanel
            ref={skillsRef}
            currentApp={app}
            onOpenDiscovery={() => setView("discovery")}
            onNavigationBlockedChange={setBusy}
            onCheckUpdatesStateChange={setSkillsUpdates}
          />
        )}
        {view === "discovery" && (
          <SkillsPage ref={discoveryRef} initialApp={app} />
        )}
      </main>
      <AppUpdateDialog />
    </div>
  );
}

export default App;
