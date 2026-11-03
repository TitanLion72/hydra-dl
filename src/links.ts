/* Paylaşılan bağlantı ayrıştırıcısı — **tek kaynak** `webapp/links.js`.
 *
 * Telefon arayüzü aynı dosyayı `<script type="module">` ile yükler; masaüstü
 * Vite üzerinden buradan içe aktarır. Böylece "toplu içe aktarma" davranışı iki
 * arayüzde ayrışmaz.
 */

// @ts-ignore — saf JS modülü; tipini aşağıdaki arayüzle veriyoruz.
import { parseLinkList as sharedParse, MAX_LINKS as sharedMax } from "../webapp/links.js";

export interface LinkList {
  /** Geçerli, yinelenmemiş bağlantılar (sıra korunur). */
  links: string[];
  /** Bağlantı içermeyen / desteklenmeyen satırlar (ilk 160 karakter). */
  rejected: string[];
  /** Yinelenen olduğu için atlananlar. */
  duplicates: number;
  /** `MAX_LINKS` sınırını aşanlar. */
  truncated: number;
}

export const parseLinkList = sharedParse as (text: string) => LinkList;
export const MAX_LINKS = sharedMax as number;
