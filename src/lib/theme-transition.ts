export interface ThemeOrigin {
  x: number;
  y: number;
}

interface ThemeTransition {
  ready: Promise<void>;
  finished: Promise<void>;
}

// 旧版桌面 WebView 没有 View Transition，用静态副本保留切换前后的画面。
function captureTheme() {
  const source = document.getElementById("root");
  if (!source) return null;
  const layer = document.createElement("div");
  layer.className = document.documentElement.className;
  layer.setAttribute("aria-hidden", "true");
  layer.inert = true;
  layer.style.cssText =
    "position:fixed;inset:0;z-index:2147483646;pointer-events:none;overflow:hidden;background:hsl(var(--background));color:hsl(var(--foreground))";
  const style = getComputedStyle(document.documentElement);
  for (const property of Array.from(style)) {
    if (property.startsWith("--")) {
      layer.style.setProperty(property, style.getPropertyValue(property));
    }
  }
  const copy = source.cloneNode(true) as HTMLElement;
  layer.append(copy);
  document.body.append(layer);
  const originals = [source, ...source.querySelectorAll<HTMLElement>("*")];
  const copies = [copy, ...copy.querySelectorAll<HTMLElement>("*")];
  copies.forEach((element, index) => {
    // 固定副本的配色，避免 html 切换后旧画面被 dark 选择器重新染色。
    const colors = getComputedStyle(originals[index]);
    for (const property of [
      "color",
      "background-color",
      "background-image",
      "border-color",
      "box-shadow",
      "fill",
      "stroke",
      "color-scheme",
    ]) {
      element.style.setProperty(
        property,
        colors.getPropertyValue(property),
        "important",
      );
    }
    element.removeAttribute("id");
    element.scrollTop = originals[index].scrollTop;
    element.scrollLeft = originals[index].scrollLeft;
  });
  return layer;
}

export async function transitionTheme(apply: () => void, origin?: ThemeOrigin) {
  if (
    !origin ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  ) {
    apply();
    return;
  }
  const { x, y } = origin;
  const radius = Math.hypot(
    Math.max(x, window.innerWidth - x),
    Math.max(y, window.innerHeight - y),
  );
  const frames = {
    clipPath: [
      `circle(0px at ${x}px ${y}px)`,
      `circle(${radius}px at ${x}px ${y}px)`,
    ],
  };
  const timing = {
    duration: 520,
    easing: "cubic-bezier(0.22, 1, 0.36, 1)",
    fill: "both" as const,
  };
  const viewDocument = document as Document & {
    startViewTransition?: (update: () => void) => ThemeTransition;
  };

  if (viewDocument.startViewTransition) {
    const transition = viewDocument.startViewTransition(apply);
    try {
      await transition.ready;
      await document.documentElement.animate(frames, {
        ...timing,
        pseudoElement: "::view-transition-new(root)",
      }).finished;
    } catch {
      // 窗口隐藏或动画取消时仍保留已选择的主题。
    }
    await transition.finished.catch(() => undefined);
    return;
  }

  const previous = captureTheme();
  let next: HTMLElement | null = null;
  try {
    apply();
    if (!previous) return;
    next = captureTheme();
    if (next) await next.animate(frames, timing).finished;
  } catch {
    // 动画不可用时直接显示新主题。
  } finally {
    previous?.remove();
    next?.remove();
  }
}
