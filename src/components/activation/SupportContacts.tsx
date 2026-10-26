import { useState } from "react";
import { ChevronDown, ExternalLink } from "lucide-react";
import { settingsApi } from "@/lib/api";
import { Button } from "@/components/ui/button";
import qqQr from "@/assets/contact/qq-customer-service.jpg";
import wechatQr from "@/assets/contact/wechat-customer-service.jpg";

const contacts = [
  { kind: "qq", label: "QQ", account: "169106354", image: qqQr },
  { kind: "wechat", label: "微信", account: "US666377", image: wechatQr },
] as const;

function ContactIcon({ kind }: { kind: "qq" | "wechat" }) {
  return (
    <svg
      viewBox="0 0 24 24"
      className="h-6 w-6"
      fill="currentColor"
      aria-hidden="true"
    >
      {kind === "qq" ? (
        <>
          <path d="M12 2c-3.3 0-5.4 3.2-5.4 7.3 0 1-.2 1.7-.7 2.5C4.5 14 4 16.2 4.8 16.7c.5.3 1.2-.4 1.8-1.1.1 1.8.9 3 2.1 3.7-1.3.4-2.1 1-1.9 1.7.3 1 3.4 1 5.2.2 1.8.8 4.9.8 5.2-.2.2-.7-.6-1.3-1.9-1.7 1.2-.7 2-1.9 2.1-3.7.6.7 1.3 1.4 1.8 1.1.8-.5.3-2.7-1.1-4.9-.5-.8-.7-1.5-.7-2.5C17.4 5.2 15.3 2 12 2Z" />
          <ellipse cx="10.2" cy="7" rx=".8" ry="1.2" fill="white" />
          <ellipse cx="13.8" cy="7" rx=".8" ry="1.2" fill="white" />
          <path d="M9 10h6l-3 2Z" fill="white" />
        </>
      ) : (
        <>
          <path d="M9 3C4.6 3 1 5.9 1 9.5c0 2.1 1.2 3.9 3.1 5.1l-.8 2.5 2.9-1.4c.9.2 1.8.3 2.8.3h.7A6.7 6.7 0 0 1 9 13c0-3.8 3.6-6.8 8-6.8C15.6 4.3 12.6 3 9 3Z" />
          <path d="M23 13c0-3.1-2.9-5.5-6.5-5.5S10 9.9 10 13s2.9 5.5 6.5 5.5c.8 0 1.6-.1 2.3-.3l2.4 1.3-.6-2.1C22.1 16.4 23 14.8 23 13Z" />
          <g fill="white">
            <circle cx="6" cy="8" r="1" />
            <circle cx="12" cy="8" r="1" />
            <circle cx="14" cy="12" r=".8" />
            <circle cx="19" cy="12" r=".8" />
          </g>
        </>
      )}
    </svg>
  );
}

export function SupportContacts({ contact }: { contact: string }) {
  const [expanded, setExpanded] = useState({ qq: false, wechat: false });
  // 品牌客服入口始终保留，服务端只下发 QQ 时也不能隐藏微信。
  const hasKnownContact = contacts.some((item) =>
    contact.includes(item.account),
  );
  return (
    <section
      aria-label="联系客服"
      className="rounded-2xl border border-border/70 bg-card/70 p-5"
    >
      <div className="mb-4 flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="text-sm font-semibold">联系客服</h3>
        <span className="text-xs text-muted-foreground">点击展开二维码</span>
      </div>
      <div className="grid items-start gap-3 sm:grid-cols-2">
        {contacts.map((item) => {
          const open = expanded[item.kind];
          return (
            <div
              key={item.kind}
              className="overflow-hidden rounded-xl border border-border bg-background/60"
            >
              <button
                type="button"
                aria-expanded={open}
                aria-controls={`contact-${item.kind}`}
                onClick={() =>
                  setExpanded((previous) => ({
                    ...previous,
                    [item.kind]: !previous[item.kind],
                  }))
                }
                className="flex w-full items-center gap-3 p-4 text-left transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <span
                  className={`rounded-lg p-2 ${item.kind === "qq" ? "bg-sky-500/10 text-sky-600 dark:text-sky-300" : "bg-emerald-500/10 text-emerald-600 dark:text-emerald-300"}`}
                >
                  <ContactIcon kind={item.kind} />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium">
                    {item.label}
                  </span>
                  <span className="block truncate text-xs text-muted-foreground">
                    {item.account}
                  </span>
                </span>
                <ChevronDown
                  className={`h-4 w-4 text-muted-foreground transition-transform ${open ? "rotate-180" : ""}`}
                />
              </button>
              {open && (
                <div
                  id={`contact-${item.kind}`}
                  className="space-y-3 border-t border-border p-4 text-center"
                >
                  <img
                    src={item.image}
                    alt={`${item.label} ${item.account} 加好友二维码`}
                    className="mx-auto w-48 max-w-full rounded-lg bg-white"
                  />
                  <p className="text-xs text-muted-foreground">
                    使用{item.label}扫描，或搜索 {item.account}
                  </p>
                  {item.kind === "qq" && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() =>
                        void settingsApi.openExternal(
                          `https://wpa.qq.com/msgrd?v=3&uin=${item.account}&site=qq&menu=yes`,
                        )
                      }
                    >
                      <ExternalLink className="mr-2 h-3.5 w-3.5" />
                      打开 QQ 聊天
                    </Button>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
      {contact.trim() && !hasKnownContact && (
        <p className="mt-3 text-sm text-muted-foreground">{contact}</p>
      )}
    </section>
  );
}
