export type GenderKey = "male" | "female" | "unknown";

export const GENDER_LABEL_KEY = {
  male: "casting.gender.male",
  female: "casting.gender.female",
  unknown: "casting.gender.unknown",
} as const satisfies Record<GenderKey, `casting.gender.${GenderKey}`>;

export function genderKey(raw: string): GenderKey {
  return raw === "male" || raw === "female" ? raw : "unknown";
}
