import { useState } from "react";
import { useTranslation } from "react-i18next";
import { TriangleAlert, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { BRAND } from "@/config/brand";
import { settingsApi } from "@/lib/api";
import type { XinghuoActivationView } from "@/lib/api/xinghuo";

export type RenewalReason = "exhausted" | "invalid";

/**
 * 需要续费的原因。以服务端的 valid 为准：订阅计费分组的密钥余额可能显示为 0
 * 但仍然可用，不能只看余额。
 */
export function renewalReason(
  view: XinghuoActivationView | undefined,
): RenewalReason | null {
  if (!view?.activated || view.valid !== false) return null;
  return view.quota && view.quota.remaining <= 0 ? "exhausted" : "invalid";
}

/** 余额为零也可能来自账户余额不足，只有累计用量触顶才判定密钥上限耗尽。 */
export function isKeyQuotaExhausted(
  view: XinghuoActivationView | undefined,
): boolean {
  return (
    view?.balanceScope === "key_cap" &&
    !!view.quota &&
    view.quota.total > 0 &&
    view.quota.used >= view.quota.total
  );
}

interface RenewalBannerProps {
  view: XinghuoActivationView | undefined;
  onOpenActivation: () => void;
}

/** 余额用完或密钥失效时挂在主界面顶部的「去续费」提示。关闭后本次运行不再显示。 */
export function RenewalBanner({ view, onOpenActivation }: RenewalBannerProps) {
  const { t } = useTranslation();
  const [dismissed, setDismissed] = useState(false);
  const reason = renewalReason(view);
  if (!reason || dismissed) return null;

  const contact = view?.contact || BRAND.contact;
  const shopUrl = view?.shopUrl || BRAND.shopUrl;

  return (
    <div className="px-6 pt-4">
      <div
        role="alert"
        className="flex flex-wrap items-center gap-3 rounded-lg border border-amber-500/30 bg-amber-500/5 px-4 py-3 text-sm"
      >
        <TriangleAlert className="h-4 w-4 shrink-0 text-amber-600 dark:text-amber-400" />
        <span className="min-w-0 flex-1">
          {reason === "exhausted"
            ? t(
                isKeyQuotaExhausted(view)
                  ? "activation.banner.keyExhausted"
                  : "activation.banner.exhausted",
              )
            : t("activation.banner.invalid", { contact })}
        </span>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            size="sm"
            className="h-8"
            onClick={() => void settingsApi.openExternal(shopUrl)}
          >
            {t("activation.banner.renew")}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8"
            onClick={onOpenActivation}
          >
            {t("activation.banner.enterCode")}
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => setDismissed(true)}
            aria-label={t("activation.banner.dismiss")}
            title={t("activation.banner.dismiss")}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}

export default RenewalBanner;
