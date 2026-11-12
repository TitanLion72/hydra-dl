/* Hydra DL — yerelleştirme motoru (**tek kaynak**, saf).
 *
 * Tasarım kararı: **anahtar = Türkçe metnin kendisi**. Neden:
 * - Mevcut kod/HTML Türkçe; her metne ayrı bir `add.button` anahtarı uydurmak
 *   yüzlerce dokunma noktası ve "hangi anahtar nerede" yükü demekti.
 * - Eksik çeviri olduğunda anahtar Türkçe olarak görünür: arayüz **asla boş**
 *   ya da `missing.key` göstermez.
 * - Envanter testi (`tools/test-i18n.mjs`) her Türkçe metnin sözlükte olduğunu
 *   doğrular, böylece "yarı Türkçe yarı İngilizce" sessizce oluşamaz.
 *
 * `%1`..`%9` yer tutucuları desteklenir; dinamik metinler `tt` etiketli şablonla
 * yazılır:
 *   tt`İndiriliyor… %1 (%2 / %3)`
 * `tt` değişkenleri `%n` biçimine çevirip sözlüğe bakar, sonra geri yerleştirir.
 */

import { EN } from "./i18n-en.js";

/** Desteklenen diller; ilki varsayılan. */
export const LANGUAGES = ["tr", "en"];

/** Arayüzde dil seçiminde gösterilecek adlar (kendi dilinde). */
export const LANGUAGE_NAMES = { tr: "Türkçe", en: "English" };

/** Sözlükler. `tr` boştur: anahtar zaten Türkçe. */
const DICTIONARIES = { tr: {}, en: EN };

let current = "tr";

/** Geçersiz dil isteğini varsayılana indirger. */
export function normalizeLanguage(value) {
  const text = String(value ?? "").trim().toLowerCase();
  // "en-US", "en_GB" gibi değerleri de kabul et.
  const base = text.split(/[-_]/)[0];
  return LANGUAGES.includes(base) ? base : "tr";
}

export function getLanguage() {
  return current;
}

export function setLanguage(language) {
  current = normalizeLanguage(language);
  return current;
}

/**
 * Metni çevirir.
 *
 * @param {string} key Türkçe kaynak metin.
 * @param {Array<string|number>} [vars] `%1`..`%9` yerine konacak değerler.
 */
export function t(key, vars) {
  const source = String(key ?? "");
  if (!source) return source;
  const dict = DICTIONARIES[current] ?? {};
  const translated = Object.prototype.hasOwnProperty.call(dict, source) ? dict[source] : source;
  const filled = Array.isArray(vars) && vars.length > 0 ? fill(translated, vars) : translated;
  // `%%` → literal `%` (yerleştirme sonrası; yüzde işareti içeren metinler için).
  return filled.replace(/%%/g, "%");
}

/**
 * `%1`..`%9` yer tutucularını doldurur.
 *
 * `%%` **literal yüzde** demektir (printf kuralı): "… %1 (%2 / %3)" gibi
 * metinlerde hem yüzde işareti hem yer tutucu birlikte bulunabilsin.
 */
function fill(template, vars) {
  const MARK = "\u0000";
  return template
    .replace(/%%/g, MARK)
    .replace(/%(\d)/g, (match, index) => {
      const value = vars[Number(index) - 1];
      return value === undefined || value === null ? match : String(value);
    })
    .replace(new RegExp(MARK, "g"), "%");
}

/** Etiketli şablon: `tt\`${n} dosya seçildi\`` → çevir + değerleri yerleştir. */
export function tt(strings, ...values) {
  // Şablonu `%1`, `%2` … biçimine çevir (anahtar bu biçimde saklanır).
  let key = "";
  strings.forEach((part, index) => {
    key += part;
    if (index < values.length) key += `%${index + 1}`;
  });
  return t(key, values);
}

/** Bu metnin çevirisi var mı? (Envanter testi ve hata ayıklama için.) */
export function hasTranslation(key, language = "en") {
  const dict = DICTIONARIES[normalizeLanguage(language)] ?? {};
  return Object.prototype.hasOwnProperty.call(dict, String(key ?? ""));
}

/** Bir dildeki sözlük (salt okunur kopya; testler için). */
export function dictionary(language) {
  return { ...(DICTIONARIES[normalizeLanguage(language)] ?? {}) };
}

// ---------------------------------------------------------------------------
// DOM yerelleştirme
// ---------------------------------------------------------------------------

/** Şablon kovası anahtarı için kaç karakter kullanılır. */
const TEMPLATE_PREFIX = 14;

/**
 * Çevrilmemiş **orijinal** metinler.
 *
 * Neden gerekli: dil değişimi sayfa yenilemeden yapılıyor. İngilizce metin DOM'a
 * yazıldıktan sonra Türkçe'ye dönüldüğünde kaynak metin kaybolmuş olurdu. Bu
 * yüzden her düğümün ilk hâli saklanır ve çeviri **her zaman orijinalden** yapılır.
 */
const originalText = new WeakMap();
/** Öğe başına orijinal öznitelik değerleri (`placeholder`, `title`, …). */
const originalAttrs = new WeakMap();
/** `localizeDom`'un en son yazdığı değer: dışarıdan değişikliği ayırt eder. */
const lastLocalized = new WeakMap();

/** Dil başına derlenmiş şablonlar (ilk çağrıda kurulur). */
const templateCache = new Map();

/**
 * `%1` içeren sözlük anahtarlarını regex'e çevirir ve prefix kovalarına böler.
 *
 * Neden kova: her metin düğümü için 100+ regex denemek 300 satırlık listede
 * her render'da pahalı olurdu. Metnin ilk 14 karakteri hangi kovaya düşeceğini
 * söyler; yalnız o kovadaki birkaç şablon denenir.
 */
function templatesFor(language) {
  const cached = templateCache.get(language);
  if (cached) return cached;

  const dict = DICTIONARIES[language] ?? {};
  const buckets = new Map();
  for (const [key, english] of Object.entries(dict)) {
    if (!/%\d/.test(key)) continue;
    // Kaçış + yer tutucu → yakalama grubu.
    const pattern = key
      .split(/(%\d)/)
      .map((part) => {
        if (/^%\d$/.test(part)) return "(.+?)";
        return part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      })
      .join("");
    const template = { regex: new RegExp(`^${pattern}$`), english };
    const prefix = key.slice(0, TEMPLATE_PREFIX);
    if (!buckets.has(prefix)) buckets.set(prefix, []);
    buckets.get(prefix).push(template);
  }
  templateCache.set(language, buckets);
  return buckets;
}

/**
 * Statik HTML metinlerini çevirir.
 *
 * Neden DOM taraması: HTML'de 150+ metne `data-i18n` eklemek yerine, metin
 * düğümlerini sözlükte arıyoruz. Anahtar = Türkçe metin olduğu için eşleşme
 * doğal. Sözlükte olmayan metne (dosya adı, URL, sayı) dokunulmaz.
 *
 * `%1`..`%9` içeren sözlük anahtarları **şablon** sayılır: metinle regex ile
 * eşleştirilir, yakalanan değerler İngilizce şablona yerleştirilir. Böylece
 * kodda her dinamik metni `tt` ile sarmalamak gerekmez (mevcut `textContent`
 * atamaları olduğu gibi çalışır).
 *
 * `data-i18n-skip` işaretli alt ağaç atlanır (kullanıcı içeriği, dosya adları).
 */
export function localizeDom(root = document) {
  if (!root || typeof document === "undefined") return;
  const dict = DICTIONARIES[current] ?? {};
  // Türkçe'de sözlük boş; yine de tarama yapılır çünkü **İngilizce'den geri
  // dönüş** orijinal metinlerin geri yazılmasını gerektirir (aşağıdaki WeakMap).

  const templates = templatesFor(current);

  /** Tek bir metni çevirir; eşleşme yoksa aynen döner. */
  const translateText = (text) => {
    const trimmed = text.trim();
    if (!trimmed) return text;
    let translated = null;
    if (Object.prototype.hasOwnProperty.call(dict, trimmed)) {
      translated = dict[trimmed];
    } else if (trimmed.length >= 4) {
      // Şablon eşleşmesi: prefix kovası ile sınırlı deneme (hızlı).
      const bucket = templates.get(trimmed.slice(0, TEMPLATE_PREFIX));
      if (bucket) {
        for (const template of bucket) {
          const match = trimmed.match(template.regex);
          if (!match) continue;
          translated = template.english.replace(/%(\d)/g, (whole, index) => {
            const value = match[Number(index)];
            return value === undefined ? whole : value;
          });
          // `%%` → literal yüzde (printf kuralı; şablonlarda `%%` yazılır).
          translated = translated.replace(/%%/g, "%");
          break;
        }
      }
    }
    if (translated === null) return text;
    // Baştaki/sondaki boşlukları koru (satır içi düzen bozulmasın).
    const leading = text.slice(0, text.indexOf(trimmed));
    const trailing = text.slice(text.indexOf(trimmed) + trimmed.length);
    return leading + translated + trailing;
  };

  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement;
      if (!parent) return NodeFilter.FILTER_REJECT;
      const tag = parent.tagName;
      // Betik/stil içeriği ve kullanıcı verisi çevrilmez.
      if (tag === "SCRIPT" || tag === "STYLE" || tag === "TEXTAREA") return NodeFilter.FILTER_REJECT;
      if (parent.closest("[data-i18n-skip]")) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  const nodes = [];
  while (walker.nextNode()) nodes.push(walker.currentNode);
  for (const node of nodes) {
    const value = node.nodeValue ?? "";
    // Dışarıdan (render/atama) değiştiyse orijinali tazele; aksi hâlde eski
    // İngilizce metin kaynak sanılırdı.
    if (lastLocalized.has(node) && lastLocalized.get(node) !== value) {
      originalText.set(node, value);
    }
    let original = originalText.get(node);
    if (original === undefined) {
      original = value;
      originalText.set(node, original);
    }
    const next = translateText(original);
    if (next !== value) {
      node.nodeValue = next;
      lastLocalized.set(node, next);
    } else {
      lastLocalized.set(node, value);
    }
  }

  // Öznitelikler: placeholder, title, aria-label, alt.
  if (typeof root.querySelectorAll === "function") {
    for (const element of root.querySelectorAll("[placeholder],[title],[aria-label],[alt]")) {
      if (element.closest("[data-i18n-skip]")) continue;
      let originals = originalAttrs.get(element);
      if (!originals) {
        originals = {};
        originalAttrs.set(element, originals);
      }
      let written = lastLocalized.get(element);
      if (!written) {
        written = {};
        lastLocalized.set(element, written);
      }
      for (const attribute of ["placeholder", "title", "aria-label", "alt"]) {
        const value = element.getAttribute(attribute);
        if (value === null) continue;
        // Dışarıdan değişen özniteliği yeni orijinal kabul et.
        if (Object.prototype.hasOwnProperty.call(written, attribute) && written[attribute] !== value) {
          originals[attribute] = value;
        }
        if (!Object.prototype.hasOwnProperty.call(originals, attribute)) {
          originals[attribute] = value;
        }
        const source = originals[attribute];
        if (!source) continue;
        const next = translateText(source);
        if (next !== value) element.setAttribute(attribute, next);
        written[attribute] = next;
      }
    }
  }
}
