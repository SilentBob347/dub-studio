import { useState } from "react";
import type { ProjectListing } from "./api";

// Порядок списка проектов. newest/oldest — по созданию, edited — по последней правке.
export type ProjectOrder = "newest" | "oldest" | "edited" | "nameAsc" | "nameDesc" | "longest" | "shortest";

export const PROJECT_ORDERS: ProjectOrder[] = ["newest", "oldest", "edited", "nameAsc", "nameDesc", "longest", "shortest"];

type Orderable = Pick<ProjectListing, "video" | "created" | "mtime" | "duration">;

// Проект без времени создания (ФС его не хранит) считается самым старым.
const createdOf = (p: Orderable) => p.created ?? 0;

export function compareProjects(order: ProjectOrder, language: string): (a: Orderable, b: Orderable) => number {
  const names = new Intl.Collator(language, { numeric: true, sensitivity: "base" });
  const made = (a: Orderable, b: Orderable) => createdOf(a) - createdOf(b);
  switch (order) {
    case "oldest": return made;
    case "edited": return (a, b) => b.mtime - a.mtime || made(b, a);
    case "nameAsc": return (a, b) => names.compare(a.video, b.video) || made(b, a);
    case "nameDesc": return (a, b) => names.compare(b.video, a.video) || made(b, a);
    case "longest": return (a, b) => b.duration - a.duration || made(b, a);
    case "shortest": return (a, b) => a.duration - b.duration || made(b, a);
    case "newest": return (a, b) => made(b, a);
  }
}

export type ProjectStatusFilter = "all" | "done" | "pending";

export type ProjectFilter = {
  query: string;
  modes: ReadonlySet<string>;   // пусто = любой режим
  langs: ReadonlySet<string>;   // пусто = любой язык перевода
  status: ProjectStatusFilter;
};

export const NO_FILTER: ProjectFilter = { query: "", modes: new Set(), langs: new Set(), status: "all" };

// Поиск — по имени исходного файла, коду языка перевода и режиму; без учёта регистра.
export function matchesProject(p: ProjectListing, f: ProjectFilter, language: string): boolean {
  if (f.modes.size && !f.modes.has(p.mode)) return false;
  if (f.langs.size && !f.langs.has(p.tgt_lang)) return false;
  if (f.status === "done" && !p.done) return false;
  if (f.status === "pending" && p.done) return false;
  const q = f.query.trim().toLocaleLowerCase(language);
  if (!q) return true;
  return [p.video, p.tgt_lang, p.mode].some((field) => field.toLocaleLowerCase(language).includes(q));
}

export function selectProjects(list: ProjectListing[], f: ProjectFilter, order: ProjectOrder, language: string): ProjectListing[] {
  return list.filter((p) => matchesProject(p, f, language)).sort(compareProjects(order, language));
}

// Значения фасета (режимы, языки) с числом проектов, по убыванию частоты.
export function facet(list: ProjectListing[], key: "mode" | "tgt_lang"): { value: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const p of list) counts.set(p[key], (counts.get(p[key]) ?? 0) + 1);
  return [...counts].map(([value, count]) => ({ value, count })).sort((a, b) => b.count - a.count || a.value.localeCompare(b.value));
}

// Порядок, в котором список читали последним; это вид окна, поэтому он хранится в localStorage окна.
export function useListOrder(list: string, fallback: ProjectOrder, allowed: readonly ProjectOrder[]): [ProjectOrder, (order: ProjectOrder) => void] {
  const key = `dub.order.${list}`;
  const [order, setOrder] = useState<ProjectOrder>(() => {
    const kept = localStorage.getItem(key);
    return allowed.find((o) => o === kept) ?? fallback;
  });
  const choose = (next: ProjectOrder) => {
    setOrder(next);
    localStorage.setItem(key, next);
  };
  return [order, choose];
}
