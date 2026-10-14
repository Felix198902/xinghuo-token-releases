# 星火无限token 客户端公开下载

[GitHub 发行版](https://github.com/Felix198902/xinghuo-token-releases/releases/tag/v3.20.4) · [Gitee 国内下载](https://gitee.com/jerrylinap/xinghuo-token-releases/releases)

星火无限token 3.20.4 的 macOS 和 Windows 安装包现公开发布，供用户下载和安装。

本仓库用于发布客户端安装包和下载说明。

## 下载安装包

| 版本 | 适用电脑 | 下载 |
| --- | --- | --- |
| macOS 通用版（推荐） | Intel 与 Apple Silicon，macOS 12 或以上 | [下载 DMG](https://github.com/Felix198902/xinghuo-token-releases/releases/download/v3.20.4/xinghuo-token_3.20.4_macos_universal.dmg) |
| macOS Apple Silicon 版 | M 系列芯片，macOS 12 或以上 | [下载 DMG](https://github.com/Felix198902/xinghuo-token-releases/releases/download/v3.20.4/xinghuo-token_3.20.4_macos_arm64.dmg) |
| macOS Intel 版 | Intel 处理器，macOS 12 或以上 | [下载 DMG](https://github.com/Felix198902/xinghuo-token-releases/releases/download/v3.20.4/xinghuo-token_3.20.4_macos_x86_64.dmg) |
| Windows x64 版 | Windows 10 / 11，64 位 Intel / AMD 电脑 | [下载安装程序](https://github.com/Felix198902/xinghuo-token-releases/releases/download/v3.20.4/xinghuo-token_3.20.4_windows_x64_setup.exe) |

## 安装方法

- macOS：打开 DMG，将“星火无限token”拖入右侧 Applications（应用程序），再从“应用程序”中打开。仅打开 DMG 不代表已安装。
- Windows：下载并运行 setup.exe，按照中文向导安装。安装程序包含 WebView2 引导程序；系统缺少运行环境时，需要联网下载安装。
- 不确定 Mac 芯片类型时，请选择 macOS 通用版。

## 签名与验证

- macOS 应用和 DMG 已使用 Apple Developer ID 签名，通过苹果公证并附加离线票据。
- Windows 安装包尚未进行 Windows 数字签名，可能出现 SmartScreen 未知发布者提示；尚未进行 Windows 实机安装测试。
- 已完成构建、架构及安装包完整性校验。文件 SHA-256 见附件 SHA256SUMS.txt。

## 项目来源

本客户端基于 [CC Switch](https://github.com/farion1231/cc-switch) 定制。上游 MIT 许可证随附件提供，保留原作者版权声明。
