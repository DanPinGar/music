import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import es from "./locales/es.json";

export const LANGUAGES = ["es", "en"] as const;
export type Language = (typeof LANGUAGES)[number];

declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "translation";
    resources: { translation: typeof en };
  }
}

const STORAGE_KEY = "segno.language";

function isLanguage(value: unknown): value is Language {
  return LANGUAGES.some((language) => language === value);
}

function storedLanguage(): Language | undefined {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return isLanguage(value) ? value : undefined;
  } catch {
    return undefined;
  }
}

function initialLanguage(): Language {
  return storedLanguage() ?? (navigator.language.toLowerCase().startsWith("es") ? "es" : "en");
}

export function currentLanguage(): Language {
  return isLanguage(i18n.language) ? i18n.language : "en";
}

export function setLanguage(language: Language): void {
  void i18n.changeLanguage(language);
  document.documentElement.lang = language;
  try {
    localStorage.setItem(STORAGE_KEY, language);
  } catch {
    // Storage unavailable: the choice lasts for this session only.
  }
}

const language = initialLanguage();
document.documentElement.lang = language;

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, es: { translation: es } },
  lng: language,
  fallbackLng: "en",
  // React already escapes rendered text.
  interpolation: { escapeValue: false },
});

export default i18n;
