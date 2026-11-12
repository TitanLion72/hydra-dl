/* Yerelleştirme köprüsü — **tek kaynak** `webapp/i18n.js`.
 *
 * Masaüstü (Vite) ve telefon (native ES modülü) aynı motoru ve aynı sözlüğü
 * kullanır; iki ayrı çeviri kopyası tutmuyoruz.
 */

// Anahtar = Türkçe metin; tipler `webapp/i18n.d.ts` içinde.
import {
  LANGUAGES as sharedLanguages,
  LANGUAGE_NAMES as sharedNames,
  getLanguage as sharedGet,
  setLanguage as sharedSet,
  normalizeLanguage as sharedNormalize,
  localizeDom as sharedLocalize,
  t as sharedT,
  tt as sharedTt,
  hasTranslation as sharedHas,
} from "../webapp/i18n.js";

export type Language = "tr" | "en";

export const LANGUAGES = sharedLanguages as Language[];
export const LANGUAGE_NAMES = sharedNames as Record<Language, string>;

export function getLanguage(): Language {
  return sharedGet() as Language;
}

export function setLanguage(language: string): Language {
  return sharedSet(language) as Language;
}

export function normalizeLanguage(value: unknown): Language {
  return sharedNormalize(value) as Language;
}

/** Metni çevirir; `%1`..`%9` yer tutucuları `vars` ile dolar. */
export function t(key: string, vars?: Array<string | number>): string {
  return sharedT(key, vars) as string;
}

/** Etiketli şablon: `` tt`${count} kayıt seçili` `` */
export function tt(strings: TemplateStringsArray, ...values: Array<string | number>): string {
  return sharedTt(strings, ...values) as string;
}

export function hasTranslation(key: string, language?: string): boolean {
  return sharedHas(key, language) as boolean;
}

/** DOM'daki statik metinleri (ve placeholder/title) çevirir. */
export function localizeDom(root?: Element | Document | null): void {
  sharedLocalize((root ?? document) as Document);
}
