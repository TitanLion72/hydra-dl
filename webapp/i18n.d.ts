/**
 * `webapp/i18n.js` için tip bildirimleri.
 *
 * Motor saf JavaScript (telefon arayüzü derleme adımı olmadan kullanır); masaüstü
 * TypeScript tarafı aynı dosyayı import ediyor. Tip güvenliği için burada
 * imzaları tanımlıyoruz.
 */

export type Language = "tr" | "en";

export const LANGUAGES: Language[];
export const LANGUAGE_NAMES: Record<Language, string>;

export function normalizeLanguage(value: unknown): Language;
export function getLanguage(): Language;
export function setLanguage(language: string): Language;

export function t(key: string, vars?: Array<string | number>): string;
export function tt(strings: TemplateStringsArray, ...values: Array<string | number>): string;
export function hasTranslation(key: string, language?: string): boolean;
export function dictionary(language: string): Record<string, string>;
export function localizeDom(root?: Document | Element | null): void;
