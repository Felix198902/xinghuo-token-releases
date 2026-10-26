#!/usr/bin/env bash
set -euo pipefail

# 凭据只从钥匙串读取，不在项目中保存 Apple 密码。
PROFILE="${NOTARY_PROFILE:-novadesk}"
IDENTITY="${APPLE_SIGNING_IDENTITY:-Developer ID Application: JIAYE LIN (63AVMD7Z5P)}"
# 默认发布通用包，避免 Intel 客户误装仅支持 Apple Silicon 的版本。
TARGET="${MACOS_TARGET:-universal-apple-darwin}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[[ "$(uname -s)" == Darwin ]] || { echo "请在 macOS 上运行" >&2; exit 1; }
xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null

if [[ "${1:-}" != --skip-build ]]; then
  if [[ "$TARGET" == universal-apple-darwin ]]; then
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
  else
    rustup target add "$TARGET"
  fi
  pnpm typecheck
  APPLE_SIGNING_IDENTITY="$IDENTITY" pnpm tauri build --target "$TARGET" --bundles app --ci
fi

NAME="$(python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json", encoding="utf-8"))["productName"])')"
VERSION="$(python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json", encoding="utf-8"))["version"])')"
APP="$ROOT/src-tauri/target/$TARGET/release/bundle/macos/$NAME.app"
[[ -d "$APP" ]] || { echo "未找到应用：$APP" >&2; exit 1; }
codesign --verify --deep --strict "$APP"
SIGNATURE="$(codesign -dv --verbose=4 "$APP" 2>&1)"
[[ "$SIGNATURE" == *flags=*runtime* && "$SIGNATURE" == *Timestamp=* ]] || {
  echo "应用缺少 Hardened Runtime 或安全时间戳" >&2; exit 1;
}
BINARY="$(/usr/libexec/PlistBuddy -c 'Print CFBundleExecutable' "$APP/Contents/Info.plist")"
ARCH="$(lipo -archs "$APP/Contents/MacOS/$BINARY")"
if [[ "$TARGET" == universal-apple-darwin ]]; then
  [[ " $ARCH " == *" arm64 "* && " $ARCH " == *" x86_64 "* ]] || {
    echo "通用包缺少 arm64 或 x86_64 架构" >&2; exit 1;
  }
fi
[[ "$ARCH" != *" "* ]] || ARCH=universal
mkdir -p "$ROOT/temp" "$ROOT/release"
WORK="$(mktemp -d "$ROOT/temp/macos-notary.XXXXXX")"
trap 'rm -rf "$WORK"; rmdir "$ROOT/temp" 2>/dev/null || true' EXIT

submit() {
  local artifact="$1" result status submission
  # 当前网络到 S3 加速端点会重置连接，使用普通上传端点。
  result="$(xcrun notarytool submit "$artifact" --keychain-profile "$PROFILE" --no-s3-acceleration --wait --output-format json)"
  printf '%s\n' "$result"
  status="$(printf '%s' "$result" | python3 -c 'import json,sys; print(json.load(sys.stdin)["status"])')"
  if [[ "$status" != Accepted ]]; then
    submission="$(printf '%s' "$result" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
    xcrun notarytool log "$submission" --keychain-profile "$PROFILE"
    return 1
  fi
}

if ! xcrun stapler validate "$APP" >/dev/null 2>&1; then
  ditto -c -k --keepParent "$APP" "$WORK/app.zip"
  submit "$WORK/app.zip"
  xcrun stapler staple "$APP"
fi
xcrun stapler validate "$APP"

# 先给应用附加票据，再制作镜像，保证离线安装时也保留应用票据。
mkdir "$WORK/stage"
ditto "$APP" "$WORK/stage/$NAME.app"
ln -s /Applications "$WORK/stage/Applications"
DMG="$WORK/${NAME}_${VERSION}_${ARCH}.dmg"
hdiutil create -volname "$NAME" -srcfolder "$WORK/stage" -format UDZO "$DMG"
codesign --force --sign "$IDENTITY" --timestamp "$DMG"
submit "$DMG"
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"
codesign --verify --deep --strict "$APP"
if command -v syspolicy_check >/dev/null 2>&1; then
  syspolicy_check distribution "$APP"
fi
spctl --assess --verbose=4 --type execute "$APP"
spctl --assess --verbose=4 --type open --context context:primary-signature "$DMG"

# 只将通过公证与校验的产物放入发布目录。
OUTPUT="$ROOT/release/$(basename "$DMG")"
mv -f "$DMG" "$OUTPUT"
ditto -c -k --keepParent "$APP" "$ROOT/release/${NAME}_${VERSION}_${ARCH}.zip"
shasum -a 256 "$OUTPUT" "$ROOT/release/${NAME}_${VERSION}_${ARCH}.zip"
printf '\n公证完成：%s\n' "$OUTPUT"
