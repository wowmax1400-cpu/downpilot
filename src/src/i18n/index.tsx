// Tiny hand-rolled i18n: flat dictionary lookup + React context.
// No libraries — just `t("key")` and an `I18nProvider`.
//
// Keys live in ./en.ts (source of truth) and ./fa.ts. `t()` falls back
// to the English string when a key is missing from the active dictionary,
// so partial translations never render raw keys.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { en } from "./en";

export type Lang = "en" | "fa";

/** One flat dictionary of UI strings. fa mirrors this shape. */
export type Dict = typeof en;

const STORAGE_KEY = "dp.lang";
export const LANGS: { code: Lang; nativeName: string; dir: "ltr" | "rtl" }[] = [
  { code: "en", nativeName: "English", dir: "ltr" },
  { code: "fa", nativeName: "فارسی", dir: "rtl" },
];

const DICTS: Record<Lang, Partial<Dict>> = {
  en,
  fa: {}, // replaced below via import — kept here so the type stays honest
};

import { fa } from "./fa";
DICTS.fa = fa;

/** Load the saved language (localStorage; defaults to English). */
export function initialLang(): Lang {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    return v === "fa" ? "fa" : "en";
  } catch {
    return "en";
  }
}

type I18nValue = {
  lang: Lang;
  dir: "ltr" | "rtl";
  /** Translate a key; missing keys fall back to English, then the key itself. */
  t: (key: keyof Dict) => string;
  setLang: (lang: Lang) => void;
};

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(initialLang);

  const dir: "ltr" | "rtl" = lang === "fa" ? "rtl" : "ltr";

  // Mirror the language onto <html lang> / <html dir> so native
  // form controls, scrollbars and text layout follow along.
  useEffect(() => {
    const el = document.documentElement;
    el.lang = lang;
    el.dir = dir;
  }, [lang, dir]);

  const setLang = useCallback((next: Lang) => {
    setLangState(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* private mode — session-only language */
    }
  }, []);

  const t = useCallback(
    (key: keyof Dict) => {
      const d = DICTS[lang] as Partial<Dict>;
      return d[key] ?? en[key] ?? String(key);
    },
    [lang]
  );

  const value = useMemo<I18nValue>(() => ({ lang, dir, t, setLang }), [lang, dir, t, setLang]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

/** Use inside any component under <I18nProvider>. */
export function useI18n(): I18nValue {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n must be used inside <I18nProvider>");
  return ctx;
}
