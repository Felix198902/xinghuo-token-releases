import { useQuery, useQueryClient } from "@tanstack/react-query";
import { xinghuoApi } from "@/lib/api/xinghuo";

export const XINGHUO_ACTIVATION_QUERY_KEY = ["xinghuo", "activation"] as const;

const STATUS_REFRESH_INTERVAL_MS = 5 * 60 * 1000;

/** 本机激活状态（读本地文件，不联网） */
export function useXinghuoActivationQuery() {
  return useQuery({
    queryKey: XINGHUO_ACTIVATION_QUERY_KEY,
    queryFn: () => xinghuoApi.getActivation(),
    staleTime: Infinity,
  });
}

/**
 * 已激活时每 5 分钟联网刷新一次余额与有效性，结果写回激活状态，
 * 「去续费」横幅据此显示。窗口隐藏时暂停，回到前台时补一次。
 */
export function useXinghuoStatusPolling(enabled: boolean) {
  const queryClient = useQueryClient();
  return useQuery({
    queryKey: ["xinghuo", "status"],
    queryFn: async () => {
      const view = await xinghuoApi.refreshStatus();
      queryClient.setQueryData(XINGHUO_ACTIVATION_QUERY_KEY, view);
      return view;
    },
    enabled,
    refetchInterval: STATUS_REFRESH_INTERVAL_MS,
    staleTime: 60 * 1000,
    retry: false,
  });
}
