/** The docs' languages by Starlight locale; `root` is English, served from the site root. */
export const LOCALES = {
	root: { label: "English", lang: "en" },
	"es-mx": { label: "Español", lang: "es-MX" },
} as const;

/** A Starlight locale key, as in `/es-mx/…`; `root` for the site root. */
export type LocaleKey = keyof typeof LOCALES;
