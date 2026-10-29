import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  CheckCircle2,
  ExternalLink,
  Loader2,
  LogOut,
  RefreshCw,
  ShieldAlert,
  Settings2,
  TriangleAlert,
  Undo2,
  Zap,
} from "lucide-react";
import { QuickUseBar } from "@/components/QuickUseBar";
import { APP_ICON_MAP, APP_IDS, MORE_PLATFORM_IDS } from "@/config/appConfig";
import type { AppId } from "@/lib/api/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { SupportContacts } from "./SupportContacts";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import appIcon from "@/assets/icons/app-icon.png";
import { BRAND } from "@/config/brand";
import { providersApi, settingsApi } from "@/lib/api";
import {
  xinghuoApi,
  type XinghuoActivationView,
  type XinghuoAppliedApp,
  type XinghuoManagedApp,
} from "@/lib/api/xinghuo";
import {
  useXinghuoActivationQuery,
  XINGHUO_ACTIVATION_QUERY_KEY,
} from "@/lib/query/xinghuo";
import { describeActivationError } from "./activationErrors";
import { isKeyQuotaExhausted, renewalReason } from "./RenewalBanner";
import { DesktopConnectionDialog } from "./DesktopConnectionDialog";
import { DesktopRuntimeNotice } from "./DesktopRuntimeNotice";

function formatExpiry(
  value: string | null | undefined,
  permanentLabel: string,
): string {
  if (!value) return permanentLabel;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString();
}

function formatAmount(value: number): string {
  return Number.isFinite(value) ? value.toFixed(2) : "-";
}

/** 各接入平台对应的官方服务器名称，用于接入成功提示 */
const OFFICIAL_SERVER_NAMES: Record<XinghuoManagedApp, string> = {
  claude: "Claude",
  "claude-desktop": "Claude Desktop",
  codex: "ChatGPT",
  opencode: "OpenCode",
  gemini: "Gemini",
  grokbuild: "Grok",
  openclaw: "OpenClaw",
  hermes: "Hermes",
  mcode: "MiniMax",
  pi: "Pi",
};

function connectedToast(app: XinghuoManagedApp): string {
  return `已连接上 【${OFFICIAL_SERVER_NAMES[app]}】 官方服务器，请放心使用`;
}

interface StatusCardProps {
  view: XinghuoActivationView;
  refreshing: boolean;
  refreshError: string | null;
  contact: string;
  onRefresh: () => void;
}

function StatusCard({
  view,
  refreshing,
  refreshError,
  contact,
  onRefresh,
}: StatusCardProps) {
  const { t } = useTranslation();
  const reason = renewalReason(view);
  const unitLabel = view.unitLabel || BRAND.unitLabel;
  const keyQuotaExhausted = isKeyQuotaExhausted(view);

  return (
    <section className="space-y-4 rounded-xl border border-border bg-card/60 p-5 shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          {reason === "exhausted" ? (
            <TriangleAlert className="h-5 w-5 shrink-0 text-amber-500" />
          ) : reason === "invalid" ? (
            <ShieldAlert className="h-5 w-5 shrink-0 text-red-500" />
          ) : (
            <CheckCircle2 className="h-5 w-5 shrink-0 text-emerald-500" />
          )}
          <span className="font-semibold">
            {reason === "exhausted"
              ? t(
                  keyQuotaExhausted
                    ? "activation.keyExhausted"
                    : "activation.exhausted",
                )
              : reason === "invalid"
                ? t("activation.invalid")
                : t("activation.activated")}
          </span>
          {view.maskedKey && (
            <code className="truncate rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
              {view.maskedKey}
            </code>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-8 gap-1.5 text-xs"
            onClick={onRefresh}
            disabled={refreshing}
          >
            <RefreshCw
              className={`h-3.5 w-3.5 ${refreshing ? "animate-spin" : ""}`}
            />
            {t("activation.refresh")}
          </Button>
        </div>
      </div>

      {reason === "exhausted" && (
        <p className="text-sm text-amber-700 dark:text-amber-300">
          {t(
            keyQuotaExhausted
              ? "activation.keyExhaustedHint"
              : "activation.exhaustedHint",
          )}
        </p>
      )}
      {reason === "invalid" && (
        <p className="text-sm text-red-600 dark:text-red-400">
          {t("activation.invalidHint", { contact })}
        </p>
      )}

      {view.quota && (
        <dl
          className={`grid gap-3 text-sm ${view.balanceScope === "key_cap" ? "grid-cols-2 sm:grid-cols-4" : "grid-cols-3"}`}
        >
          <div className="space-y-1">
            <dt className="text-xs text-muted-foreground">
              {t(
                view.balanceScope === "account"
                  ? "activation.accountBalance"
                  : "activation.remaining",
              )}
            </dt>
            <dd className="font-semibold tabular-nums">
              {unitLabel}
              {formatAmount(view.quota.remaining)}
            </dd>
          </div>
          <div className="space-y-1">
            <dt className="text-xs text-muted-foreground">
              {t("activation.expiresAt")}
            </dt>
            <dd className="font-medium tabular-nums">
              {formatExpiry(view.expiresAt, t("activation.permanent"))}
            </dd>
          </div>
          {view.balanceScope === "key_cap" && (
            <div className="space-y-1">
              <dt className="text-xs text-muted-foreground">
                {t("activation.keyQuotaLimit")}
              </dt>
              <dd className="font-medium tabular-nums">
                {unitLabel}
                {formatAmount(view.quota.total)}
              </dd>
            </div>
          )}
          <div className="space-y-1">
            <dt className="text-xs text-muted-foreground">已使用</dt>
            <dd className="font-medium tabular-nums">
              {unitLabel}
              {formatAmount(view.quota.used)}
            </dd>
          </div>
        </dl>
      )}

      {view.apps.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span className="text-muted-foreground">
            {t("activation.configuredApps")}
          </span>
          {view.apps.map((app) => (
            <span
              key={app}
              className="rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-emerald-700 dark:text-emerald-300"
            >
              {t(`activation.apps.${app}`)}
            </span>
          ))}
        </div>
      )}

      {refreshError && (
        <p className="text-xs text-muted-foreground">{refreshError}</p>
      )}
    </section>
  );
}

/**
 * 激活页：粘贴激活码或 sk- 密钥，一键配置 Claude Code / Codex / OpenCode。
 * 已激活时同一个输入框用来续费或更换密钥。所有联网与写盘都在 Rust 端完成，
 * 完整密钥不进 WebView。
 */
export function ActivationPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: view } = useXinghuoActivationQuery();
  const [input, setInput] = useState("");
  const [activationDialogOpen, setActivationDialogOpen] = useState(false);
  const [usingApp, setUsingApp] = useState<XinghuoAppliedApp["app"]>("codex");
  const operationRef = useRef(false);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [desktopOpen, setDesktopOpen] = useState(false);
  const refreshedRef = useRef(false);

  const activated = view?.activated ?? false;
  const contact = view?.contact || BRAND.contact;
  const shopUrl = view?.shopUrl || BRAND.shopUrl;

  const closeActivationDialog = () => {
    setActivationDialogOpen(false);
    setInput("");
    setErrorText(null);
  };

  const appName = (app: XinghuoAppliedApp["app"]) =>
    t(`activation.apps.${app}`);

  const afterProvidersChanged = async () => {
    await queryClient.invalidateQueries({ queryKey: ["providers"] });
    await providersApi.updateTrayMenu().catch((error) => {
      console.warn("[ActivationPanel] Failed to update tray menu", error);
    });
  };

  const refreshMutation = useMutation({
    mutationFn: () => xinghuoApi.refreshStatus(),
    onSuccess: (next) => {
      setRefreshError(null);
      queryClient.setQueryData(XINGHUO_ACTIVATION_QUERY_KEY, next);
    },
    onError: (error) => {
      setRefreshError(describeActivationError(error, t, contact));
    },
  });

  // 打开页面时刷新一次余额；失败只在卡片里轻提示，不打断顾客。
  useEffect(() => {
    if (!activated || refreshedRef.current) return;
    refreshedRef.current = true;
    refreshMutation.mutate();
  }, [activated, refreshMutation]);

  const activateMutation = useMutation({
    mutationFn: (value: string) => xinghuoApi.activate(value),
    onSuccess: async (result) => {
      queryClient.setQueryData(XINGHUO_ACTIVATION_QUERY_KEY, result.view);
      closeActivationDialog();
      await afterProvidersChanged();

      const configured = result.apps.filter((a) => a.ok).map((a) => a.app);
      const failed = result.apps.filter((a) => !a.ok);
      if (result.renewed) {
        toast.success(t("activation.renewSuccess"), { closeButton: true });
      } else if (configured.length > 0) {
        toast.success(
          t("activation.success", {
            apps: configured.map(appName).join("、"),
          }),
          { closeButton: true },
        );
      } else if (failed.length === 0) {
        // 服务端按分组关掉了所有工具：激活本身成功，但没有可配置的工具。
        toast.warning(t("activation.successNoApps", { contact }), {
          closeButton: true,
        });
      }
      for (const item of failed) {
        toast.error(
          t("activation.partialFailure", {
            app: appName(item.app),
            error: item.error ?? "",
          }),
          { closeButton: true },
        );
      }
      if (configured.includes("claude-desktop")) {
        toast.info(
          "Claude Desktop 接入后，请保持星火运行，并完全退出后重启 Claude Desktop",
        );
      }
    },
    onError: (error) => {
      setErrorText(describeActivationError(error, t, contact));
    },
  });

  const deactivateMutation = useMutation({
    mutationFn: () => xinghuoApi.deactivate(),
    onSuccess: async (result) => {
      setConfirmOpen(false);
      setActivationDialogOpen(false);
      queryClient.setQueryData(XINGHUO_ACTIVATION_QUERY_KEY, result.view);
      await afterProvidersChanged();
      const failed = result.apps.filter((a) => !a.ok);
      if (failed.length === 0) {
        toast.success(t("activation.deactivateSuccess"), { closeButton: true });
      } else {
        toast.error(
          t("activation.deactivatePartial", {
            apps: failed
              .map((a) => `${appName(a.app)}（${a.error ?? ""}）`)
              .join("；"),
          }),
          { closeButton: true },
        );
      }
    },
    onError: (error) => {
      setConfirmOpen(false);
      toast.error(describeActivationError(error, t, contact), {
        closeButton: true,
      });
    },
  });

  const restoreMutation = useMutation({
    mutationFn: () => xinghuoApi.restore(usingApp),
    onSuccess: async (result) => {
      queryClient.setQueryData(XINGHUO_ACTIVATION_QUERY_KEY, result.view);
      await afterProvidersChanged();
      const failed = result.apps.filter((app) => !app.ok);
      if (failed.length) {
        toast.error(
          failed.map((app) => `${appName(app.app)}：${app.error}`).join("；"),
        );
      } else {
        toast.success("已恢复切换前配置，重启对应工具后生效");
      }
    },
    onError: (error) => toast.error(describeActivationError(error, t, contact)),
  });

  const useAppMutation = useMutation({
    mutationFn: (app: XinghuoAppliedApp["app"]) => xinghuoApi.useApp(app),
    onSuccess: async (result, app) => {
      queryClient.setQueryData(XINGHUO_ACTIVATION_QUERY_KEY, result.view);
      if (result.alreadyConnected) {
        toast.success("您已成功接入");
        return;
      }
      await afterProvidersChanged();
      const applied = result.apps.find((item) => item.app === app);
      if (applied?.ok) {
        toast.success(connectedToast(app));
      } else {
        toast.error(
          t("activation.partialFailure", {
            app: appName(app),
            error: applied?.error ?? "",
          }),
        );
      }
    },
    onError: (error) => toast.error(describeActivationError(error, t, contact)),
    onSettled: () => {
      operationRef.current = false;
    },
  });

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (
      activateMutation.isPending ||
      useAppMutation.isPending ||
      restoreMutation.isPending ||
      deactivateMutation.isPending
    )
      return;
    const value = input.trim();
    if (!value) {
      setErrorText(t("activation.errors.inputEmpty"));
      return;
    }
    setErrorText(null);
    activateMutation.mutate(value);
  };

  const openShop = () => {
    void settingsApi.openExternal(shopUrl);
  };

  const pending = activateMutation.isPending;
  const operationPending =
    pending ||
    useAppMutation.isPending ||
    restoreMutation.isPending ||
    deactivateMutation.isPending ||
    desktopOpen;

  return (
    <div className="activation-page flex-1 overflow-y-auto px-4 pb-8 sm:px-6">
      <div className="mx-auto max-w-2xl space-y-5 pt-6">
        {activated && view ? (
          <StatusCard
            view={view}
            refreshing={refreshMutation.isPending}
            refreshError={refreshError}
            contact={contact}
            onRefresh={() => refreshMutation.mutate()}
          />
        ) : (
          <div className="flex items-center gap-4 rounded-2xl border border-primary/15 bg-card/75 p-5">
            <img src={appIcon} alt="" className="h-16 w-16" />
            <div className="space-y-1">
              <h2 className="text-xl font-semibold">{BRAND.name}</h2>
              <p className="text-sm text-muted-foreground">
                一个入口，连接你的 AI 工作台
              </p>
            </div>
          </div>
        )}

        <section
          aria-label="工具接入"
          className="overflow-hidden rounded-2xl border border-primary/20 bg-card/85 shadow-sm"
        >
          <div className="space-y-1 border-b border-primary/10 bg-primary/5 px-5 py-5">
            <h2 className="text-lg font-semibold">选择工具，接入星火</h2>
            <p className="text-sm text-muted-foreground">
              接入或还原将应用于所选工具
            </p>
          </div>
          <div className="px-5 pt-5">
            <QuickUseBar
              activeApp={usingApp}
              disabled={operationPending}
              moreApps={MORE_PLATFORM_IDS}
              onSelect={(app) => {
                if (operationPending) return;
                if (
                  (APP_IDS as AppId[]).includes(app) ||
                  MORE_PLATFORM_IDS.includes(app)
                ) {
                  setUsingApp(app as XinghuoManagedApp);
                }
              }}
            />
          </div>
          <div className="px-5 pb-6 pt-6 text-center">
            <p className="mb-4 text-sm font-medium text-foreground">
              当前选择：{APP_ICON_MAP[usingApp].label}
            </p>
            <div className="mx-auto grid max-w-md grid-cols-1 gap-3 min-[420px]:grid-cols-2">
              <Button
                type="button"
                className="h-14 w-full gap-2.5 rounded-xl bg-primary px-6 text-base font-semibold text-primary-foreground shadow-lg shadow-primary/15 hover:bg-primary/90 dark:bg-primary dark:hover:bg-primary/90"
                disabled={operationPending}
                onClick={() => {
                  if (operationRef.current || operationPending) return;
                  if (!activated) {
                    setActivationDialogOpen(true);
                    return;
                  }
                  if (
                    usingApp === "claude-desktop" &&
                    !view?.desktopConnectionConfigured &&
                    (view?.quota?.remaining === 0 || view?.valid === false)
                  ) {
                    setDesktopOpen(true);
                    return;
                  }
                  operationRef.current = true;
                  useAppMutation.mutate(usingApp);
                }}
              >
                {useAppMutation.isPending ? (
                  <Loader2
                    className="h-5 w-5 animate-spin"
                    aria-hidden="true"
                  />
                ) : (
                  <Zap className="h-5 w-5" aria-hidden="true" />
                )}
                {useAppMutation.isPending ? "正在接入…" : "一键接入"}
              </Button>
              <Button
                type="button"
                variant="outline"
                className="h-14 w-full gap-2.5 rounded-xl px-6 text-base font-semibold text-foreground"
                disabled={operationPending || !view?.restoreAvailable}
                onClick={() => restoreMutation.mutate()}
              >
                {restoreMutation.isPending ? (
                  <Loader2
                    className="h-5 w-5 animate-spin"
                    aria-hidden="true"
                  />
                ) : (
                  <Undo2 className="h-5 w-5" aria-hidden="true" />
                )}
                {restoreMutation.isPending ? "正在还原…" : "一键还原"}
              </Button>
            </div>
          </div>
          <div className="flex flex-wrap items-center justify-center gap-x-3 gap-y-2 border-t border-border/60 bg-muted/20 px-4 py-3">
            {activated && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={operationPending}
                onClick={() => setActivationDialogOpen(true)}
              >
                续费 / 更换激活码
              </Button>
            )}
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={operationPending}
              onClick={() => setDesktopOpen(true)}
            >
              <Settings2 className="mr-1.5 h-3.5 w-3.5" />
              Claude Desktop 接口
            </Button>
          </div>
          {view?.desktopConnectionConfigured && (
            <p className="px-4 pb-3 text-center text-xs text-muted-foreground">
              Desktop：{view.desktopModel ?? "Claude"} · {view.desktopMaskedKey}
            </p>
          )}
          {view?.desktopConnectionConfigured && (
            <div className="px-4 pb-4">
              <DesktopRuntimeNotice status={view.desktopRuntime} />
            </div>
          )}
        </section>

        <Dialog
          open={activationDialogOpen}
          onOpenChange={(open) => {
            if (!open && !operationPending) closeActivationDialog();
          }}
        >
          <DialogContent
            className="rounded-2xl border-primary/20"
            onEscapeKeyDown={(event) => {
              if (operationPending) event.preventDefault();
            }}
          >
            <DialogHeader>
              <DialogTitle>
                {activated ? t("activation.renewTitle") : "输入激活码或密钥"}
              </DialogTitle>
              <DialogDescription>
                {activated
                  ? t("activation.renewHint")
                  : "粘贴激活码或 sk- 密钥，接入星火服务。"}
              </DialogDescription>
            </DialogHeader>
            <form onSubmit={handleSubmit} className="space-y-4 p-6">
              <Input
                type="password"
                value={input}
                onChange={(event) => {
                  setInput(event.target.value);
                  if (errorText) setErrorText(null);
                }}
                placeholder={
                  activated
                    ? t("activation.renewPlaceholder")
                    : t("activation.inputPlaceholder")
                }
                aria-label={
                  activated
                    ? t("activation.renewPlaceholder")
                    : t("activation.inputPlaceholder")
                }
                autoComplete="off"
                spellCheck={false}
                disabled={operationPending}
                className="font-mono"
              />
              {errorText && (
                <p
                  role="alert"
                  className="text-sm leading-relaxed text-red-600 dark:text-red-400"
                >
                  {errorText}
                </p>
              )}
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  type="submit"
                  disabled={operationPending}
                  className="gap-2"
                >
                  {pending && <Loader2 className="h-4 w-4 animate-spin" />}
                  {pending
                    ? t("activation.activating")
                    : activated
                      ? t("activation.renew")
                      : t("activation.activate")}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  className="gap-2"
                  onClick={openShop}
                >
                  <ExternalLink className="h-4 w-4" />
                  {t("activation.buy")}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  disabled={operationPending}
                  onClick={closeActivationDialog}
                >
                  取消
                </Button>
              </div>
            </form>
          </DialogContent>
        </Dialog>

        <div className="space-y-1 text-xs leading-relaxed text-muted-foreground">
          <p>{t("activation.restartHint")}</p>
        </div>

        <SupportContacts contact={contact} />

        {(activated || view?.restoreAvailable) && (
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="gap-1.5 text-xs text-red-600 hover:text-red-700 dark:text-red-400"
              onClick={() => setConfirmOpen(true)}
              disabled={!activated || operationPending}
            >
              <LogOut className="h-3.5 w-3.5" />
              {t("activation.deactivate")}
            </Button>
          </div>
        )}
      </div>

      {desktopOpen && (
        <DesktopConnectionDialog
          view={view ?? { activated: false, apps: [], hasWallet: false }}
          onClose={() => setDesktopOpen(false)}
          onConfigured={async (result) => {
            queryClient.setQueryData(XINGHUO_ACTIVATION_QUERY_KEY, result.view);
            await afterProvidersChanged();
            toast.success(connectedToast("claude-desktop"));
          }}
        />
      )}
      <ConfirmDialog
        isOpen={confirmOpen}
        title={t("activation.deactivateTitle")}
        message={t("activation.deactivateMessage")}
        confirmText={t("activation.deactivate")}
        variant="destructive"
        pending={deactivateMutation.isPending}
        onConfirm={() => deactivateMutation.mutate()}
        onCancel={() => setConfirmOpen(false)}
      />
    </div>
  );
}

export default ActivationPanel;
