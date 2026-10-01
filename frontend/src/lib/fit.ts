import type { Fit } from "./api";

// Не влезает ли реплика в слот: по отчёту рендера её текста, без него — по прогнозу (считает сервер).
export const fitOver = (fit: Fit | null | undefined): boolean => fit?.over === true;
