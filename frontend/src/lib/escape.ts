import { useEffect, useLayoutEffect, useRef } from "react";

// Открытые окна в порядке открытия: Escape достаётся только последнему (подтверждение внутри настроек
// закрывает себя, а не настройки).
const layers: { close: () => void }[] = [];

function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape" || layers.length === 0) return;
  e.stopPropagation();
  e.preventDefault();
  layers[layers.length - 1].close();
}

/** While mounted and `active`, Escape runs `onEscape` when this is the window opened last. */
export function useEscapeLayer(onEscape: () => void, active = true): void {
  const latest = useRef(onEscape);
  useLayoutEffect(() => {
    latest.current = onEscape;
  });
  useEffect(() => {
    if (!active) return;
    const layer = { close: () => latest.current() };
    if (layers.length === 0) window.addEventListener("keydown", onKey, true);
    layers.push(layer);
    return () => {
      layers.splice(layers.indexOf(layer), 1);
      if (layers.length === 0) window.removeEventListener("keydown", onKey, true);
    };
  }, [active]);
}
