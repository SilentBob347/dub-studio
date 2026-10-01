import i18n from "./i18n";

// human byte size for the setup component list and download progress, in the window's language
export function fmtBytes(n: number) {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} ${i18n.t("units.gb")}`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(0)} ${i18n.t("units.mb")}`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(0)} ${i18n.t("units.kb")}`;
  return `${n} ${i18n.t("units.b")}`;
}
