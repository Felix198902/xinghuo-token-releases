//! 旧版本官方供应商 ID，仅用于兼容历史数据识别。

pub(crate) const CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID: &str = "claude-desktop-official";
pub(crate) const CODEX_OFFICIAL_PROVIDER_ID: &str = "codex-official";
#[cfg(test)]
pub(crate) const GROKBUILD_OFFICIAL_PROVIDER_ID: &str = "grokbuild-official";

pub(crate) fn is_official_seed_id(id: &str) -> bool {
    matches!(
        id,
        "claude-official"
            | "claude-desktop-official"
            | "codex-official"
            | "gemini-official"
            | "grokbuild-official"
    )
}
