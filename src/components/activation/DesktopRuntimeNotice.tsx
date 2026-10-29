import { useState } from "react";
import { AlertTriangle, Download, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { settingsApi } from "@/lib/api/settings";
import type { XinghuoDesktopRuntimeStatus } from "@/lib/api/xinghuo";

interface Props {
  status?: XinghuoDesktopRuntimeStatus;
  onRecheck?: () => Promise<void>;
}

export function DesktopRuntimeNotice({ status, onRecheck }: Props) {
  const [checking, setChecking] = useState(false);
  if (!status || (status.helperInstalled && status.workspaceInstalled)) {
    return null;
  }

  const missing = [
    !status.helperInstalled && "Claude CLI",
    !status.workspaceInstalled && "VM 工作区",
  ]
    .filter(Boolean)
    .join("、");

  return (
    <div
      role="alert"
      className="space-y-3 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm"
    >
      <div className="flex gap-2">
        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600" />
        <div className="space-y-1.5">
          <p className="font-medium">
            缺少 {missing}，Code/Cowork 可能无法启动
          </p>
          <p className="text-xs leading-relaxed text-muted-foreground">
            接口接入不等于工作区已就绪。若出现 Failed to start Claude&apos;s
            workspace 或 ERR_TIMED_OUT，请安装官方离线完整版（包含 Claude CLI 和
            VM 工作区），再完全退出并重新打开 Claude Desktop。
          </p>
        </div>
      </div>
      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() =>
            void settingsApi.openExternal(status.offlineInstallerUrl)
          }
        >
          <Download className="mr-1.5 h-3.5 w-3.5" />
          下载官方离线完整版
        </Button>
        {onRecheck && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={checking}
            onClick={() => {
              setChecking(true);
              void onRecheck().finally(() => setChecking(false));
            }}
          >
            <RefreshCw
              className={`mr-1.5 h-3.5 w-3.5 ${checking ? "animate-spin" : ""}`}
            />
            重新检测
          </Button>
        )}
      </div>
    </div>
  );
}
