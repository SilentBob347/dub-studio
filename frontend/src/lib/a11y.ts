/** The name of a control that shows only an icon: read by screen readers and by an agent's ui_read_page, shown as its tooltip. */
export function named(text: string): { "aria-label": string; title: string } {
  return { "aria-label": text, title: text };
}
