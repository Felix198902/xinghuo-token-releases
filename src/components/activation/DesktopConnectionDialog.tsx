import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Loader2, RefreshCw, Save, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import {
  xinghuoApi,
  isXinghuoError,
  type XinghuoActivationView,
  type XinghuoActivateResult,
} from "@/lib/api/xinghuo";
import { DesktopRuntimeNotice } from "./DesktopRuntimeNotice";

interface Props {
  view: XinghuoActivationView;
  onClose: () => void;
  onConfigured: (result: XinghuoActivateResult) => Promise<void>;
}

function errorMessage(error: unknown): string {
  if (isXinghuoError(error)) {
    if (error.code.startsWith("DESKTOP_")) return error.message;
    if (error.code === "API_KEY_QUOTA_EXHAUSTED" || error.status === 429)
      return "接口密钥额度已用完，请充值或更换有额度的密钥";
    if (error.code === "NETWORK") return "网络连接失败，请重试";
    if (error.status === 401 || error.status === 403)
      return "密钥无效或没有访问权限";
    return `接口请求失败${error.status ? `（HTTP ${error.status}）` : ""}`;
  }
  return "配置失败，请重试";
}

export function DesktopConnectionDialog({
  view,
  onClose,
  onConfigured,
}: Props) {
  const [input, setInput] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [runtime, setRuntime] = useState(view.desktopRuntime);
  const discover = useMutation({
    mutationFn: () => xinghuoApi.discoverDesktopModels(input.trim()),
    onSuccess: (items) => {
      setModels(items);
      setError(null);
    },
    onError: (e) => {
      setModels([]);
      setError(errorMessage(e));
    },
  });
  const configure = useMutation({
    mutationFn: () => xinghuoApi.configureDesktop(input.trim(), null),
    onSuccess: async (result) => {
      setInput("");
      setRuntime(result.view.desktopRuntime);
      await onConfigured(result);
      const runtimeReady =
        !result.view.desktopRuntime ||
        (result.view.desktopRuntime.helperInstalled &&
          result.view.desktopRuntime.workspaceInstalled);
      if (runtimeReady) onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  const pending = discover.isPending || configure.isPending;

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !pending) onClose();
      }}
    >
      <DialogContent aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>Claude Desktop 接口</DialogTitle>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!pending && models.length) configure.mutate();
          }}
        >
          <div className="space-y-4 overflow-y-auto px-6 py-5">
            <div className="space-y-1.5">
              <label htmlFor="desktop-key" className="text-sm font-medium">
                星火接口密钥
              </label>
              <Input
                id="desktop-key"
                type="password"
                autoComplete="off"
                spellCheck={false}
                disabled={pending}
                value={input}
                placeholder={view.desktopMaskedKey ?? view.maskedKey ?? "sk-…"}
                onChange={(event) => {
                  setInput(event.target.value);
                  setModels([]);
                  setError(null);
                }}
              />
              <p className="text-xs text-muted-foreground">
                留空会使用当前保存的密钥
                {view.maskedKey ? `（${view.maskedKey}）` : ""}
                ；输入新密钥仅用于 Claude Desktop。
              </p>
            </div>
            <div className="space-y-1.5">
              <div className="flex items-center justify-between gap-2">
                <span className="text-sm font-medium">可用模型</span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  disabled={pending}
                  onClick={() => discover.mutate()}
                >
                  <RefreshCw
                    className={`mr-1.5 h-3.5 w-3.5 ${discover.isPending ? "animate-spin" : ""}`}
                  />
                  读取模型
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                全部可用模型会加入 Claude Desktop，可在其对话界面直接切换。
              </p>
              <div className="max-h-48 space-y-1 overflow-y-auto rounded-md border p-3 text-sm">
                {models.length ? (
                  models.map((id) => <div key={id}>{id}</div>)
                ) : (
                  <span className="text-muted-foreground">未读取</span>
                )}
              </div>
            </div>
            <DesktopRuntimeNotice
              status={runtime}
              onRecheck={async () => {
                const latest = await xinghuoApi.getActivation();
                setRuntime(latest.desktopRuntime);
              }}
            />
            {error && (
              <p
                role="alert"
                className="text-sm text-red-600 dark:text-red-400"
              >
                {error}
              </p>
            )}
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={pending}
              onClick={onClose}
            >
              <X className="mr-1.5 h-4 w-4" />
              取消
            </Button>
            <Button type="submit" disabled={pending || !models.length}>
              {configure.isPending ? (
                <Loader2 className="mr-1.5 h-4 w-4 animate-spin" />
              ) : (
                <Save className="mr-1.5 h-4 w-4" />
              )}
              保存并接入
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
